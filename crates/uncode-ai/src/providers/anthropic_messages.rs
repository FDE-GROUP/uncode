use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

use crate::api::{Api, StreamEvent, ToolCallEndData, UsageInfo};
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
                let mut blocks: Vec<Value> = Vec::new();
                for block in &msg.content {
                    match block {
                        ContentBlock::Thinking { text } => {
                            blocks.push(serde_json::json!({
                                "type": "thinking",
                                "thinking": text
                            }));
                        }
                        ContentBlock::Text { text } => {
                            blocks.push(serde_json::json!({"type": "text", "text": text}));
                        }
                        ContentBlock::ToolCall(tc) => {
                            blocks.push(serde_json::json!({
                                "id": tc.id,
                                "type": "tool_use",
                                "name": tc.name,
                                "input": tc.arguments
                            }));
                        }
                        _ => {}
                    }
                }
                serde_json::json!({
                    "role": "assistant",
                    "content": Value::Array(blocks)
                })
            }
            Role::Tool => {
                let mut content: Vec<Value> = Vec::new();
                for block in &msg.content {
                    if let ContentBlock::ToolResult(tr) = block {
                        content.push(serde_json::json!({
                            "type": "tool_result",
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
        "max_tokens": options.max_tokens.unwrap_or(model.max_output_tokens),
        "stream": true,
    });

    if let Some(system) = context.system_prompt.as_deref() {
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

    if let Some(budget) = resolve_anthropic_thinking_budget(model, options) {
        body["thinking"] = serde_json::json!({
            "type": "enabled",
            "budget_tokens": budget
        });
    }

    body
}

/// Extended-thinking token budget from `StreamOptions` or `Model.thinking_level_map` (preset strings).
fn resolve_anthropic_thinking_budget(model: &Model, options: &StreamOptions) -> Option<u32> {
    if !model.reasoning {
        return None;
    }
    let level = options.thinking_level?;
    if level == ThinkingLevel::Off {
        return None;
    }
    if let Some(budget) = options.thinking_budget_tokens {
        return Some(budget);
    }
    model
        .thinking_level_map
        .get(&level)
        .and_then(|v| v.as_deref())
        .and_then(|s| s.parse().ok())
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
        if let Some(json_str) = line.strip_prefix("data: ")
            && let Ok(event) = serde_json::from_str::<Value>(json_str)
        {
            match event["type"].as_str() {
                Some("content_block_start") => {
                    let idx = event["index"].as_u64().unwrap_or(0) as usize;
                    let block_type = event["content_block"]["type"].as_str();
                    if block_type == Some("tool_use") {
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
                    } else if block_type == Some("thinking") {
                        if let Some(text) = event["content_block"]["thinking"].as_str()
                            && !text.is_empty()
                        {
                            events.push(StreamEvent::ThinkingDelta(text.to_string()));
                        }
                    } else if let Some(text) = event["content_block"]["text"].as_str()
                        && !text.is_empty()
                    {
                        events.push(StreamEvent::TextDelta(text.to_string()));
                    }
                }
                Some("content_block_delta") => {
                    let idx = event["index"].as_u64().unwrap_or(0) as usize;
                    let delta_type = event["delta"]["type"].as_str();
                    if delta_type == Some("thinking_delta") {
                        if let Some(text) = event["delta"]["thinking"].as_str()
                            && !text.is_empty()
                        {
                            events.push(StreamEvent::ThinkingDelta(text.to_string()));
                        }
                    } else if delta_type == Some("input_json_delta") {
                        if let Some(partial) = event["delta"]["partial_json"].as_str()
                            && state.active_tools.contains_key(&idx)
                        {
                            state.pending_args.entry(idx).or_default().push_str(partial);
                            if let Some(entry) = state.active_tools.get(&idx) {
                                events.push(StreamEvent::ToolCallDelta {
                                    id: entry.0.clone(),
                                    arguments: partial.to_string(),
                                });
                            }
                        }
                    } else if let Some(text) = event["delta"]["text"].as_str()
                        && !text.is_empty()
                    {
                        events.push(StreamEvent::TextDelta(text.to_string()));
                    }
                }
                Some("content_block_stop") => {
                    let idx = event["index"].as_u64().unwrap_or(0) as usize;
                    if let Some((id, name)) = state.active_tools.remove(&idx) {
                        let args_str = state.pending_args.remove(&idx).unwrap_or_default();
                        match serde_json::from_str::<Value>(&args_str) {
                            Ok(parsed) => {
                                events.push(StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                                    id,
                                    name,
                                    arguments: parsed,
                                })));
                            }
                            Err(e) => {
                                events.push(StreamEvent::Error {
                                    reason: crate::api_types::StopReason::Error,
                                    message: format!("tool args JSON parse failed: {e}"),
                                });
                            }
                        }
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
        let mut body = build_anthropic_body(model, context, options);
        crate::notify_request_payload(options, &mut body);
        let url = format!("{}/messages", model.base_url);

        let mut req = self.client.post(&url).json(&body);
        req = crate::apply_option_headers(req, options);

        if let Some(ref key) = options.api_key {
            req = req.header("x-api-key", key);
        }
        req = req
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json");

        for (k, v) in &model.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let send_future = req.send();
        let response = match options.timeout_ms {
            Some(ms) => tokio::time::timeout(std::time::Duration::from_millis(ms), send_future)
                .await
                .map_err(|_| UncodeError::Llm("request timed out".into()))?
                .map_err(|e| UncodeError::Network(e.to_string()))?,
            None => send_future
                .await
                .map_err(|e| UncodeError::Network(e.to_string()))?,
        };

        crate::notify_http_response(options, response.status().as_u16(), response.headers());

        if !response.status().is_success() {
            return Err(crate::providers::http_error_from_response(response).await);
        }

        let state = AnthropicToolState::new();
        let line_buf = std::sync::Arc::new(std::sync::Mutex::new(
            crate::providers::NdjsonLineBuffer::new(),
        ));
        let stream = response
            .bytes_stream()
            .scan((state, line_buf), |(state, line_buf), chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => {
                        let mut all_events = Vec::new();
                        let mut guard = crate::safe_lock(&line_buf, "anthropic sse buffer");
                        for line in guard.push_chunk_and_drain_lines(&c) {
                            all_events.extend(parse_anthropic_chunk(&line, state));
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
            .chain(stream::once(async {
                StreamEvent::Done {
                    reason: StopReason::Stop,
                }
            }));

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
    use crate::provider_preset::apply_provider_preset;

    fn claude_model() -> Model {
        apply_provider_preset(Model {
            id: "claude-test".into(),
            name: "Claude Test".into(),
            provider: "anthropic".into(),
            api: "anthropic-messages".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            reasoning: true,
            ..Model::default()
        })
    }

    #[test]
    fn resolve_thinking_budget_from_level_map() {
        let model = claude_model();
        let options = StreamOptions {
            thinking_level: Some(ThinkingLevel::High),
            ..Default::default()
        };
        assert_eq!(
            resolve_anthropic_thinking_budget(&model, &options),
            Some(16000)
        );
    }

    #[test]
    fn resolve_thinking_budget_explicit_override() {
        let model = claude_model();
        let options = StreamOptions {
            thinking_level: Some(ThinkingLevel::High),
            thinking_budget_tokens: Some(999),
            ..Default::default()
        };
        assert_eq!(
            resolve_anthropic_thinking_budget(&model, &options),
            Some(999)
        );
    }

    #[test]
    fn resolve_thinking_budget_off_skips() {
        let model = claude_model();
        let options = StreamOptions {
            thinking_level: Some(ThinkingLevel::Off),
            ..Default::default()
        };
        assert_eq!(resolve_anthropic_thinking_budget(&model, &options), None);
    }

    #[test]
    fn build_body_includes_thinking_and_assistant_blocks() {
        let model = claude_model();
        let context = Context {
            messages: vec![Message {
                id: "m1".into(),
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Thinking {
                        text: "plan".into(),
                    },
                    ContentBlock::Text { text: "hi".into() },
                ],
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            }],
            ..Default::default()
        };
        let options = StreamOptions {
            thinking_level: Some(ThinkingLevel::Low),
            ..Default::default()
        };
        let body = build_anthropic_body(&model, &context, &options);
        assert_eq!(body["thinking"]["budget_tokens"], 4096);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "plan");
        assert_eq!(content[1]["type"], "text");
    }

    #[test]
    fn parse_thinking_sse_deltas() {
        let mut state = AnthropicToolState::new();
        let chunk = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"a"}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"b"}}
"#;
        let events = parse_anthropic_chunk(chunk, &mut state);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            StreamEvent::ThinkingDelta(s) if s == "a"
        ));
        assert!(matches!(
            &events[1],
            StreamEvent::ThinkingDelta(s) if s == "b"
        ));
    }

    #[test]
    fn stream_buffer_text_delta_via_ndjson_lines() {
        let mut state = AnthropicToolState::new();
        let mut buf = crate::providers::NdjsonLineBuffer::new();
        let chunk = br#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Hi"}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}
"#;
        let mut events = Vec::new();
        for line in buf.push_chunk_and_drain_lines(chunk) {
            events.extend(parse_anthropic_chunk(&line, &mut state));
        }
        assert!(matches!(&events[0], StreamEvent::TextDelta(s) if s == "Hi"));
        assert!(matches!(&events[1], StreamEvent::TextDelta(s) if s == "!"));
    }

    #[test]
    fn stream_buffer_tool_use_lifecycle() {
        let mut state = AnthropicToolState::new();
        let mut buf = crate::providers::NdjsonLineBuffer::new();
        let chunk = br#"data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tu_1","name":"read"}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"x\"}"}}
data: {"type":"content_block_stop","index":0}
"#;
        let mut events = Vec::new();
        for line in buf.push_chunk_and_drain_lines(chunk) {
            events.extend(parse_anthropic_chunk(&line, &mut state));
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolCallStart { name, .. } if name == "read"))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::ToolCallEnd(d) if d.name == "read" && d.arguments["path"] == "x"
        )));
    }

    #[test]
    fn test_build_body_with_image_block() {
        let model = claude_model();
        let context = Context {
            messages: vec![Message {
                id: "x".into(),
                role: Role::User,
                content: vec![ContentBlock::Image {
                    mime_type: "image/png".into(),
                    data: "base64data".into(),
                }],
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            }],
            ..Default::default()
        };
        let options = StreamOptions::default();
        let body = build_anthropic_body(&model, &context, &options);
        let content = body["messages"][0]["content"].as_str().unwrap();
        assert!(content.contains("image"));
        assert!(content.contains("base64"));
        assert!(content.contains("image/png"));
        assert!(content.contains("base64data"));
    }

    #[test]
    fn test_build_body_assistant_with_thinking_block() {
        let model = claude_model();
        let context = Context {
            messages: vec![Message {
                id: "x".into(),
                role: Role::Assistant,
                content: vec![ContentBlock::Thinking {
                    text: "I need to analyze...".into(),
                }],
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            }],
            ..Default::default()
        };
        let options = StreamOptions::default();
        let body = build_anthropic_body(&model, &context, &options);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "I need to analyze...");
    }

    #[test]
    fn test_build_body_tool_use_with_arguments() {
        let model = claude_model();
        let context = Context {
            messages: vec![Message {
                id: "x".into(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "src/main.rs"}),
                }))],
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            }],
            ..Default::default()
        };
        let options = StreamOptions::default();
        let body = build_anthropic_body(&model, &context, &options);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["id"], "tc1");
        assert_eq!(content[0]["name"], "read");
        assert_eq!(content[0]["input"]["path"], "src/main.rs");
    }

    #[test]
    fn test_build_body_assistant_multi_block() {
        let model = claude_model();
        let context = Context {
            messages: vec![Message {
                id: "x".into(),
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Thinking {
                        text: "plan".into(),
                    },
                    ContentBlock::Text {
                        text: "hello".into(),
                    },
                ],
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            }],
            ..Default::default()
        };
        let options = StreamOptions::default();
        let body = build_anthropic_body(&model, &context, &options);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[1]["type"], "text");
    }

    #[test]
    fn test_build_body_tool_result() {
        let model = claude_model();
        let context = Context {
            messages: vec![Message {
                id: "x".into(),
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(Box::new(ToolResult {
                    tool_call_id: "tc1".into(),
                    content: "file content".into(),
                    is_error: false,
                }))],
                usage: None,
                stop_reason: None,
                error_message: None,
                timestamp: None,
            }],
            ..Default::default()
        };
        let options = StreamOptions::default();
        let body = build_anthropic_body(&model, &context, &options);
        let msg = &body["messages"][0];
        assert_eq!(msg["role"], "user");
        let content = msg["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["tool_use_id"], "tc1");
        assert_eq!(content[0]["content"], "file content");
    }

    #[test]
    fn test_build_body_system_as_top_level() {
        let model = claude_model();
        let context = Context {
            system_prompt: Some("You are a helpful assistant.".into()),
            ..Default::default()
        };
        let options = StreamOptions::default();
        let body = build_anthropic_body(&model, &context, &options);
        assert_eq!(body["system"], "You are a helpful assistant.");
    }
}
