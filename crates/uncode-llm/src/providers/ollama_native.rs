use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

use crate::api::Api;
use crate::api::StreamEvent;
use uncode_core::api_types::{Context, StreamOptions};
use uncode_core::error::UncodeError;
use uncode_core::message::{ContentBlock, Role};
use uncode_core::model::Model;
use uncode_core::tool::ToolDefinition;

pub struct OllamaNativeApi {
    client: Client,
}

impl OllamaNativeApi {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for OllamaNativeApi {
    fn default() -> Self {
        Self::new()
    }
}

fn build_chat_messages(context: &Context) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(ref system) = context.system_prompt {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system
        }));
    }

    for msg in &context.messages {
        if msg.role == Role::System {
            continue;
        }
        match msg.role {
            Role::Assistant => {
                let mut text_parts: Vec<String> = Vec::new();
                let mut tool_calls: Vec<Value> = Vec::new();

                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => text_parts.push(text.clone()),
                        ContentBlock::ToolCall(tc) => {
                            tool_calls.push(serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string()
                                }
                            }));
                        }
                        _ => {}
                    }
                }

                let mut m = serde_json::json!({
                    "role": "assistant",
                    "content": if text_parts.is_empty() {
                        Value::Null
                    } else {
                        Value::String(text_parts.join("\n"))
                    }
                });
                if !tool_calls.is_empty() {
                    m["tool_calls"] = Value::Array(tool_calls);
                }
                messages.push(m);
            }
            Role::Tool => {
                for block in &msg.content {
                    if let ContentBlock::ToolResult(tr) = block {
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tr.tool_call_id,
                            "content": tr.content
                        }));
                    }
                }
            }
            _ => {
                let content = msg
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.clone()),
                        ContentBlock::Image { .. } => Some("[image]".into()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                messages.push(serde_json::json!({
                    "role": msg.role.to_string(),
                    "content": content
                }));
            }
        }
    }
    messages
}

fn build_tools_json(tools: &[ToolDefinition]) -> Option<Value> {
    if tools.is_empty() {
        return None;
    }
    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        })
        .collect();
    Some(Value::Array(tools_json))
}

fn build_ollama_body(model: &Model, context: &Context, options: &StreamOptions) -> Value {
    let messages = build_chat_messages(context);
    let mut body = serde_json::json!({
        "model": model.id,
        "messages": messages,
        "stream": true,
    });

    if let Some(t) = options.temperature {
        body["options"] = serde_json::json!({"temperature": t});
    }
    if let Some(tools) = build_tools_json(&context.tools) {
        body["tools"] = tools;
    }
    body
}

struct OllamaToolState {
    active_tools: HashMap<usize, (String, String)>,
}

impl OllamaToolState {
    fn new() -> Self {
        Self {
            active_tools: HashMap::new(),
        }
    }
}

fn parse_ollama_chunk(text: &str, state: &mut OllamaToolState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<Value>(line) {
            if let Some(content) = event["message"]["content"].as_str() {
                if !content.is_empty() {
                    events.push(StreamEvent::TextDelta(content.to_string()));
                }
            }

            if let Some(tool_calls) = event["message"]["tool_calls"].as_array() {
                for (i, tc) in tool_calls.iter().enumerate() {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    if let Some(func) = tc.get("function") {
                        let name = func["name"].as_str().unwrap_or_default().to_string();
                        let args = func["arguments"].clone();
                        if !name.is_empty() {
                            state.active_tools.insert(i, (id.clone(), name.clone()));
                            events.push(StreamEvent::ToolCallStart {
                                id: id.clone(),
                                name: name.clone(),
                            });
                            events.push(StreamEvent::ToolCallDelta {
                                id: id.clone(),
                                arguments: args.to_string(),
                            });
                            events.push(StreamEvent::ToolCallEnd {
                                id,
                                name,
                                arguments: args,
                            });
                        }
                    }
                }
            }

            if event["done"].as_bool() == Some(true) {
                if let Some(tool_calls) = event["message"]["tool_calls"].as_array() {
                    for (i, tc) in tool_calls.iter().enumerate() {
                        if state.active_tools.contains_key(&i) {
                            continue;
                        }
                        let id = tc["id"].as_str().unwrap_or("").to_string();
                        if let Some(func) = tc.get("function") {
                            let name = func["name"].as_str().unwrap_or_default().to_string();
                            let args = func["arguments"].clone();
                            if !name.is_empty() {
                                events.push(StreamEvent::ToolCallStart {
                                    id: id.clone(),
                                    name: name.clone(),
                                });
                                events.push(StreamEvent::ToolCallDelta {
                                    id: id.clone(),
                                    arguments: args.to_string(),
                                });
                                events.push(StreamEvent::ToolCallEnd {
                                    id,
                                    name,
                                    arguments: args,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    events
}

#[async_trait]
impl Api for OllamaNativeApi {
    fn api_name(&self) -> &'static str {
        "ollama-native"
    }

    async fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let body = build_ollama_body(model, context, options);
        let url = format!("{}/api/chat", model.base_url);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(UncodeError::Llm(text));
        }

        let state = OllamaToolState::new();
        let stream = response
            .bytes_stream()
            .scan(state, |state, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_ollama_chunk(&String::from_utf8_lossy(&c), state),
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once(async { StreamEvent::Done }));

        Ok(Box::pin(stream))
    }
}
