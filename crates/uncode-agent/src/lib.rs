//! uncode-agent — 代理循环引擎
//!
//! 编排 LLM 调用 → 工具执行 → 事件广播 → 循环的主流程。
//! `AgentHarness` 是生产编排器，`AgentLoop` 是核心执行引擎，`GitHubClient` 提供 Issue/PR 集成能力。
//!
//! **L1（Pi）：** 机制与 Pi `packages/agent` 对齐（双环、三队列、会话树、Compaction）；
//! 对照表见 `docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md`。

pub mod branch_summarization;
pub mod compaction;
pub mod context;
pub mod context_builder;
pub mod github;
pub mod harness;
pub mod hooks;
pub mod loop_engine;
pub mod model_switch;
pub mod permission_gate;
pub mod phase_summary;
pub mod session;
pub mod steering;
pub mod stop;
pub mod system_prompt;
pub mod token;
pub mod tool_permission;
pub mod tools;
pub mod workspace_graph;

pub use compaction::{
    compact_messages, compact_session, estimate_context_tokens, should_compact,
    should_compact_session,
};
pub use context::ContextLoader;
pub use github::GitHubClient;
pub use harness::{AgentHarness, AgentHarnessPhase, HarnessResources};
pub use hooks::{ChainedToolHooks, PermissionToolHooks};
pub use loop_engine::AgentLoop;
pub use permission_gate::{Approval, PermissionGate};
pub use stop::{StopCondition, StopReason, step_count_is, text_contains};
pub use system_prompt::SystemPromptBuilder;
pub use token::{estimate_cost, estimate_message_tokens, estimate_tokens};

#[cfg(test)]
mod tests;
