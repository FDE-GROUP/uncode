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
    /// Additional resource paths registered by extensions. `resolve_path()` allows
    /// access to files under these prefixes in addition to the project CWD.
    pub allowed_paths: Vec<std::path::PathBuf>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("tool_call_id", &self.tool_call_id)
            .field("allowed_paths", &self.allowed_paths)
            .finish_non_exhaustive()
    }
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
    use super::*;
    use async_trait::async_trait;

    // ── existing test ──

    #[test]
    fn tool_result_with_duration_ms_merges_existing_details() {
        let tr = ToolResult::ok("done")
            .with_details(serde_json::json!({ "bytes_written": 3 }))
            .with_duration_ms(42);
        let d = tr.details.unwrap();
        assert_eq!(d["bytes_written"], 3);
        assert_eq!(d["duration_ms"], 42);
    }

    // ── ToolResult::ok / err ──

    #[test]
    fn tool_result_ok() {
        let r = ToolResult::ok("success");
        assert!(!r.is_error);
        assert!(r.details.is_none());
        assert!(!r.terminate);
        assert_eq!(r.text_content(), "success");
    }

    #[test]
    fn tool_result_err() {
        let r = ToolResult::err("error msg");
        assert!(r.is_error);
        assert_eq!(r.text_content(), "error msg");
    }

    #[test]
    fn tool_result_err_with_details() {
        let r = ToolResult::err_with_details("error", serde_json::json!({"code": 42}));
        assert!(r.is_error);
        assert_eq!(r.details.unwrap()["code"], 42);
    }

    #[test]
    fn tool_result_with_details_on_ok() {
        let r = ToolResult::ok("done").with_details(serde_json::json!({"key": "val"}));
        assert!(!r.is_error);
        assert_eq!(r.details.unwrap()["key"], "val");
    }

    #[test]
    fn tool_result_with_terminate() {
        let r = ToolResult::ok("done").with_terminate();
        assert!(r.terminate);
    }

    #[test]
    fn tool_result_text_content_multiple() {
        let r = ToolResult {
            content: vec![
                ToolContent::Text("line1".into()),
                ToolContent::Image {
                    mime_type: "image/png".into(),
                    data: "abc".into(),
                },
                ToolContent::Text("line2".into()),
            ],
            is_error: false,
            details: None,
            terminate: false,
        };
        assert_eq!(r.text_content(), "line1\nline2");
    }

    #[test]
    fn tool_result_text_content_empty() {
        let r = ToolResult {
            content: vec![],
            is_error: false,
            details: None,
            terminate: false,
        };
        assert_eq!(r.text_content(), "");
    }

    #[test]
    fn tool_result_with_duration_ms_no_existing_details() {
        let r = ToolResult::ok("done").with_duration_ms(100);
        assert_eq!(r.details.unwrap()["duration_ms"], 100);
    }

    // ── ToolContent construction ──

    #[test]
    fn tool_content_text_variant() {
        let c = ToolContent::Text("hello".into());
        match c {
            ToolContent::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn tool_content_image_variant() {
        let c = ToolContent::Image {
            mime_type: "image/png".into(),
            data: "base64data".into(),
        };
        match c {
            ToolContent::Image { mime_type, data } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(data, "base64data");
            }
            _ => panic!("expected Image"),
        }
    }

    // ── ToolProgress variants ──

    #[test]
    fn tool_progress_spinner() {
        let p = ToolProgress::Spinner("loading".into());
        match p {
            ToolProgress::Spinner(msg) => assert_eq!(msg, "loading"),
            _ => panic!("expected Spinner"),
        }
    }

    #[test]
    fn tool_progress_percentage() {
        let p = ToolProgress::Percentage {
            current: 5,
            total: 10,
            detail: "processing".into(),
        };
        match p {
            ToolProgress::Percentage { current, total, detail } => {
                assert_eq!(current, 5);
                assert_eq!(total, 10);
                assert_eq!(detail, "processing");
            }
            _ => panic!("expected Percentage"),
        }
    }

    #[test]
    fn tool_progress_log_line() {
        let p = ToolProgress::LogLine("info".into());
        match p {
            ToolProgress::LogLine(line) => assert_eq!(line, "info"),
            _ => panic!("expected LogLine"),
        }
    }

    #[test]
    fn tool_progress_debug_clone() {
        let p = ToolProgress::Spinner("test".into());
        let _dbg = format!("{:?}", p);
        let _cloned = p.clone();
    }

    // ── BeforeToolCallContext / BeforeToolCallResult ──

    #[test]
    fn before_tool_call_context_construction() {
        let ctx = BeforeToolCallContext {
            tool_call_id: "call-1".into(),
            tool_name: "read".into(),
            args: serde_json::json!({"path": "x.txt"}),
        };
        assert_eq!(ctx.tool_call_id, "call-1");
        assert_eq!(ctx.tool_name, "read");
    }

    #[test]
    fn before_tool_call_result_type() {
        // BeforeToolCallResult = Option<String>
        let r: BeforeToolCallResult = None;
        assert!(r.is_none());
        let r: BeforeToolCallResult = Some("denied".into());
        assert_eq!(r.unwrap(), "denied");
    }

    // ── AfterToolCallContext / AfterToolCallResult ──

    #[test]
    fn after_tool_call_context_construction() {
        let ctx = AfterToolCallContext {
            tool_call_id: "call-2".into(),
            tool_name: "write".into(),
            args: serde_json::json!({"path": "x.txt"}),
        };
        assert_eq!(ctx.tool_call_id, "call-2");
    }

    #[test]
    fn after_tool_call_result_default() {
        let r = AfterToolCallResult::default();
        assert!(r.content.is_none());
        assert!(r.details.is_none());
        assert!(r.is_error.is_none());
        assert!(r.terminate.is_none());
    }

    #[test]
    fn after_tool_call_result_construction() {
        let r = AfterToolCallResult {
            content: Some(vec![ToolContent::Text("patched".into())]),
            details: None,
            is_error: None,
            terminate: Some(true),
        };
        assert!(r.terminate.unwrap());
        assert_eq!(r.content.unwrap().len(), 1);
    }

    // ── FileInfo ──

    #[test]
    fn file_info_construction() {
        let fi = FileInfo {
            path: PathBuf::from("/tmp/f.txt"),
            size: 42,
            is_dir: false,
            is_file: true,
            is_symlink: false,
            modified: None,
        };
        assert_eq!(fi.path.to_str().unwrap(), "/tmp/f.txt");
        assert_eq!(fi.size, 42);
        assert!(fi.is_file);
    }

    // ── DirEntry ──

    #[test]
    fn dir_entry_construction() {
        let de = DirEntry {
            name: "foo.rs".into(),
            is_dir: false,
            is_file: true,
            is_symlink: false,
        };
        assert_eq!(de.name, "foo.rs");
        assert!(de.is_file);
    }

    // ── ShellOptions / ShellResult ──

    #[test]
    fn shell_options_default() {
        let opts = ShellOptions::default();
        assert!(opts.workdir.is_none());
        assert!(opts.timeout_ms.is_none());
        assert!(opts.env.is_none());
    }

    #[test]
    fn shell_options_construction() {
        let opts = ShellOptions {
            workdir: Some(PathBuf::from("/tmp")),
            timeout_ms: Some(5000),
            env: Some(std::collections::HashMap::from([("KEY".into(), "val".into())])),
        };
        assert_eq!(opts.timeout_ms, Some(5000));
    }

    #[test]
    fn shell_result_construction() {
        let sr = ShellResult {
            stdout: "out".into(),
            stderr: "err".into(),
            exit_code: 0,
            cancelled: false,
        };
        assert_eq!(sr.stdout, "out");
        assert_eq!(sr.exit_code, 0);
        assert!(!sr.cancelled);
    }

    // ── ToolHooks trait (compile check) ──

    #[derive(Debug)]
    struct NoopHooks;

    #[async_trait]
    impl ToolHooks for NoopHooks {}

    #[test]
    fn tool_hooks_default_impl_compiles() {
        let hooks = NoopHooks;
        let _ = format!("{:?}", hooks);
    }

    struct RejectHooks;

    #[async_trait]
    impl ToolHooks for RejectHooks {
        async fn before_tool_call(&self, ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
            Some(format!("rejected: {}", ctx.tool_name))
        }
    }

    #[tokio::test]
    async fn tool_hooks_custom_before_rejects() {
        let hooks = RejectHooks;
        let ctx = BeforeToolCallContext {
            tool_call_id: "c1".into(),
            tool_name: "bash".into(),
            args: serde_json::json!({}),
        };
        let result = hooks.before_tool_call(&ctx).await;
        assert_eq!(result.unwrap(), "rejected: bash");
    }
}
