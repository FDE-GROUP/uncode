//! uncode-agent — 代理循环引擎
//!
//! 编排 LLM 调用 → 工具执行 → 事件广播 → 循环的主流程。
//! `AgentLoop` 是核心引擎，`GitHubClient` 提供 Issue/PR 集成能力。

pub mod compaction;
pub mod context;
pub mod github;
pub mod loop_engine;
pub mod model_switch;
pub mod steering;
pub mod stop;
pub mod system_prompt;
pub mod token;

pub use compaction::{compact_messages, estimate_context_tokens, should_compact};
pub use context::ContextLoader;
pub use github::GitHubClient;
pub use loop_engine::AgentLoop;
pub use stop::{StopCondition, StopReason, step_count_is, text_contains};
pub use system_prompt::SystemPromptBuilder;
pub use token::{estimate_cost, estimate_message_tokens, estimate_tokens};

#[cfg(test)]
mod tests;
