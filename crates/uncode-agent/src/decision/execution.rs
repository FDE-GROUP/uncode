//! 执行派发 — 将被批准的动作派发到工具执行管线
//!
//! ## 职责
//!
//! 管理 parallel / sequential / terminate 语义。
//! 当前工具管线（`tools/` 目录）的逻辑保持不变；
//! `ExecutionOrchestrator` 作为 Facade 统一入口。
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 决策层

use super::types::ApprovedAction;

/// 执行编排器 — 管理工具批次派发
pub struct ExecutionOrchestrator;

impl ExecutionOrchestrator {
    pub fn new() -> Self {
        Self
    }

    /// 派发已批准的动作到工具执行管线
    ///
    /// 在后续 commit 中对接现有的 `loop_engine.rs` 工具执行逻辑
    /// （prepare → validate → before_hook → execute → after_hook）。
    pub async fn dispatch(
        &self,
        _approved: Vec<ApprovedAction>,
    ) -> Result<Vec<ExecutionResult>, ExecutionError> {
        // TODO(decision-refactor): 对接 loop_engine.rs 的工具执行管线
        Ok(vec![])
    }
}

#[derive(Debug)]
pub struct ExecutionResult {
    pub tool_name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub output: Option<String>,
    pub terminate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("tool execution failed: {0}")]
    ToolFailed(String),
}
