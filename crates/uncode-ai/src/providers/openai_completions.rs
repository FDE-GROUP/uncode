use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

use crate::api::{Api, StreamEvent, ToolCallEndData, UsageInfo};
use crate::api_types::{
    CompatConfig, Context, MaxTokensField, StopReason, StreamOptions, ThinkingFormat, ThinkingLevel,
};
use crate::message::{ContentBlock, Role};
use crate::model::Model;
use crate::providers::build_tools_json;
use uncode_shared::error::UncodeError;

pub struct OpenAiCompletionsApi {
    client: Client,
}

impl OpenAiCompletionsApi {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for OpenAiCompletionsApi {
    fn default() -> Self {
        Self::new()
    }
}

// ── 请求构建 ──

fn build_chat_messages(context: &Context, compat: &CompatConfig) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(system) = context.system_prompt.as_deref() {
        let role = if compat.supports_developer_role {
            "developer"
        } else {
            "system"
        };
        messages.push(serde_json::json!({
            "role": role,
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
                let mut thinking_parts: Vec<String> = Vec::new();
                let mut tool_calls: Vec<Value> = Vec::new();

                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => text_parts.push(text.clone()),
                        ContentBlock::Thinking { text } => thinking_parts.push(text.clone()),
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

                if !thinking_parts.is_empty()
                    && (!tool_calls.is_empty()
                        || compat.requires_reasoning_content_on_assistant_messages)
                {
                    // 工具调用回合必须回传；GLM 保留式思考下所有回合必须回传
                    m["reasoning_content"] = Value::String(thinking_parts.join("\n"));
                }

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

fn build_request_body(model: &Model, context: &Context, options: &StreamOptions) -> Value {
    let messages = build_chat_messages(context, &model.compat);
    let mut body = serde_json::json!({
        "model": model.id,
        "messages": messages,
        "stream": true,
    });
    if model.compat.supports_usage_in_streaming {
        body["stream_options"] = serde_json::json!({"include_usage": true});
    }

    if let Some(mt) = options.max_tokens {
        let field = match model.compat.max_tokens_field {
            MaxTokensField::MaxTokens => "max_tokens",
            MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
        };
        body[field] = serde_json::json!(mt);
    }
    if let Some(t) = options.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    if let Some(tools) = build_tools_json(&context.tools) {
        body["tools"] = tools;
    }
    if model.compat.supports_store {
        body["store"] = serde_json::json!(true);
    }
    if let Some(ref sid) = options.session_id {
        if model.compat.supports_user_id {
            body["user_id"] = serde_json::json!(sid);
        }
        if model.compat.send_session_affinity_headers || model.compat.supports_long_cache_retention
        {
            body["prompt_cache_key"] = serde_json::json!(sid);
        }
        if model.compat.supports_long_cache_retention
            && options.cache_retention == Some(crate::api_types::CacheRetention::Long)
        {
            body["prompt_cache_retention"] = serde_json::json!("24h");
        }
    }

    // Thinking / reasoning parameters
    if let Some(level) = options.thinking_level
        && level != ThinkingLevel::Off
        && model.reasoning
    {
        let mapped = model
            .thinking_level_map
            .get(&level)
            .and_then(|v| v.as_deref());

        match model.effective_thinking_format() {
            Some(ThinkingFormat::DeepSeek) => {
                if model.compat.supports_clear_thinking {
                    body["thinking"] =
                        serde_json::json!({"type": "enabled", "clear_thinking": false});
                } else {
                    body["thinking"] = serde_json::json!({"type": "enabled"});
                }
                if model.compat.supports_reasoning_effort {
                    let effort = mapped.unwrap_or("high");
                    body["reasoning_effort"] = serde_json::json!(effort);
                }
            }
            Some(ThinkingFormat::OpenRouter) => {
                let effort = mapped.unwrap_or("high");
                body["reasoning"] = serde_json::json!({"effort": effort});
            }
            _ => {
                if model.compat.supports_reasoning_effort {
                    let effort = mapped.unwrap_or(match level {
                        ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
                        ThinkingLevel::Medium => "medium",
                        ThinkingLevel::High | ThinkingLevel::XHigh => "high",
                        _ => "medium",
                    });
                    body["reasoning_effort"] = serde_json::json!(effort);
                }
            }
        }
    }

    body
}

// ── 流状态 ──

struct StreamState {
    pending_args: HashMap<String, String>,
    tool_names: HashMap<String, String>,
    index_to_id: HashMap<usize, String>,
}

impl StreamState {
    fn new() -> Self {
        Self {
            pending_args: HashMap::new(),
            tool_names: HashMap::new(),
            index_to_id: HashMap::new(),
        }
    }
}

// ── SSE 解析 ──

pub(crate) fn map_finish_reason(reason: &str) -> StopReason {
    match reason {
        "stop" | "end" => StopReason::Stop,
        "length" | "max_tokens" => StopReason::Length,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "content_filter" => StopReason::Error,
        _ => StopReason::Stop,
    }
}

fn parse_sse_chunk(text: &str, state: &mut StreamState, compat: &CompatConfig) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "data: [DONE]" {
            if compat.done_breaks_stream {
                break;
            }
            continue;
        }
        if let Some(json_str) = line.strip_prefix("data: ")
            && let Ok(event) = serde_json::from_str::<Value>(json_str)
        {
            if let Some(choice) = event["choices"][0].as_object() {
                events.extend(parse_tool_calls(choice, state));

                if compat.thinking_format == Some(ThinkingFormat::DeepSeek)
                    && let Some(reasoning) = choice["delta"]["reasoning_content"].as_str()
                    && !reasoning.is_empty()
                {
                    events.push(StreamEvent::ThinkingDelta(reasoning.to_string()));
                }

                if let Some(content) = choice["delta"]["content"].as_str()
                    && !content.is_empty()
                {
                    events.push(StreamEvent::TextDelta(content.to_string()));
                }

                if let Some(reason) = choice.get("finish_reason")
                    && !reason.is_null()
                {
                    events.extend(flush_tool_calls(state));
                    let stop = reason
                        .as_str()
                        .map(map_finish_reason)
                        .unwrap_or(StopReason::Stop);
                    events.push(StreamEvent::Done { reason: stop });
                }
            }
            if compat.supports_usage_in_streaming
                && let Some(usage_event) = extract_usage(&event)
            {
                events.push(usage_event);
            }
        }
    }
    events
}

fn parse_tool_calls(
    choice: &serde_json::Map<String, Value>,
    state: &mut StreamState,
) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    if let Some(tool_calls) = choice
        .get("delta")
        .and_then(|d| d.get("tool_calls"))
        .and_then(|tc| tc.as_array())
    {
        for tc in tool_calls {
            let index = tc["index"].as_u64().unwrap_or(0) as usize;
            let raw_id = tc["id"].as_str().unwrap_or("");
            let id = if !raw_id.is_empty() {
                state.index_to_id.insert(index, raw_id.to_string());
                raw_id.to_string()
            } else {
                state.index_to_id.get(&index).cloned().unwrap_or_default()
            };
            if let Some(func) = tc.get("function") {
                if let Some(name) = func["name"].as_str() {
                    state.tool_names.insert(id.clone(), name.to_string());
                    events.push(StreamEvent::ToolCallStart {
                        id: id.clone(),
                        name: name.to_string(),
                    });
                }
                if let Some(args) = func["arguments"].as_str() {
                    events.push(StreamEvent::ToolCallDelta {
                        id: id.clone(),
                        arguments: args.to_string(),
                    });
                    state.pending_args.entry(id).or_default().push_str(args);
                }
            }
        }
    }
    events
}

fn parse_streamed_tool_arguments(args: &str) -> Result<Value, serde_json::Error> {
    if let Ok(v) = serde_json::from_str(args) {
        return Ok(v);
    }
    let trimmed = args.trim();
    if trimmed.starts_with('{') && !trimmed.ends_with('}') {
        let mut repaired = trimmed.to_string();
        repaired.push('}');
        if let Ok(v) = serde_json::from_str(&repaired) {
            return Ok(v);
        }
    }
    serde_json::from_str(args)
}

fn flush_tool_calls(state: &mut StreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for (id, args) in state.pending_args.drain() {
        let parsed = match parse_streamed_tool_arguments(&args) {
            Ok(v) => v,
            Err(e) => {
                events.push(StreamEvent::Error {
                    reason: crate::api_types::StopReason::Error,
                    message: format!("tool args JSON parse failed: {e}"),
                });
                state.tool_names.remove(&id);
                continue;
            }
        };
        let name = state.tool_names.remove(&id).unwrap_or_default();
        events.push(StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
            id,
            name,
            arguments: parsed,
        })));
    }
    events
}

fn extract_usage(event: &Value) -> Option<StreamEvent> {
    event.get("usage").map(|usage| {
        // DeepSeek: prompt_cache_hit_tokens / prompt_cache_miss_tokens
        // GLM: prompt_tokens_details.cached_tokens
        let cache_hit = usage["prompt_cache_hit_tokens"]
            .as_u64()
            .or_else(|| usage["prompt_tokens_details"]["cached_tokens"].as_u64());
        let cache_miss = usage["prompt_cache_miss_tokens"].as_u64();
        StreamEvent::Usage(UsageInfo {
            input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
            cache_hit_tokens: cache_hit,
            cache_miss_tokens: cache_miss,
            reasoning_tokens: usage["completion_tokens_details"]["reasoning_tokens"].as_u64(),
        })
    })
}

// ── Api trait 实现 ──

#[async_trait]
impl Api for OpenAiCompletionsApi {
    fn api_name(&self) -> &'static str {
        "openai-completions"
    }

    async fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let mut body = build_request_body(model, context, options);
        crate::notify_request_payload(options, &mut body);
        let url = format!("{}/chat/completions", model.base_url);

        let mut req = self.client.post(&url).json(&body);
        req = crate::apply_option_headers(req, options);

        if let Some(ref key) = options.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        req = req.header("Content-Type", "application/json");

        for (k, v) in &model.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        if model.compat.send_session_affinity_headers
            && let Some(ref sid) = options.session_id
        {
            req = req.header("session_id", sid.as_str());
            req = req.header("x-client-request-id", sid.as_str());
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

        let mut compat = model.compat.clone();
        if compat.thinking_format.is_none() {
            compat.thinking_format = model.thinking_format;
        }
        let state = StreamState::new();
        let line_buf = std::sync::Arc::new(std::sync::Mutex::new(
            crate::providers::NdjsonLineBuffer::new(),
        ));
        let stream = response
            .bytes_stream()
            .scan((state, line_buf), move |(state, line_buf), chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => {
                        let mut all_events = Vec::new();
                        let mut guard = crate::safe_lock(&line_buf, "openai sse buffer");
                        for line in guard.push_chunk_and_drain_lines(&c) {
                            all_events.extend(parse_sse_chunk(&line, state, &compat));
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
mod stream_buffer_tests {
    use super::{StreamState, parse_sse_chunk, parse_streamed_tool_arguments};
    use crate::api::StreamEvent;
    use crate::api_types::CompatConfig;
    use crate::providers::NdjsonLineBuffer;

    #[test]
    fn ndjson_buffer_lines_drive_sse_parser() {
        let compat = CompatConfig::default();
        let mut state = StreamState::new();
        let mut buf = NdjsonLineBuffer::new();
        let chunk = br#"data: {"choices":[{"delta":{"content":"hi"}}]}
data: {"choices":[{"delta":{},"finish_reason":"stop"}]}
"#;
        let mut events = Vec::new();
        for line in buf.push_chunk_and_drain_lines(chunk) {
            events.extend(parse_sse_chunk(&line, &mut state, &compat));
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::TextDelta(s) if s == "hi"))
        );
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Done { .. })));
    }

    #[test]
    fn sse_tool_call_roundtrip_through_buffer() {
        let compat = CompatConfig::default();
        let mut state = StreamState::new();
        let mut buf = NdjsonLineBuffer::new();
        let chunk = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read","arguments":"{\"path\":"}}]}}]}
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a\"}"}}]}}]}
"#;
        let mut events = Vec::new();
        for line in buf.push_chunk_and_drain_lines(chunk) {
            events.extend(parse_sse_chunk(&line, &mut state, &compat));
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolCallStart { name, .. } if name == "read"))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::ToolCallDelta { id, arguments }
                if id == "call_1" && arguments.contains("path")
        )));
    }

    #[test]
    fn parse_streamed_tool_arguments_repairs_truncated_object() {
        let raw = r#"{"path":"src/main.rs","line":1"#;
        let v = parse_streamed_tool_arguments(raw).expect("repaired json");
        assert_eq!(v["path"], "src/main.rs");
        assert_eq!(v["line"], 1);
    }

    #[test]
    fn parse_streamed_tool_arguments_valid_unchanged() {
        let raw = r#"{"path":"a.rs"}"#;
        let v = parse_streamed_tool_arguments(raw).unwrap();
        assert_eq!(v["path"], "a.rs");
    }
}

#[cfg(test)]
mod build_functions_tests {
    use super::*;
    use crate::api_types::{CompatConfig, MaxTokensField};
    use crate::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
    use crate::model::Model;
    use crate::tool_def::ToolDefinition;

    fn default_message() -> Message {
        Message {
            id: String::new(),
            role: Role::User,
            content: vec![],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        }
    }

    // ── build_chat_messages ──

    #[test]
    fn build_messages_empty_context() {
        let context = Context::default();
        let compat = CompatConfig::default();
        let msgs = build_chat_messages(&context, &compat);
        assert!(msgs.is_empty());
    }

    #[test]
    fn build_messages_system_prompt() {
        let context = Context {
            system_prompt: Some("You are helpful.".into()),
            ..Context::default()
        };
        let compat = CompatConfig::default();
        let msgs = build_chat_messages(&context, &compat);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
    }

    #[test]
    fn build_messages_system_prompt_developer_role() {
        let context = Context {
            system_prompt: Some("You are helpful.".into()),
            ..Context::default()
        };
        let compat = CompatConfig {
            supports_developer_role: true,
            ..CompatConfig::default()
        };
        let msgs = build_chat_messages(&context, &compat);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "developer");
    }

    #[test]
    fn build_messages_skips_system_role_in_list() {
        let context = Context {
            messages: vec![Message::system("system msg"), Message::user("hello")],
            ..Context::default()
        };
        let compat = CompatConfig::default();
        let msgs = build_chat_messages(&context, &compat);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn build_messages_user_message() {
        let context = Context {
            messages: vec![Message::user("hello world")],
            ..Context::default()
        };
        let compat = CompatConfig::default();
        let msgs = build_chat_messages(&context, &compat);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "hello world");
    }

    #[test]
    fn build_messages_assistant_with_text_and_thinking() {
        let msg = Message {
            id: "1".into(),
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "result".into(),
                },
                ContentBlock::Thinking {
                    text: "reasoning".into(),
                },
            ],
            ..default_message()
        };
        let context = Context {
            messages: vec![msg],
            ..Context::default()
        };
        let compat = CompatConfig::default();
        let msgs = build_chat_messages(&context, &compat);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "assistant");
        assert_eq!(msgs[0]["content"], "result");
        // reasoning_content 仅在工具调用回合回传（无 ToolCall = 不发送）
        assert!(msgs[0].get("reasoning_content").is_none());
    }

    #[test]
    fn build_messages_assistant_with_tool_calls() {
        let msg = Message {
            id: "1".into(),
            role: Role::Assistant,
            content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "src/main.rs"}),
            }))],
            ..default_message()
        };
        let context = Context {
            messages: vec![msg],
            ..Context::default()
        };
        let compat = CompatConfig::default();
        let msgs = build_chat_messages(&context, &compat);
        assert_eq!(msgs.len(), 1);
        let tool_calls = msgs[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(tool_calls[0]["function"]["name"], "read");
    }

    #[test]
    fn build_messages_tool_result() {
        let msg = Message {
            id: "1".into(),
            role: Role::Tool,
            content: vec![ContentBlock::ToolResult(Box::new(ToolResult {
                tool_call_id: "call_1".into(),
                content: "file contents".into(),
                is_error: false,
            }))],
            ..default_message()
        };
        let context = Context {
            messages: vec![msg],
            ..Context::default()
        };
        let compat = CompatConfig::default();
        let msgs = build_chat_messages(&context, &compat);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "tool");
        assert_eq!(msgs[0]["tool_call_id"], "call_1");
        assert_eq!(msgs[0]["content"], "file contents");
    }

    // ── build_request_body ──

    fn make_model() -> Model {
        Model {
            id: "test-model".into(),
            name: "Test".into(),
            ..Model::default()
        }
    }

    #[test]
    fn build_body_minimal() {
        let model = make_model();
        let context = Context {
            messages: vec![Message::user("hi")],
            ..Context::default()
        };
        let options = StreamOptions::default();
        let body = build_request_body(&model, &context, &options);
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn build_body_with_tools() {
        let model = make_model();
        let context = Context {
            messages: vec![Message::user("read file")],
            tools: vec![ToolDefinition {
                name: "read".into(),
                description: "read file".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"]
                }),
                label: None,
                execution_mode: Default::default(),
            }],
            ..Context::default()
        };
        let options = StreamOptions::default();
        let body = build_request_body(&model, &context, &options);
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
    }

    #[test]
    fn build_body_with_temperature() {
        let model = make_model();
        let context = Context::default();
        let options = StreamOptions {
            temperature: Some(0.5),
            ..Default::default()
        };
        let body = build_request_body(&model, &context, &options);
        assert_eq!(body["temperature"], 0.5);
    }

    #[test]
    fn build_body_with_max_tokens() {
        let model = make_model();
        let context = Context::default();
        let options = StreamOptions {
            max_tokens: Some(1024),
            ..Default::default()
        };
        let body = build_request_body(&model, &context, &options);
        assert_eq!(body["max_tokens"], 1024);
    }

    #[test]
    fn build_body_max_completion_tokens_field() {
        let model = Model {
            compat: CompatConfig {
                max_tokens_field: MaxTokensField::MaxCompletionTokens,
                ..CompatConfig::default()
            },
            ..make_model()
        };
        let context = Context::default();
        let options = StreamOptions {
            max_tokens: Some(512),
            ..Default::default()
        };
        let body = build_request_body(&model, &context, &options);
        assert_eq!(body["max_completion_tokens"], 512);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn build_body_session_affinity_headers() {
        let model = Model {
            compat: CompatConfig {
                send_session_affinity_headers: true,
                ..CompatConfig::default()
            },
            ..make_model()
        };
        let context = Context::default();
        let options = StreamOptions {
            session_id: Some("sess-1".into()),
            ..Default::default()
        };
        let body = build_request_body(&model, &context, &options);
        assert_eq!(body["prompt_cache_key"], "sess-1");
    }

    #[test]
    fn build_body_long_cache_retention() {
        let model = Model {
            compat: CompatConfig {
                supports_long_cache_retention: true,
                send_session_affinity_headers: true,
                ..CompatConfig::default()
            },
            ..make_model()
        };
        let context = Context::default();
        let options = StreamOptions {
            session_id: Some("sess-2".into()),
            cache_retention: Some(crate::api_types::CacheRetention::Long),
            ..Default::default()
        };
        let body = build_request_body(&model, &context, &options);
        assert_eq!(body["prompt_cache_key"], "sess-2");
        assert_eq!(body["prompt_cache_retention"], "24h");
    }
}
