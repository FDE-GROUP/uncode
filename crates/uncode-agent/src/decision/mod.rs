//! 决策层 — 认知显化与决策驱动设计的核心
//!
//! ## 四阶段决策管线
//!
//! ```text
//! LLM 输出 (自然语言/结构化 ToolCall)
//!   → proposal (提案接收)   — ActionProposal
//!   → firewall (语义防火墙) — Parsing → Validation → Normalization
//!   → adjudication (裁决)   — DecisionPolicy 链 → ApprovedAction
//!   → execution (执行派发)  — parallel/sequential/terminate
//!   → audit (审计)         — DecisionRecord, AgentStep
//! ```
//!
//! ## 范式引用
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3
//! "认知层回答'接下来可以做什么'，决策层回答'哪些可以做、做了什么、结果怎样'"
//!
//! ## 与现有代码的关系
//!
//! 本模块从 `loop_engine.rs`（1616行）和 `AgentHarness`（276行）中
//! 提取决策逻辑。防火墙的 `ValidationRule` 实现包装现有的
//! `PermissionPolicy` / `PermissionGate` / 路径安全校验。

pub mod adjudication;
pub mod bridge;
pub mod evaluator; // ★ H0-H3 评估阶梯
pub mod feedback; // ★ 决策→认知 上行反馈通道
pub mod firewall;
pub mod proposal;
pub mod types;
