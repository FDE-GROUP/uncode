use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 工具定义，传递给 LLM 的 JSON Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 工具执行器 trait，所有工具（内置或扩展）必须实现
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 返回工具的 JSON Schema 定义
    fn definition(&self) -> ToolDefinition;
    /// 执行工具，接收 JSON 参数，返回执行结果
    async fn execute(
        &self,
        arguments: serde_json::Value,
    ) -> Result<String, crate::error::UncodeError>;
}

/// 工具执行模式：并行或顺序
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    #[default]
    Parallel,
    Sequential,
}
