pub mod anthropic_messages;
pub mod gemini_generative;
pub mod ollama_native;
pub mod openai_completions;

use crate::tool_def::ToolDefinition;
use serde_json::Value;

/// Build OpenAI-compatible tool definitions JSON.
/// Shared by openai_completions and ollama_native providers.
pub(crate) fn build_tools_json(tools: &[ToolDefinition]) -> Option<Value> {
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
