use serde_json::Value;

use crate::driver::CompletionRequest;
use uncode_core::error::UncodeError;
use uncode_core::message::{ContentBlock, Role};

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
