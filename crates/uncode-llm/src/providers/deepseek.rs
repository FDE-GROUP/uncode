use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent};
use crate::providers::common::{
    OpenAiStreamState, build_chat_messages, build_tools_json, extract_usage, flush_tool_calls,
    map_http_error, parse_openai_tool_calls,
};
use uncode_core::error::UncodeError;

pub struct DeepSeekDriver {
    client: Client,
    api_key: String,
    base_url: String,
}

impl DeepSeekDriver {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.deepseek.com/v1".into(),
        }
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
        }
    }

    fn build_body(&self, request: &CompletionRequest) -> Value {
        tracing::debug!("input messages: {}", request.messages.len());
        for (i, msg) in request.messages.iter().enumerate() {
            tracing::debug!("msg[{i}]: role={:?} blocks={}", msg.role, msg.content.len());
        }
        let messages = build_chat_messages(request);
        tracing::debug!(
            "API messages: {}",
            serde_json::to_string_pretty(&messages).unwrap_or_default()
        );
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": true,
        });

        if let Some(mt) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(mt);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if let Some(tools) = build_tools_json(&request.tools) {
            body["tools"] = tools;
        }

        body
    }
}

#[async_trait]
impl LlmDriver for DeepSeekDriver {
    fn provider_name(&self) -> &'static str {
        "deepseek"
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let body = self.build_body(&request);

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(map_http_error(
                response.status(),
                response.text().await.unwrap_or_default(),
            ));
        }

        let state = OpenAiStreamState::new();
        let stream = response
            .bytes_stream()
            .scan(state, |state, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_deepseek_chunk(&String::from_utf8_lossy(&c), state),
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once(async { StreamEvent::Done }));

        Ok(Box::pin(stream))
    }
}

fn parse_deepseek_chunk(text: &str, state: &mut OpenAiStreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "data: [DONE]" {
            continue;
        }
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                if let Some(choice) = event["choices"][0].as_object() {
                    // Tool calls (shared OpenAI-compatible logic)
                    events.extend(parse_openai_tool_calls(choice, state));

                    // Reasoning/thinking content (DeepSeek reasoning models)
                    if let Some(reasoning) = choice["delta"]["reasoning_content"].as_str() {
                        if !reasoning.is_empty() {
                            events.push(StreamEvent::ThinkingDelta(reasoning.to_string()));
                        }
                    }
                    // Text delta
                    if let Some(content) = choice["delta"]["content"].as_str() {
                        if !content.is_empty() {
                            events.push(StreamEvent::TextDelta(content.to_string()));
                        }
                    }
                    // Finish reason — flush tool calls
                    if let Some(reason) = choice.get("finish_reason") {
                        if !reason.is_null() {
                            events.extend(flush_tool_calls(state));
                        }
                    }
                }
                if let Some(usage_event) = extract_usage(&event) {
                    events.push(usage_event);
                }
            }
        }
    }
    events
}
