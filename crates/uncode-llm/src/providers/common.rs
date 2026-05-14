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

/// 将 CompletionRequest 中的消息转换为 LLM API 格式的 JSON
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
            continue; // already handled above
        }
        let content = msg
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                ContentBlock::Thinking { .. } => None,
                ContentBlock::ToolCall(tc) => Some(format!("[tool_call: {}]", tc.name)),
                ContentBlock::ToolResult(tr) => Some(tr.content.clone()),
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

    messages
}
