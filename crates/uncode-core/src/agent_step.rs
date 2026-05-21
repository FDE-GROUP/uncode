//! AgentStep — 面向离线训练的决策步骤模型
//!
//! 认知与决策驱动设计 决策层 §3.3 中的 AgentStep 模型：
//! ```text
//! { state_before, action, observation, feedback? }
//! ```
//!
//! AgentStep 将"在线推理"和"离线训练"统一到同一数据结构——
//! 事件流 = 在线系统 + 离线训练数据的统一接口。
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3

use serde::{Deserialize, Serialize};

/// 单次 Agent 决策步骤（面向 RL trajectory 建模）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub step_id: String,
    pub turn_id: String,
    /// 决策前的状态快照
    pub state_before: AgentStateSnapshot,
    /// Agent 采取的行动
    pub action: ExecutedAction,
    /// 行动后的观察结果
    pub observation: ActionObservation,
    /// 人类或自动化评价信号
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<Feedback>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateSnapshot {
    pub phase: String,
    pub turn_number: u32,
    pub active_tools: Vec<String>,
    pub context_size_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedAction {
    pub tool_name: String,
    pub arguments_summary: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionObservation {
    pub success: bool,
    pub output_summary: String,
    pub files_changed: Vec<String>,
    pub duration_ms: u64,
    pub terminate: bool,
}

/// 人类或自动化评价信号
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feedback {
    HumanApproval { approved: bool, comment: Option<String> },
    TestPassed { test_name: String },
    TestFailed { test_name: String, error: String },
    AutoRevert { reason: String },
}
