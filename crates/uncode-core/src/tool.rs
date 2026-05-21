//! Agent 工具 trait、沙箱与 Hook 扩展点。
//!
//! **Pi:** `ToolExecutor` 对应 `AgentTool`；`ToolHooks` 对应 Harness `tool_call` / `tool_result`。

use async_trait::async_trait;
use std::path::{Path, PathBuf};

// Re-export ToolDefinition + ExecutionMode from uncode-ai
pub use uncode_ai::tool_def::{ExecutionMode, ToolDefinition};

/// Content item within a tool result
#[derive(Debug, Clone)]
pub enum ToolContent {
    Text(String),
    Image { mime_type: String, data: String },
}

/// Structured tool execution result.
///
/// **Pi:** 对应 tool result；`terminate` 参与批次 AND 终止语义（同 Pi agentLoop）。
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
    pub details: Option<serde_json::Value>,
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

    pub fn err_with_details(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            content: vec![ToolContent::Text(message.into())],
            is_error: true,
            details: Some(details),
            terminate: false,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// 合并 `duration_ms` 等字段到现有 `details`（保留工具已写入的键）。
    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        let mut map = match self.details.take() {
            Some(serde_json::Value::Object(m)) => m,
            Some(other) => {
                let mut m = serde_json::Map::new();
                m.insert("_extra".into(), other);
                m
            }
            None => serde_json::Map::new(),
        };
        map.insert("duration_ms".into(), serde_json::json!(duration_ms));
        self.details = Some(serde_json::Value::Object(map));
        self
    }

    pub fn with_terminate(mut self) -> Self {
        self.terminate = true;
        self
    }

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

/// Progress update emitted during tool execution
#[derive(Debug, Clone)]
pub enum ToolProgress {
    Spinner(String),
    Percentage {
        current: u64,
        total: u64,
        detail: String,
    },
    LogLine(String),
}

/// Context passed to tool execution
pub struct ToolContext {
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub on_progress: Option<Box<dyn Fn(ToolProgress) + Send + Sync>>,
    pub tool_call_id: String,
    /// Runtime file/shell backend. `None` → tools fall back to `LocalExecutionEnv`.
    pub execution_env: Option<std::sync::Arc<dyn ExecutionEnv>>,
}

/// Context provided to beforeToolCall hook
#[derive(Debug, Clone)]
pub struct BeforeToolCallContext {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

pub type BeforeToolCallResult = Option<String>;

/// Context provided to afterToolCall hook
#[derive(Debug, Clone)]
pub struct AfterToolCallContext {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<ToolContent>>,
    pub details: Option<serde_json::Value>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}

/// Tool lifecycle hooks.
///
/// **Pi:** 对应 Harness `tool_call`（阻止执行）与 `tool_result`（patch / terminate）。
#[async_trait]
pub trait ToolHooks: Send + Sync {
    async fn before_tool_call(&self, _ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        None
    }

    async fn after_tool_call(
        &self,
        _ctx: &AfterToolCallContext,
        result: &mut ToolResult,
    ) -> AfterToolCallResult {
        let _ = result;
        AfterToolCallResult::default()
    }
}

/// 工具执行器 trait。
///
/// **Pi:** 对应 `AgentTool` 执行面；**OpenCode:** 对照 Tool 注册与执行器。
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    async fn execute(
        &self,
        arguments: serde_json::Value,
    ) -> Result<String, crate::error::UncodeError>;

    fn prepare_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, crate::error::UncodeError> {
        Ok(arguments)
    }

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, crate::error::UncodeError> {
        let output = self.execute(arguments).await?;
        Ok(ToolResult::ok(output))
    }
}

// ── ExecutionEnv ──

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ShellOptions {
    pub workdir: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct ShellResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub cancelled: bool,
}

#[async_trait]
pub trait FileSystem: Send + Sync {
    async fn read_text_file(&self, path: &Path) -> Result<String, crate::error::UncodeError>;
    async fn write_file(&self, path: &Path, content: &str)
    -> Result<(), crate::error::UncodeError>;
    async fn file_info(&self, path: &Path) -> Result<FileInfo, crate::error::UncodeError>;
    async fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, crate::error::UncodeError>;
    async fn exists(&self, path: &Path) -> Result<bool, crate::error::UncodeError>;
    async fn canonical_path(&self, path: &Path) -> Result<PathBuf, crate::error::UncodeError>;
}

#[async_trait]
pub trait Shell: Send + Sync {
    async fn exec(
        &self,
        cmd: &str,
        opts: ShellOptions,
    ) -> Result<ShellResult, crate::error::UncodeError>;
}

pub trait ExecutionEnv: Send + Sync {
    fn fs(&self) -> &dyn FileSystem;
    fn shell(&self) -> &dyn Shell;
}

#[cfg(test)]
mod tests {
    use super::ToolResult;

    #[test]
    fn tool_result_with_duration_ms_merges_existing_details() {
        let tr = ToolResult::ok("done")
            .with_details(serde_json::json!({ "bytes_written": 3 }))
            .with_duration_ms(42);
        let d = tr.details.unwrap();
        assert_eq!(d["bytes_written"], 3);
        assert_eq!(d["duration_ms"], 42);
    }
}
