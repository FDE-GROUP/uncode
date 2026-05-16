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
    let messages = build_chat_messages(request)
        .into_iter()
        .filter(|m| m["role"].as_str() != Some("system"))
        .collect::<Vec<_>>();
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
    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters
                })
            })
            .collect();
        body["tools"] = serde_json::json!(tools);
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

        let state = AnthropicToolState::new();
        let stream = response
            .bytes_stream()
            .scan(state, |state, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_anthropic_chunk(&String::from_utf8_lossy(&c), state),
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once(async { StreamEvent::Done }));
        Ok(Box::pin(stream))
    }
}

struct AnthropicToolState {
    /// tool_use block index → (id, name)
    active_tools: std::collections::HashMap<usize, (String, String)>,
    /// tool_use block index → accumulated input JSON
    pending_args: std::collections::HashMap<usize, String>,
}

impl AnthropicToolState {
    fn new() -> Self {
        Self {
            active_tools: std::collections::HashMap::new(),
            pending_args: std::collections::HashMap::new(),
        }
    }
}

fn parse_anthropic_chunk(text: &str, state: &mut AnthropicToolState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                match event["type"].as_str() {
                    Some("content_block_start") => {
                        let idx = event["index"].as_u64().unwrap_or(0) as usize;
                        if event["content_block"]["type"].as_str() == Some("tool_use") {
                            let id = event["content_block"]["id"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            let name = event["content_block"]["name"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            state.active_tools.insert(idx, (id.clone(), name.clone()));
                            events.push(StreamEvent::ToolCallStart { id, name });
                        } else if let Some(text) = event["content_block"]["text"].as_str() {
                            if !text.is_empty() {
                                events.push(StreamEvent::TextDelta(text.to_string()));
                            }
                        }
                    }
                    Some("content_block_delta") => {
                        let idx = event["index"].as_u64().unwrap_or(0) as usize;
                        if event["delta"]["type"].as_str() == Some("input_json_delta") {
                            if let Some(partial) = event["delta"]["partial_json"].as_str() {
                                if state.active_tools.contains_key(&idx) {
                                    state.pending_args.entry(idx).or_default().push_str(partial);
                                    let id = state.active_tools.get(&idx).unwrap().0.clone();
                                    events.push(StreamEvent::ToolCallDelta {
                                        id,
                                        arguments: partial.to_string(),
                                    });
                                }
                            }
                        } else if let Some(text) = event["delta"]["text"].as_str() {
                            if !text.is_empty() {
                                events.push(StreamEvent::TextDelta(text.to_string()));
                            }
                        }
                    }
                    Some("content_block_stop") => {
                        let idx = event["index"].as_u64().unwrap_or(0) as usize;
                        if let Some((id, name)) = state.active_tools.remove(&idx) {
                            let args_str = state.pending_args.remove(&idx).unwrap_or_default();
                            let parsed = match serde_json::from_str::<Value>(&args_str) {
                                Ok(v) => v,
                                Err(e) => {
                                    events.push(StreamEvent::Error(format!(
                                        "tool args JSON parse failed: {e}"
                                    )));
                                    Value::Object(Default::default())
                                }
                            };
                            events.push(StreamEvent::ToolCallEnd {
                                id,
                                name,
                                arguments: parsed,
                            });
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
                    Some("message_start") => {
                        if let Some(usage) = event["message"].get("usage") {
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
