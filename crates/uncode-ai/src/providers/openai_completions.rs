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

    if let Some(ref system) = context.system_prompt {
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
                    && compat.thinking_format == Some(ThinkingFormat::DeepSeek)
                {
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
    if let Some(level) = options.thinking_level {
        if level != ThinkingLevel::Off && model.reasoning {
            let mapped = model
                .thinking_level_map
                .get(&level)
                .and_then(|v| v.as_deref());

            match model.compat.thinking_format {
                Some(ThinkingFormat::DeepSeek) => {
                    if let Some(effort) = mapped {
                        body["thinking"] = serde_json::json!({"type": "enabled"});
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
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                if let Some(choice) = event["choices"][0].as_object() {
                    events.extend(parse_tool_calls(choice, state));

                    if compat.thinking_format == Some(ThinkingFormat::DeepSeek) {
                        if let Some(reasoning) = choice["delta"]["reasoning_content"].as_str() {
                            if !reasoning.is_empty() {
                                events.push(StreamEvent::ThinkingDelta(reasoning.to_string()));
                            }
                        }
                    }

                    if let Some(content) = choice["delta"]["content"].as_str() {
                        if !content.is_empty() {
                            events.push(StreamEvent::TextDelta(content.to_string()));
                        }
                    }

                    if let Some(reason) = choice.get("finish_reason") {
                        if !reason.is_null() {
                            events.extend(flush_tool_calls(state));
                            let stop = reason
                                .as_str()
                                .map(map_finish_reason)
                                .unwrap_or(StopReason::Stop);
                            events.push(StreamEvent::Done { reason: stop });
                        }
                    }
                }
                if compat.supports_usage_in_streaming {
                    if let Some(usage_event) = extract_usage(&event) {
                        events.push(usage_event);
                    }
                }
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

fn flush_tool_calls(state: &mut StreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for (id, args) in state.pending_args.drain() {
        let parsed = match serde_json::from_str::<Value>(&args) {
            Ok(v) => v,
            Err(e) => {
                events.push(StreamEvent::Error {
                    reason: crate::api_types::StopReason::Error,
                    message: format!("tool args JSON parse failed: {e}"),
                });
                Value::Object(Default::default())
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
        StreamEvent::Usage(UsageInfo {
            input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
        })
    })
}

fn map_http_error(status: reqwest::StatusCode, body: String) -> UncodeError {
    match status.as_u16() {
        401 | 403 => UncodeError::LlmAuth(body),
        429 => UncodeError::LlmRateLimit(body),
        _ => UncodeError::Llm(format!("HTTP {status}: {body}")),
    }
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
        let body = build_request_body(model, context, options);
        let url = format!("{}/chat/completions", model.base_url);

        let mut req = self.client.post(&url).json(&body);

        if let Some(ref key) = options.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        req = req.header("Content-Type", "application/json");

        for (k, v) in &model.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        if model.compat.send_session_affinity_headers {
            if let Some(ref sid) = options.session_id {
                req = req.header("session_id", sid.as_str());
                req = req.header("x-client-request-id", sid.as_str());
            }
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

        if !response.status().is_success() {
            return Err(map_http_error(
                response.status(),
                response.text().await.unwrap_or_default(),
            ));
        }

        let compat = model.compat.clone();
        let state = StreamState::new();
        let stream = response
            .bytes_stream()
            .scan(state, move |state, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_sse_chunk(&String::from_utf8_lossy(&c), state, &compat),
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
