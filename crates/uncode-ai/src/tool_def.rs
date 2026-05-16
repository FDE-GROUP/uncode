use serde::{Deserialize, Serialize};

/// 工具定义，传递给 LLM 的 JSON Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    /// Human-readable label for UI display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Execution mode: Parallel or Sequential
    #[serde(default)]
    pub execution_mode: ExecutionMode,
}

/// 工具执行模式：并行或顺序
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    #[default]
    Parallel,
    Sequential,
}
