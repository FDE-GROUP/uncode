use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent, UsageInfo};
use crate::providers::common::{build_chat_messages, map_http_error};
use uncode_core::error::UncodeError;

pub struct OpenAiDriver {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAiDriver {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.openai.com/v1".into(),
        }
    }
}

fn build_body(request: &CompletionRequest) -> Value {
    let messages = build_chat_messages(request);
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
    body
}

#[async_trait]
impl LlmDriver for OpenAiDriver {
    fn provider_name(&self) -> &'static str {
        "openai"
    }
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&build_body(&request))
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(map_http_error(
                response.status(),
                response.text().await.unwrap_or_default(),
            ));
        }

        let stream = response
            .bytes_stream()
            .flat_map(|chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_sse(&String::from_utf8_lossy(&c)),
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                stream::iter(events)
            })
            .chain(stream::once(async { StreamEvent::Done }));
        Ok(Box::pin(stream))
    }
}

fn parse_sse(text: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "data: [DONE]" {
            continue;
        }
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                if let Some(choice) = event["choices"][0].as_object() {
                    if let Some(delta) = choice.get("delta") {
                        if let Some(content) = delta["content"].as_str() {
                            if !content.is_empty() {
                                events.push(StreamEvent::TextDelta(content.to_string()));
                            }
                        }
                    }
                }
                if let Some(usage) = event.get("usage") {
                    events.push(StreamEvent::Usage(UsageInfo {
                        input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
                        output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
                    }));
                }
            }
        }
    }
    events
}
