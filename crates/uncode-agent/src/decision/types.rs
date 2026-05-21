//! 决策层共享类型
//!
//! 定义在提案接收、防火墙、裁决、执行、审计各阶段之间流转的核心类型。
//!
//! ## 类型状态机
//!
//! ```text
//! ActionProposal (LLM 原始输出)
//!   → ParsedAction (防火墙 Parsing)
//!   → ValidatedAction (防火墙 Validation)
//!   → NormalizedAction (防火墙 Normalization)
//!   → ApprovedAction (裁决通过)
//!   → DecisionVerdict (允许/拒绝 + 理由)
//! ```
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3

/// 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3
#[derive(Debug, Clone)]
pub struct ActionProposal {
    pub tool_name: String,
    pub raw_arguments: serde_json::Value,
    pub rationale: Option<String>,
    pub confidence: Option<f32>,
}

/// 防火墙 Parsing 层输出——已结构化的动作
#[derive(Debug, Clone)]
pub struct ParsedAction {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// 防火墙 Validation 层输出——已通过合法性校验的动作
#[derive(Debug, Clone)]
pub struct ValidatedAction {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub applied_rules: Vec<String>,
}

/// 防火墙 Normalization 层输出——消歧义、标准化后的最终形式
#[derive(Debug, Clone)]
pub struct NormalizedAction {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub normalized_fields: Vec<String>,
}

/// 裁决通过的、可执行的动作
#[derive(Debug, Clone)]
pub struct ApprovedAction {
    pub action: NormalizedAction,
    pub adjudicated_at: chrono::DateTime<chrono::Utc>,
}

/// 裁决结果
#[derive(Debug, Clone)]
pub struct DecisionVerdict {
    pub allowed: bool,
    pub reason: Option<String>,
    pub violations: Vec<String>,
}

impl DecisionVerdict {
    pub fn approved() -> Self {
        Self { allowed: true, reason: None, violations: vec![] }
    }

    pub fn denied(reason: impl Into<String>) -> Self {
        Self { allowed: false, reason: Some(reason.into()), violations: vec![] }
    }
}

/// 裁决上下文快照
#[derive(Debug, Clone)]
pub struct DecisionContext {
    pub turn_number: u32,
    pub max_turns: u32,
    pub active_tools: Vec<String>,
}

/// 决策记录——进入审计层的单次决策
#[derive(Debug, Clone)]
pub struct DecisionRecord {
    pub turn_id: String,
    pub proposal: ActionProposal,
    pub verdict: DecisionVerdict,
    pub approved_action: Option<ApprovedAction>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub adjudication_duration_ms: u64,
}

/// 面向离线训练的决策步骤
/// 对应 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 中的 AgentStep 模型
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStep {
    pub step_id: String,
    pub turn_id: String,
    pub state_before: AgentStateSnapshot,
    pub action: ExecutedAction,
    pub observation: ActionObservation,
    pub feedback: Option<Feedback>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStateSnapshot {
    pub phase: String,
    pub turn_number: u32,
    pub active_tools: Vec<String>,
    pub context_size_tokens: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutedAction {
    pub tool_name: String,
    pub arguments_summary: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionObservation {
    pub success: bool,
    pub output_summary: String,
    pub files_changed: Vec<String>,
    pub duration_ms: u64,
    pub terminate: bool,
}

/// 人类或自动化评价信号
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Feedback {
    HumanApproval { approved: bool, comment: Option<String> },
    TestPassed { test_name: String },
    TestFailed { test_name: String, error: String },
    AutoRevert { reason: String },
}
