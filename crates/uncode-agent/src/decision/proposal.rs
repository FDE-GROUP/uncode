//! 提案接收 — 从 LLM 流式输出中提取工具调用提案
//!
//! 对应决策层四阶段中的"提案接收"阶段。
//! 负责将 LLM 的 `StreamEvent::ToolCall*` 流累积为 `ActionProposal`。
//!
//! ## 提取目标
//!
//! 当前逻辑位于 `loop_engine.rs::AgentLoop::run_inner()` 的 stream 处理循环中
//! （ToolCallStart → ToolCallDelta → ToolCallEnd 的累积过程）。
//! 本模块将在后续 commit 中逐步提取该逻辑。
//!
//! 参见 `docs/ai-agent-archi/uncodenow-refactoring-roadmap.md` §1.2

use super::types::ActionProposal;
use uncode_ai::StreamEvent;

/// 累积 LLM 流式输出中的工具调用事件，生成 ActionProposal 列表
///
/// 将 `loop_engine.rs` 中 match StreamEvent::ToolCall* 分支的逻辑提取到此函数。
pub fn accumulate_proposals(
    _stream_events: Vec<StreamEvent>,
) -> Result<Vec<ActionProposal>, ProposalError> {
    // TODO(decision-refactor): 从 loop_engine.rs 的 run_inner() 提取
    // 当前返回空列表作为占位符
    Ok(vec![])
}

#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("failed to parse tool call: {0}")]
    ParseError(String),
}
