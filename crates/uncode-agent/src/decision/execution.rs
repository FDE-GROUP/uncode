//! 执行派发 — 将被批准的动作派发到工具执行管线
//!
//! ## 职责
//!
//! 管理 parallel / sequential / terminate 语义。
//! 对接现有 `tools/` 目录的工具管线，通过 `ToolRegistry` 和 `ToolHooks` 复现现有执行流。
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 决策层

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::info;
use uncode_core::event::AgentEvent;
use uncode_core::tool::{BeforeToolCallContext, ToolContext, ToolHooks};

use super::types::ApprovedAction;

/// 执行编排器 — 对接现有工具管线
pub struct ExecutionOrchestrator {
    tool_registry: Arc<crate::tools::ToolRegistry>,
    tool_hooks: Option<Arc<dyn ToolHooks>>,
    cancel_token: CancellationToken,
    event_tx: tokio::sync::broadcast::Sender<AgentEvent>,
}

#[derive(Debug)]
pub struct ExecutionResult {
    pub tool_name: String,
    pub tool_id: String,
    pub success: bool,
    pub duration_ms: u64,
    pub output: Option<String>,
    pub error: Option<String>,
    pub terminate: bool,
}

impl ExecutionResult {
    pub fn success(id: impl Into<String>, name: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            tool_id: id.into(),
            tool_name: name.into(),
            success: true,
            duration_ms,
            output: None,
            error: None,
            terminate: false,
        }
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }
}

impl ExecutionOrchestrator {
    pub fn new(
        tool_registry: Arc<crate::tools::ToolRegistry>,
        tool_hooks: Option<Arc<dyn ToolHooks>>,
        cancel_token: CancellationToken,
        event_tx: tokio::sync::broadcast::Sender<AgentEvent>,
    ) -> Self {
        Self { tool_registry, tool_hooks, cancel_token, event_tx }
    }

    /// 派发已批准的动作到工具执行管线
    ///
    /// 复现 `loop_engine.rs` 中的 `prepare_tool_call()` + `execute_prepared_tool()` 流程：
    /// prepare → validate → before_hook → execute → after_hook
    pub async fn dispatch(
        &self,
        approved: Vec<ApprovedAction>,
    ) -> Result<Vec<ExecutionResult>, ExecutionError> {
        let mut results = Vec::with_capacity(approved.len());

        for action in &approved {
            let id = uuid::Uuid::new_v4().to_string();
            let name = &action.action.tool_name;

            let result = self.execute_single(&id, name, &action.action.arguments).await;

            let output_text = result.output.clone().unwrap_or_default();

            results.push(ExecutionResult {
                tool_id: id,
                tool_name: name.clone(),
                success: result.success,
                duration_ms: result.duration_ms,
                output: if output_text.is_empty() { None } else { Some(output_text) },
                error: result.error.clone(),
                terminate: result.terminate,
            });
        }

        Ok(results)
    }

    /// 执行单个工具调用 — 对接现有管线
    async fn execute_single(
        &self,
        id: &str,
        name: &str,
        args: &serde_json::Value,
    ) -> ToolExecOutcome {
        let start = std::time::Instant::now();

        // 1. prepare + validate
        let prepared = match self.tool_registry.prepare_and_validate(name, args.clone()) {
            Ok(a) => a,
            Err(e) => {
                return ToolExecOutcome {
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: None,
                    error: Some(format!("{e}")),
                    terminate: false,
                };
            }
        };

        // 2. before hook
        if let Some(ref hooks) = self.tool_hooks {
            let ctx = BeforeToolCallContext {
                tool_call_id: id.to_string(),
                tool_name: name.to_string(),
                args: prepared.clone(),
            };
            if let Some(reason) = hooks.before_tool_call(&ctx).await {
                return ToolExecOutcome {
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: None,
                    error: Some(reason),
                    terminate: false,
                };
            }
        }

        // 3. execute
        let executor = match self.tool_registry.get(name) {
            Some(e) => e,
            None => {
                return ToolExecOutcome {
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: None,
                    error: Some(format!("tool not found: {name}")),
                    terminate: false,
                };
            }
        };

        let result = match executor
            .execute_with_context(
                prepared.clone(),
                ToolContext {
                    cancel_token: self.cancel_token.clone(),
                    on_progress: None,
                    tool_call_id: id.to_string(),
                    execution_env: Some(crate::tools::default_execution_env()),
                },
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return ToolExecOutcome {
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: None,
                    error: Some(format!("{e}")),
                    terminate: false,
                };
            }
        };

        let duration = start.elapsed().as_millis() as u64;
        let success = result.is_error;
        let terminate = result.terminate;

        // 4. after hook
        if let Some(ref hooks) = self.tool_hooks {
            let mut tool_result = result.clone();
            let ctx = uncode_core::tool::AfterToolCallContext {
                tool_call_id: id.to_string(),
                tool_name: name.to_string(),
                args: prepared,
            };
            let patched = hooks.after_tool_call(&ctx, &mut tool_result).await;
            if patched.terminate.unwrap_or(false) {
                info!("after_tool_call hook requested terminate for {name}");
            }
            // Apply patches from after hook
            if let Some(ref content) = patched.content {
                tool_result.content = content.clone();
            }
            if let Some(ref details) = patched.details {
                tool_result.details = Some(details.clone());
            }
            if patched.is_error.unwrap_or(false) {
                tool_result.is_error = false;
            }
        }

        let text = result.text_content();
        ToolExecOutcome {
            success,
            duration_ms: duration,
            output: Some(text.clone()),
            error: if success { None } else { Some(text) },
            terminate: result.terminate,
        }
    }
}

/// 内部工具执行结果（在 dispatch 间传递）
struct ToolExecOutcome {
    success: bool,
    duration_ms: u64,
    output: Option<String>,
    error: Option<String>,
    terminate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("tool execution failed: {0}")]
    ToolFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_orchestrator() -> ExecutionOrchestrator {
        let registry = Arc::new(crate::tools::ToolRegistry::new());
        let (tx, _) = tokio::sync::broadcast::channel(256);
        ExecutionOrchestrator::new(
            registry, None,
            tokio_util::sync::CancellationToken::new(), tx,
        )
    }

    #[test]
    fn test_orchestrator_construction() {
        let orchestrator = make_orchestrator();
        // 空 registry + 空 hooks → 构造成功即可
        let _ = orchestrator;
    }

    #[test]
    fn test_execution_result_fields() {
        let result = ExecutionResult {
            tool_id: "t1".into(), tool_name: "read".into(),
            success: true, duration_ms: 100, output: Some("content".into()),
            error: None, terminate: false,
        };
        assert!(result.success);
        assert_eq!(result.tool_name, "read");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_execution_result_failure() {
        let result = ExecutionResult {
            tool_id: "t2".into(), tool_name: "write".into(),
            success: false, duration_ms: 50, output: None,
            error: Some("permission denied".into()), terminate: false,
        };
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_execution_result_with_output_builder() {
        let result = ExecutionResult::success("t1", "read", 100)
            .with_output("file contents");
        assert_eq!(result.output, Some("file contents".into()));
    }
}
