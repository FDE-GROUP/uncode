use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent, UsageInfo};
use crate::providers::common::{build_chat_messages, map_http_error};
use uncode_core::error::UncodeError;

pub struct AnthropicDriver {
    client: Client,
    api_key: String,
}

impl AnthropicDriver {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

fn build_body(request: &CompletionRequest) -> Value {
    let messages = build_chat_messages(request);
    let mut body = serde_json::json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_tokens.unwrap_or(4096),
        "stream": true,
    });
    if let Some(ref system) = request.system {
        body["system"] = serde_json::json!(system);
    }
    if let Some(t) = request.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    body
}

#[async_trait]
impl LlmDriver for AnthropicDriver {
    fn provider_name(&self) -> &'static str {
        "anthropic"
    }
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
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
                    Ok(c) => parse_anthropic_sse(&String::from_utf8_lossy(&c)),
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                stream::iter(events)
            })
            .chain(stream::once(async { StreamEvent::Done }));
        Ok(Box::pin(stream))
    }
}

fn parse_anthropic_sse(text: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                match event["type"].as_str() {
                    Some("content_block_delta") => {
                        if let Some(text) = event["delta"]["text"].as_str() {
                            events.push(StreamEvent::TextDelta(text.to_string()));
                        }
                    }
                    Some("message_delta") => {
                        if let Some(usage) = event.get("usage") {
                            events.push(StreamEvent::Usage(UsageInfo {
                                input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
                                output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    events
}
