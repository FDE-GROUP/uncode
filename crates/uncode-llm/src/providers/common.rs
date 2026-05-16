use serde_json::Value;
use std::collections::HashMap;

use crate::driver::{CompletionRequest, StreamEvent, UsageInfo};
use uncode_core::error::UncodeError;
use uncode_core::message::{ContentBlock, Role};
use uncode_core::tool::ToolDefinition;

/// 将 HTTP 状态码映射为 UncodeError（401/403→Auth, 429→RateLimit）
pub fn map_http_error(status: reqwest::StatusCode, body: String) -> UncodeError {
    match status.as_u16() {
        401 | 403 => UncodeError::LlmAuth(body),
        429 => UncodeError::LlmRateLimit(body),
        _ => UncodeError::Llm(format!("HTTP {status}: {body}")),
    }
}

/// 将 CompletionRequest 中的消息转换为 OpenAI 兼容 API 格式的 JSON
///
/// ToolCall 和 ToolResult 必须按 API 规范格式化为结构化字段，
/// 而非纯文本，否则 LLM 无法理解工具调用历史。
pub fn build_chat_messages(request: &CompletionRequest) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(ref system) = request.system {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system
        }));
    }

    for msg in &request.messages {
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

                if !thinking_parts.is_empty() {
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

/// Build the `tools` JSON array from tool definitions (OpenAI-compatible format).
pub fn build_tools_json(tools: &[ToolDefinition]) -> Option<Value> {
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

/// Mutable state carried across SSE chunks for OpenAI-compatible tool-call assembly.
pub struct OpenAiStreamState {
    /// Accumulated arguments per tool call ID.
    pub pending_args: HashMap<String, String>,
    /// Tool name recorded at ToolCallStart, keyed by tool call ID.
    pub tool_names: HashMap<String, String>,
    /// Maps SSE tool_call index → tool call ID (subsequent arg-only chunks omit id).
    pub index_to_id: HashMap<usize, String>,
}

impl OpenAiStreamState {
    pub fn new() -> Self {
        Self {
            pending_args: HashMap::new(),
            tool_names: HashMap::new(),
            index_to_id: HashMap::new(),
        }
    }
}

impl Default for OpenAiStreamState {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse tool_calls from a single SSE choice delta, emitting ToolCallStart/Delta events.
/// Returns events to push. Accumulates args in `state`.
pub fn parse_openai_tool_calls(
    choice: &serde_json::Map<String, Value>,
    state: &mut OpenAiStreamState,
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

/// Flush accumulated tool call args as ToolCallEnd events when finish_reason is received.
pub fn flush_tool_calls(state: &mut OpenAiStreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for (id, args) in state.pending_args.drain() {
        let parsed = match serde_json::from_str::<Value>(&args) {
            Ok(v) => v,
            Err(e) => {
                events.push(StreamEvent::Error(format!(
                    "tool args JSON parse failed: {e}"
                )));
                Value::Object(Default::default())
            }
        };
        let name = state.tool_names.remove(&id).unwrap_or_default();
        events.push(StreamEvent::ToolCallEnd {
            id,
            name,
            arguments: parsed,
        });
    }
    events
}

/// Extract usage info from an SSE event's usage field.
pub fn extract_usage(event: &Value) -> Option<StreamEvent> {
    event.get("usage").map(|usage| {
        StreamEvent::Usage(UsageInfo {
            input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
        })
    })
}
