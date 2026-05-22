//! 审计器 — 决策轨迹的记录与回放
//!
//! ## 职责
//!
//! - 记录每次裁决和执行结果为 `DecisionRecord`
//! - 生成面向离线训练的 `AgentStep`
//! - 提供历史决策的查询和回放接口
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 决策层

use super::types::{ActionObservation, AgentStateSnapshot, AgentStep, DecisionRecord, ExecutedAction, Feedback};

/// 审计器 — 决策轨迹的管理者
pub struct Auditor {
    pub trail: Vec<DecisionRecord>,
}

impl Auditor {
    pub fn new() -> Self {
        Self { trail: Vec::new() }
    }

    /// 记录一次裁决 + 执行结果
    pub fn record(
        &mut self,
        record: DecisionRecord,
    ) {
        self.trail.push(record);
    }

    /// 从最近一次记录生成 AgentStep（面向离线训练）
    pub fn generate_step(
        &self,
        turn_id: impl Into<String>,
        state_before: AgentStateSnapshot,
        observation: ActionObservation,
        feedback: Option<Feedback>,
    ) -> Option<AgentStep> {
        self.trail.last().map(|record| {
            let action = record.approved_action.as_ref();
            AgentStep {
                step_id: uuid::Uuid::new_v4().to_string(),
                turn_id: turn_id.into(),
                state_before,
                action: ExecutedAction {
                    tool_name: action.map(|a| a.action.tool_name.clone()).unwrap_or_default(),
                    arguments_summary: String::new(),
                    duration_ms: 0,
                },
                observation,
                feedback,
                timestamp: chrono::Utc::now(),
            }
        })
    }

    /// 返回完整决策轨迹
    pub fn trail(&self) -> &[DecisionRecord] {
        &self.trail
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{
        ActionProposal, ApprovedAction, DecisionVerdict, NormalizedAction,
    };

    fn make_record(approved: bool, tool: &str) -> DecisionRecord {
        let proposal = ActionProposal {
            tool_name: tool.into(), raw_arguments: serde_json::json!({}),
            rationale: None, confidence: None,
        };
        let verdict = if approved {
            DecisionVerdict::approved()
        } else {
            DecisionVerdict::denied("test rejection")
        };
        let approved_action = if approved {
            Some(ApprovedAction {
                action: NormalizedAction {
                    tool_name: tool.into(), arguments: serde_json::json!({}),
                    normalized_fields: vec![],
                },
                adjudicated_at: chrono::Utc::now(),
            })
        } else {
            None
        };
        DecisionRecord {
            turn_id: "turn-1".into(), proposal, verdict, approved_action,
            timestamp: chrono::Utc::now(), adjudication_duration_ms: 5,
        }
    }

    fn make_snapshot() -> AgentStateSnapshot {
        AgentStateSnapshot { phase: "turn".into(), turn_number: 1, active_tools: vec![], context_size_tokens: 1000 }
    }

    fn make_observation(success: bool) -> ActionObservation {
        ActionObservation { success, output_summary: "test".into(), files_changed: vec![], duration_ms: 10, terminate: false }
    }

    #[test]
    fn test_empty_auditor_has_no_trail() {
        let auditor = Auditor::new();
        assert!(auditor.trail().is_empty());
    }

    #[test]
    fn test_record_appends_to_trail() {
        let mut auditor = Auditor::new();
        auditor.record(make_record(true, "read"));
        auditor.record(make_record(false, "write"));
        assert_eq!(auditor.trail().len(), 2);
    }

    #[test]
    fn test_generate_step_from_approved_action() {
        let mut auditor = Auditor::new();
        auditor.record(make_record(true, "read"));
        let step = auditor.generate_step(
            "turn-1", make_snapshot(), make_observation(true), None,
        );
        assert!(step.is_some());
        let s = step.unwrap();
        assert_eq!(s.action.tool_name, "read");
        assert!(s.observation.success);
    }

    #[test]
    fn test_generate_step_from_denied_action() {
        let mut auditor = Auditor::new();
        auditor.record(make_record(false, "rm"));
        let step = auditor.generate_step(
            "turn-1", make_snapshot(), make_observation(false),
            Some(Feedback::AutoRevert { reason: "blocked by policy".into() }),
        );
        assert!(step.is_some());
        let s = step.unwrap();
        assert_eq!(s.action.tool_name, ""); // denied = no action
        assert!(matches!(s.feedback, Some(Feedback::AutoRevert { .. })));
    }

    #[test]
    fn test_generate_step_returns_none_when_empty() {
        let auditor = Auditor::new();
        let step = auditor.generate_step("turn-1", make_snapshot(), make_observation(true), None);
        assert!(step.is_none());
    }

    #[test]
    fn test_trail_is_immutable_view() {
        let mut auditor = Auditor::new();
        auditor.record(make_record(true, "read"));
        let trail = auditor.trail();
        assert_eq!(trail.len(), 1);
        // trail() 返回不可变引用
    }
}
