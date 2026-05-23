//! uncode-core — 共享类型系统
//!
//! 定义 uncode 项目所有 crate 共用的数据类型、trait 和错误类型。
//! 位于依赖树的最底层，不依赖任何内部 crate。
//!
//! **L1（Pi）：** `SessionEntry`、`AgentEvent`、`ToolExecutor` 等与 Pi 会话树 / 事件流 / 工具模型逻辑对齐；
//! 对照 `docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md`。

#![deny(rustdoc::broken_intra_doc_links)]

pub mod agent_step; // ★ AgentStep — 认知显化与决策驱动设计 决策层训练模型
pub mod api_types;
pub mod config;
pub mod context;
pub mod dialog;
pub mod diff;
pub mod error;
pub mod event;
pub mod message;
pub mod model;
pub mod overlay;
pub mod session;
pub mod skill;
pub mod template;
pub mod text;
pub mod tool;
pub mod ui_action;
pub mod workspace_graph;

#[cfg(test)]
mod tests;
