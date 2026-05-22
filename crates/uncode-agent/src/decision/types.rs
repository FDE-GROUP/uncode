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
        Self {
            allowed: true,
            reason: None,
            violations: vec![],
        }
    }

    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
            violations: vec![],
        }
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
/// Re-exported from `uncode_core::agent_step::AgentStep`.
/// 对应 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 中的 AgentStep 模型
pub use uncode_core::agent_step::{
    ActionObservation, AgentStateSnapshot, AgentStep, ExecutedAction, Feedback,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_verdict_approved() {
        let v = DecisionVerdict::approved();
        assert!(v.allowed);
        assert!(v.reason.is_none());
    }

    #[test]
    fn test_decision_verdict_denied() {
        let v = DecisionVerdict::denied("dangerous");
        assert!(!v.allowed);
        assert_eq!(v.reason, Some("dangerous".into()));
    }

    #[test]
    fn test_approved_action_fields() {
        let action = NormalizedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "a.rs"}),
            normalized_fields: vec!["path".into()],
        };
        let approved = ApprovedAction {
            action,
            adjudicated_at: chrono::Utc::now(),
        };
        assert_eq!(approved.action.tool_name, "read");
    }

    #[test]
    fn test_decision_context() {
        let ctx = DecisionContext {
            turn_number: 3,
            max_turns: 50,
            active_tools: vec!["read".into()],
        };
        assert_eq!(ctx.turn_number, 3);
    }

    #[test]
    fn test_denied_record_has_no_action() {
        let record = DecisionRecord {
            turn_id: "t1".into(),
            proposal: ActionProposal {
                tool_name: "rm".into(),
                raw_arguments: serde_json::json!({}),
                rationale: None,
                confidence: None,
            },
            verdict: DecisionVerdict::denied("dangerous"),
            approved_action: None,
            timestamp: chrono::Utc::now(),
            adjudication_duration_ms: 1,
        };
        assert!(!record.verdict.allowed);
        assert!(record.approved_action.is_none());
    }
}
