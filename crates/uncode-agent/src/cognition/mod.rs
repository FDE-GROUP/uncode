//! 认知层 — LLM 的"自由之地"
//!
//! ## 认知与决策驱动设计中的定位
//!
//! 认知层回答一个问题：**"接下来可以做什么？"**
//!
//! 它负责理解任务、召回知识、推理方案、生成候选行动。
//! 认知层的输出是 `ActionProposal`（候选方案），而非最终命令——
//! 最终的合法性判定由决策层（`decision/`）负责。
//!
//! ## 模块结构
//!
//! | 模块 | 职责 | 状态 |
//! |:---|:---|:---|
//! | `context_builder` | 从事件流重建认知上下文 | re-export（已有） |
//! | `prompt_manager` | 系统提示词 + 工具描述生成 | ★ 新增 |
//! | `uncertainty` | 不确定性三分类显式建模 | ★ 新增 |
//! | `memory` | 压缩边界管理 + 摘要注入策略 | ★ 新增 |
//!
//! ## 范式引用
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3
//! "认知层回答'接下来可以做什么'，决策层回答'哪些可以做、做了什么、结果怎样'"

pub mod context_builder;
pub mod memory;
pub mod prompt_manager;
pub mod uncertainty;
