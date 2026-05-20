//! uncode-core — 共享类型系统
//!
//! 定义 uncode 项目所有 crate 共用的数据类型、trait 和错误类型。
//! 位于依赖树的最底层，不依赖任何内部 crate。
//!
//! **L1（Pi）：** [`SessionEntry`](session::SessionEntry)、[`AgentEvent`](event::AgentEvent)、
//! [`ToolExecutor`](tool::ToolExecutor) 等与 Pi 会话树 / 事件流 / 工具模型逻辑对齐；
//! 对照 `docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md`。

pub mod api_types;
pub mod config;
pub mod context;
pub mod diff;
pub mod error;
pub mod event;
pub mod message;
pub mod model;
pub mod session;
pub mod skill;
pub mod template;
pub mod tool;
pub mod workspace_graph;

#[cfg(test)]
mod tests;
