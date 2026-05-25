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

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 意图类型：LLM 调用工具的目的分类
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentType {
    FileRead,
    FileWrite,
    FileEdit,
    Search,
    Execution,
    WebAccess,
    Unknown,
}

impl IntentType {
    pub fn from_tool_name(tool_name: &str) -> Self {
        match tool_name {
            "read" | "find" | "ls" | "grep" => Self::FileRead,
            "write" => Self::FileWrite,
            "edit" => Self::FileEdit,
            "bash" => Self::Execution,
            "web_fetch" | "web_search" => Self::WebAccess,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for IntentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileRead => write!(f, "FileRead"),
            Self::FileWrite => write!(f, "FileWrite"),
            Self::FileEdit => write!(f, "FileEdit"),
            Self::Search => write!(f, "Search"),
            Self::Execution => write!(f, "Execution"),
            Self::WebAccess => write!(f, "WebAccess"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// 候选动作（多路裁决时的备选方案）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub description: String,
}

/// 认知路径溯源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveTrace {
    pub turn: u32,
    pub source: String,
    pub llm_model: String,
}

/// 防火墙审计记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallAudit {
    pub passed: bool,
    pub stage_failed: Option<String>,
    pub violations: Vec<String>,
    pub normalized_fields: Vec<String>,
}

/// 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3
#[derive(Debug, Clone)]
pub struct ActionProposal {
    pub proposal_id: Uuid,
    pub tool_name: String,
    pub raw_arguments: serde_json::Value,
    pub intent: IntentType,
    pub rationale: Option<String>,
    pub confidence: Option<f32>,
    pub alternatives: Vec<Alternative>,
    pub trace: Vec<CognitiveTrace>,
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
    pub warnings: Vec<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub id: String,
    pub turn_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub intent: String,
    pub verdict_allowed: bool,
    pub verdict_reason: Option<String>,
    pub firewall_result: Option<FirewallAudit>,
    pub duration_ms: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 工具执行结果（由 loop_engine 构造，供 feedback 层消费）
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub tool_id: String,
    pub tool_name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub output: Option<String>,
    pub error: Option<String>,
    pub terminate: bool,
}

impl ExecutionResult {
    pub fn success(id: impl Into<String>, name: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            tool_id: id.into(),
            tool_name: name.into(),
            success: true,
            duration_ms,
            output: None,
            error: None,
            terminate: false,
        }
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }
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
            warnings: vec![],
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
            id: uuid::Uuid::new_v4().to_string(),
            turn_id: "t1".into(),
            session_id: "s1".into(),
            tool_name: "rm".into(),
            intent: "Execution".into(),
            verdict_allowed: false,
            verdict_reason: Some("dangerous".into()),
            firewall_result: None,
            duration_ms: 1,
            timestamp: chrono::Utc::now(),
        };
        assert!(!record.verdict_allowed);
    }

    #[test]
    fn test_intent_type_from_tool_name() {
        assert_eq!(IntentType::from_tool_name("read"), IntentType::FileRead);
        assert_eq!(IntentType::from_tool_name("write"), IntentType::FileWrite);
        assert_eq!(IntentType::from_tool_name("edit"), IntentType::FileEdit);
        assert_eq!(IntentType::from_tool_name("grep"), IntentType::FileRead);
        assert_eq!(IntentType::from_tool_name("bash"), IntentType::Execution);
        assert_eq!(
            IntentType::from_tool_name("web_fetch"),
            IntentType::WebAccess
        );
        assert_eq!(IntentType::from_tool_name("custom"), IntentType::Unknown);
    }

    #[test]
    fn test_decision_record_serialization() {
        let record = DecisionRecord {
            id: "test-id".into(),
            turn_id: "t1".into(),
            session_id: "s1".into(),
            tool_name: "read".into(),
            intent: "FileRead".into(),
            verdict_allowed: true,
            verdict_reason: None,
            firewall_result: Some(FirewallAudit {
                passed: true,
                stage_failed: None,
                violations: vec![],
                normalized_fields: vec!["filepath → path".into()],
            }),
            duration_ms: 5,
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: DecisionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tool_name, "read");
        assert!(back.verdict_allowed);
        assert!(back.firewall_result.unwrap().passed);
    }
}
