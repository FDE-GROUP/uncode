use async_trait::async_trait;
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

/// Content item within a tool result (aligned with Pi's TextContent | ImageContent)
#[derive(Debug, Clone)]
pub enum ToolContent {
    Text(String),
    Image { mime_type: String, data: String },
}

/// Structured tool execution result (aligned with Pi's AgentToolResult)
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Content items sent back to the LLM (text + optional images)
    pub content: Vec<ToolContent>,
    /// Whether this result represents an error
    pub is_error: bool,
    /// Structured details for UI/logging — not sent to LLM
    pub details: Option<serde_json::Value>,
    /// Hint that the agent should terminate after this result
    pub terminate: bool,
}

impl ToolResult {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text(text.into())],
            is_error: false,
            details: None,
            terminate: false,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text(message.into())],
            is_error: true,
            details: None,
            terminate: false,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_terminate(mut self) -> Self {
        self.terminate = true;
        self
    }

    /// Extract text content as a single string (for LLM feedback)
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ToolContent::Text(t) => Some(t.as_str()),
                ToolContent::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Progress update emitted during tool execution (aligned with Pi's onUpdate)
#[derive(Debug, Clone)]
pub enum ToolProgress {
    /// Spinner-style update with a text detail
    Spinner(String),
    /// Percentage progress
    Percentage {
        current: u64,
        total: u64,
        detail: String,
    },
    /// Log line output (e.g., streaming stdout)
    LogLine(String),
}

/// Context passed to tool execution, carrying cancellation + progress
pub struct ToolContext {
    /// Per-call cancellation token (child of agent-level token)
    pub cancel_token: tokio_util::sync::CancellationToken,
    /// Progress callback for streaming updates
    pub on_progress: Option<Box<dyn Fn(ToolProgress) + Send + Sync>>,
    /// Tool call ID for correlation
    pub tool_call_id: String,
}

/// Context provided to beforeToolCall hook
#[derive(Debug, Clone)]
pub struct BeforeToolCallContext {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

/// Result from beforeToolCall — Some(reason) blocks execution
pub type BeforeToolCallResult = Option<String>;

/// Context provided to afterToolCall hook
#[derive(Debug, Clone)]
pub struct AfterToolCallContext {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

/// Result from afterToolCall — patches applied on top of original result
#[derive(Debug, Clone, Default)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<ToolContent>>,
    pub details: Option<serde_json::Value>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}

/// Tool lifecycle hooks (aligned with Pi's beforeToolCall/afterToolCall)
#[async_trait]
pub trait ToolHooks: Send + Sync {
    /// Called before tool execution. Return Some(reason) to block execution.
    async fn before_tool_call(&self, _ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        None
    }

    /// Called after tool execution. Return patches to modify the result.
    async fn after_tool_call(
        &self,
        _ctx: &AfterToolCallContext,
        result: &mut ToolResult,
    ) -> AfterToolCallResult {
        let _ = result; // suppress unused warning
        AfterToolCallResult::default()
    }
}

/// 工具执行器 trait，所有工具（内置或扩展）必须实现
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 返回工具的 JSON Schema 定义
    fn definition(&self) -> ToolDefinition;

    /// Legacy execute — return a plain String. Backward compatible.
    async fn execute(
        &self,
        arguments: serde_json::Value,
    ) -> Result<String, crate::error::UncodeError>;

    /// Prepare/validate arguments before execution (aligned with Pi's prepareArguments).
    /// Default passes arguments through unchanged.
    fn prepare_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, crate::error::UncodeError> {
        Ok(arguments)
    }

    /// Execute with context (cancellation + progress). Default wraps legacy execute.
    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, crate::error::UncodeError> {
        let output = self.execute(arguments).await?;
        Ok(ToolResult::ok(output))
    }
}
