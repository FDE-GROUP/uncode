use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

use crate::api::Api;
use crate::api::{StreamEvent, UsageInfo};
use crate::api_types::{Context, StopReason, StreamOptions, ThinkingLevel};
use crate::message::{ContentBlock, Role};
use crate::model::Model;
use uncode_shared::error::UncodeError;

pub struct AnthropicMessagesApi {
    client: Client,
}

impl AnthropicMessagesApi {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for AnthropicMessagesApi {
    fn default() -> Self {
        Self::new()
    }
}

fn build_anthropic_body(model: &Model, context: &Context, options: &StreamOptions) -> Value {
    let messages: Vec<Value> = context
        .messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|msg| match msg.role {
            Role::Assistant => {
                let mut text_parts: Vec<String> = Vec::new();
                let mut tool_calls: Vec<Value> = Vec::new();
                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => text_parts.push(text.clone()),
                        ContentBlock::ToolCall(tc) => {
                            tool_calls.push(serde_json::json!({
                                "id": tc.id,
                                "type": "tool_use",
                                "name": tc.name,
                                "input": tc.arguments
                            }));
                        }
                        _ => {}
                    }
                }
                let mut m = serde_json::json!({
                    "role": "assistant",
                    "content": if text_parts.is_empty() && tool_calls.is_empty() {
                        Value::Array(vec![])
                    } else {
                        let mut blocks: Vec<Value> = text_parts
                            .iter()
                            .map(|t| serde_json::json!({"type": "text", "text": t}))
                            .collect();
                        blocks.extend(tool_calls);
                        Value::Array(blocks)
                    }
                });
                let _ = &mut m;
                m
            }
            Role::Tool => {
                let mut content: Vec<Value> = Vec::new();
                for block in &msg.content {
                    if let ContentBlock::ToolResult(tr) = block {
                        content.push(serde_json::json!({
                            "type": if tr.is_error { "tool_result" } else { "tool_result" },
                            "tool_use_id": tr.tool_call_id,
                            "content": tr.content
                        }));
                    }
                }
                serde_json::json!({
                    "role": "user",
                    "content": content
                })
            }
            _ => {
                let text = msg
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.clone()),
                        ContentBlock::Image { mime_type, data } => Some(
                            serde_json::json!({
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": mime_type,
                                    "data": data
                                }
                            })
                            .to_string(),
                        ),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                serde_json::json!({
                    "role": msg.role.to_string(),
                    "content": text
                })
            }
        })
        .collect();

    let mut body = serde_json::json!({
        "model": model.id,
        "messages": messages,
        "max_tokens": options.max_tokens.unwrap_or(4096),
        "stream": true,
    });

    if let Some(ref system) = context.system_prompt {
        body["system"] = serde_json::json!(system);
    }
    if let Some(t) = options.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    if !context.tools.is_empty() {
        let tools: Vec<Value> = context
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

    // Thinking parameters for Anthropic extended thinking
    if model.reasoning {
        let level = options.thinking_level.unwrap_or(ThinkingLevel::Off);
        if level != ThinkingLevel::Off {
            let mapped = model
                .thinking_level_map
                .get(&level)
                .and_then(|v| v.as_deref());

            let effort = mapped.unwrap_or(match level {
                ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High | ThinkingLevel::XHigh => "high",
                _ => "high",
            });

            let budget = options.thinking_budget_tokens.unwrap_or(10000);
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget
            });
            // effort is only used by models that support adaptive thinking
            let _ = effort;
        }
    }

    body
}

struct AnthropicToolState {
    active_tools: HashMap<usize, (String, String)>,
    pending_args: HashMap<usize, String>,
}

impl AnthropicToolState {
    fn new() -> Self {
        Self {
            active_tools: HashMap::new(),
            pending_args: HashMap::new(),
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
                                    events.push(StreamEvent::Error {
                                        reason: crate::api_types::StopReason::Error,
                                        message: format!("tool args JSON parse failed: {e}"),
                                    });
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
                        if let Some(stop) = event["delta"]["stop_reason"].as_str() {
                            let reason = match stop {
                                "end_turn" | "stop_sequence" => StopReason::Stop,
                                "tool_use" => StopReason::ToolUse,
                                "max_tokens" => StopReason::Length,
                                _ => StopReason::Stop,
                            };
                            events.push(StreamEvent::Done { reason });
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

#[async_trait]
impl Api for AnthropicMessagesApi {
    fn api_name(&self) -> &'static str {
        "anthropic-messages"
    }

    async fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let body = build_anthropic_body(model, context, options);
        let url = format!("{}/messages", model.base_url);

        let mut req = self.client.post(&url).json(&body);

        if let Some(ref key) = options.api_key {
            req = req.header("x-api-key", key);
        }
        req = req
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json");

        for (k, v) in &model.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let response = req
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(match status.as_u16() {
                401 | 403 => UncodeError::LlmAuth(response.text().await.unwrap_or_default()),
                429 => UncodeError::LlmRateLimit(response.text().await.unwrap_or_default()),
                _ => UncodeError::Llm(format!(
                    "HTTP {status}: {}",
                    response.text().await.unwrap_or_default()
                )),
            });
        }

        let state = AnthropicToolState::new();
        let stream = response
            .bytes_stream()
            .scan(state, |state, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_anthropic_chunk(&String::from_utf8_lossy(&c), state),
                    Err(e) => vec![StreamEvent::Error {
                        reason: crate::api_types::StopReason::Error,
                        message: e.to_string(),
                    }],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once(async {
                StreamEvent::Done {
                    reason: StopReason::Stop,
                }
            }));

        Ok(Box::pin(stream))
    }
}
