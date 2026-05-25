use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

use crate::api::{Api, StreamEvent, ToolCallEndData};
use crate::api_types::{Context, StopReason, StreamOptions};
use crate::message::{ContentBlock, Role};
use crate::model::Model;
use crate::providers::build_tools_json;
use uncode_shared::error::UncodeError;

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

    if let Some(system) = context.system_prompt.as_deref() {
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
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments
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
                            "content": tr.content,
                            "tool_call_id": tr.tool_call_id
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
            // Ollama stream error
            if let Some(err) = event["error"].as_str()
                && !err.is_empty()
            {
                events.push(StreamEvent::Error {
                    reason: crate::api_types::StopReason::Error,
                    message: err.to_string(),
                });
                continue;
            }

            if let Some(content) = event["message"]["content"].as_str()
                && !content.is_empty()
            {
                events.push(StreamEvent::TextDelta(content.to_string()));
            }

            if let Some(tool_calls) = event["message"]["tool_calls"].as_array() {
                for (i, tc) in tool_calls.iter().enumerate() {
                    let id = tc["id"].as_str().unwrap_or_default().to_string();
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
                            events.push(StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                                id,
                                name,
                                arguments: args,
                            })));
                        }
                    }
                }
            }

            if event["done"].as_bool() == Some(true) {
                let reason = match event["done_reason"].as_str() {
                    Some("length") => StopReason::Length,
                    Some("load") | Some("unload") => StopReason::Stop,
                    _ => StopReason::Stop,
                };
                events.push(StreamEvent::Done { reason });

                if let Some(tool_calls) = event["message"]["tool_calls"].as_array() {
                    for (i, tc) in tool_calls.iter().enumerate() {
                        if state.active_tools.contains_key(&i) {
                            continue;
                        }
                        let id = tc["id"].as_str().unwrap_or_default().to_string();
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
                                events.push(StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                                    id,
                                    name,
                                    arguments: args,
                                })));
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
        let mut body = build_ollama_body(model, context, options);
        crate::notify_request_payload(options, &mut body);
        let url = format!("{}/api/chat", model.base_url);

        let req = crate::apply_option_headers(self.client.post(&url).json(&body), options);
        let response = req
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        crate::notify_http_response(options, response.status().as_u16(), response.headers());

        if !response.status().is_success() {
            return Err(crate::providers::http_error_from_response(response).await);
        }

        let state = OllamaToolState::new();
        let buf = std::sync::Arc::new(std::sync::Mutex::new(
            crate::providers::NdjsonLineBuffer::new(),
        ));
        let buf2 = buf.clone();
        let stream = response
            .bytes_stream()
            .scan((state, buf), |(state, buf), chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => {
                        let mut all_events = Vec::new();
                        let mut guard = crate::safe_lock(&buf, "ollama stream buffer");
                        for line in guard.push_chunk_and_drain_lines(&c) {
                            all_events.extend(parse_ollama_chunk(&line, state));
                        }
                        all_events
                    }
                    Err(e) => vec![StreamEvent::Error {
                        reason: crate::api_types::StopReason::Error,
                        message: e.to_string(),
                    }],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once({
                async move {
                    let guard = crate::safe_lock(&buf2, "ollama stream buffer");
                    if let Some(message) = guard.trailing_error_message("ollama") {
                        return StreamEvent::Error {
                            reason: StopReason::Error,
                            message,
                        };
                    }
                    StreamEvent::Done {
                        reason: StopReason::Stop,
                    }
                }
            }));

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod stream_buffer_tests {
    use super::{OllamaToolState, parse_ollama_chunk};
    use crate::api::StreamEvent;
    use crate::providers::NdjsonLineBuffer;

    #[test]
    fn ndjson_buffer_splits_ollama_lines() {
        let mut state = OllamaToolState::new();
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(br#"{"message":{"content":"hel"#);
        assert!(buf.drain_complete_lines().is_empty());
        buf.push_chunk(b"lo\"}}\n");
        let lines = buf.drain_complete_lines();
        assert_eq!(lines.len(), 1);
        let events = parse_ollama_chunk(&lines[0], &mut state);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::TextDelta(s) if s == "hello"))
        );
    }

    #[test]
    fn trailing_incomplete_line_surfaces_error() {
        let mut partial = NdjsonLineBuffer::new();
        partial.push_chunk(br#"{"partial":true}"#);
        let msg = partial
            .trailing_error_message("ollama")
            .expect("should report incomplete");
        assert!(msg.contains("ollama"));
        assert!(msg.contains("partial"));
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;
    use crate::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
    use crate::model::Model;
    use crate::tool_def::ToolDefinition;

    #[test]
    fn empty_context() {
        let ctx = Context::default();
        let messages = build_chat_messages(&ctx);
        assert!(messages.is_empty());
    }

    #[test]
    fn system_prompt() {
        let mut ctx = Context::default();
        ctx.system_prompt = Some("Be helpful".into());
        let messages = build_chat_messages(&ctx);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "Be helpful");
    }

    #[test]
    fn user_message() {
        let mut ctx = Context::default();
        ctx.messages = vec![Message::user("hello")];
        let messages = build_chat_messages(&ctx);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "hello");
    }

    #[test]
    fn assistant_with_text_and_tool_call() {
        let mut ctx = Context::default();
        let tc = ToolCall {
            id: "call_1".into(),
            name: "get_weather".into(),
            arguments: serde_json::json!({"city": "NYC"}),
        };
        let msg = Message {
            id: "x".into(),
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Let me check".into(),
                },
                ContentBlock::ToolCall(Box::new(tc)),
            ],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        ctx.messages = vec![msg];
        let messages = build_chat_messages(&ctx);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "Let me check");
        let tool_calls = messages[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn tool_result() {
        let mut ctx = Context::default();
        let tr = ToolResult {
            tool_call_id: "call_1".into(),
            content: "sunny".into(),
            is_error: false,
        };
        let msg = Message {
            id: "x".into(),
            role: Role::Tool,
            content: vec![ContentBlock::ToolResult(Box::new(tr))],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        ctx.messages = vec![msg];
        let messages = build_chat_messages(&ctx);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "tool");
        assert_eq!(messages[0]["content"], "sunny");
        assert_eq!(messages[0]["tool_call_id"], "call_1");
    }

    #[test]
    fn skips_system_role_messages() {
        let mut ctx = Context::default();
        ctx.messages = vec![Message::system("internal"), Message::user("hello")];
        let messages = build_chat_messages(&ctx);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "hello");
    }

    #[test]
    fn body_minimal() {
        let model = Model::default();
        let mut ctx = Context::default();
        ctx.messages = vec![Message::user("hi")];
        let options = StreamOptions::default();
        let body = build_ollama_body(&model, &ctx, &options);
        assert!(body["model"].is_string());
        assert!(body["messages"].is_array());
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn body_with_temperature() {
        let model = Model::default();
        let ctx = Context::default();
        let options = StreamOptions {
            temperature: Some(0.7),
            ..Default::default()
        };
        let body = build_ollama_body(&model, &ctx, &options);
        let t = body["options"]["temperature"].as_f64().unwrap();
        assert!((t - 0.7).abs() < 0.01);
    }

    #[test]
    fn body_with_tools() {
        let model = Model::default();
        let mut ctx = Context::default();
        ctx.tools = vec![ToolDefinition {
            name: "search".into(),
            description: "search the web".into(),
            parameters: serde_json::json!({"type": "object"}),
            label: None,
            execution_mode: Default::default(),
        }];
        let options = StreamOptions::default();
        let body = build_ollama_body(&model, &ctx, &options);
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "search");
    }
}
