//! 认知上下文构建器（Re-export）
//!
//! 核心实现在 `crate::context_builder`（382行），包含：
//! - `BuiltContext` — 上下文重建结果（messages + effective model/thinking level）
//! - `build_context()` — 从 SessionEntry 树重建 LLM 消息列表
//!
//! 本模块提供到认知层命名空间的路径别名。
//! 参见 `docs/ai-agent-archi/uncodenow-refactoring-roadmap.md` §2.1

pub use crate::context_builder::{BuiltContext, build_context};
