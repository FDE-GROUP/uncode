use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent};
use crate::providers::common::{build_chat_messages, map_http_error};
use uncode_core::error::UncodeError;

pub struct OpenRouterDriver {
    client: Client,
    api_key: String,
}

impl OpenRouterDriver {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

#[async_trait]
impl LlmDriver for OpenRouterDriver {
    fn provider_name(&self) -> &'static str {
        "openrouter"
    }
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let messages = build_chat_messages(&request);
        let mut body =
            serde_json::json!({ "model": request.model, "messages": messages, "stream": true });
        if let Some(mt) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(mt);
        }

        let response = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://github.com/FDE-GROUP/uncode")
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
            }
        }
    }
    events
}
