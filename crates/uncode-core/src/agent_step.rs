//! AgentStep — 面向离线训练的决策步骤模型
//!
//! 认知显化与决策驱动设计 决策层 §3.3 中的 AgentStep 模型：
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
    HumanApproval {
        approved: bool,
        comment: Option<String>,
    },
    TestPassed {
        test_name: String,
    },
    TestFailed {
        test_name: String,
        error: String,
    },
    AutoRevert {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn agent_step_construction() {
        let step = AgentStep {
            step_id: "step-1".into(),
            turn_id: "turn-1".into(),
            state_before: AgentStateSnapshot {
                phase: "coding".into(),
                turn_number: 1,
                active_tools: vec!["read".into(), "write".into()],
                context_size_tokens: 1024,
            },
            action: ExecutedAction {
                tool_name: "read".into(),
                arguments_summary: "path=src/main.rs".into(),
                duration_ms: 100,
            },
            observation: ActionObservation {
                success: true,
                output_summary: "file content".into(),
                files_changed: vec![],
                duration_ms: 50,
                terminate: false,
            },
            feedback: None,
            timestamp: Utc::now(),
        };
        assert_eq!(step.step_id, "step-1");
        assert_eq!(step.turn_id, "turn-1");
        assert!(step.observation.success);
        assert!(step.feedback.is_none());
    }

    #[test]
    fn agent_state_snapshot_fields() {
        let state = AgentStateSnapshot {
            phase: "planning".into(),
            turn_number: 3,
            active_tools: vec![],
            context_size_tokens: 2048,
        };
        assert_eq!(state.phase, "planning");
        assert_eq!(state.turn_number, 3);
        assert!(state.active_tools.is_empty());
        assert_eq!(state.context_size_tokens, 2048);
    }

    #[test]
    fn executed_action_construction() {
        let action = ExecutedAction {
            tool_name: "bash".into(),
            arguments_summary: "ls -la".into(),
            duration_ms: 500,
        };
        assert_eq!(action.tool_name, "bash");
        assert_eq!(action.arguments_summary, "ls -la");
        assert_eq!(action.duration_ms, 500);
    }

    #[test]
    fn action_observation_construction() {
        let obs = ActionObservation {
            success: true,
            output_summary: "done".into(),
            files_changed: vec!["a.rs".into()],
            duration_ms: 200,
            terminate: false,
        };
        assert!(obs.success);
        assert_eq!(obs.output_summary, "done");
        assert_eq!(obs.files_changed, vec!["a.rs"]);
        assert_eq!(obs.duration_ms, 200);
        assert!(!obs.terminate);
    }

    #[test]
    fn feedback_human_approval() {
        let f = Feedback::HumanApproval {
            approved: true,
            comment: Some("looks good".into()),
        };
        match &f {
            Feedback::HumanApproval { approved, comment } => {
                assert!(*approved);
                assert_eq!(comment.as_ref().unwrap(), "looks good");
            }
            _ => panic!("expected HumanApproval"),
        }
    }

    #[test]
    fn feedback_test_passed() {
        let f = Feedback::TestPassed {
            test_name: "test_foo".into(),
        };
        match &f {
            Feedback::TestPassed { test_name } => assert_eq!(test_name, "test_foo"),
            _ => panic!("expected TestPassed"),
        }
    }

    #[test]
    fn feedback_test_failed() {
        let f = Feedback::TestFailed {
            test_name: "test_foo".into(),
            error: "assertion failed".into(),
        };
        match &f {
            Feedback::TestFailed { test_name, error } => {
                assert_eq!(test_name, "test_foo");
                assert_eq!(error, "assertion failed");
            }
            _ => panic!("expected TestFailed"),
        }
    }

    #[test]
    fn feedback_auto_revert() {
        let f = Feedback::AutoRevert {
            reason: "build broken".into(),
        };
        match &f {
            Feedback::AutoRevert { reason } => assert_eq!(reason, "build broken"),
            _ => panic!("expected AutoRevert"),
        }
    }

    #[test]
    fn debug_clone_derives() {
        let state = AgentStateSnapshot {
            phase: "t".into(),
            turn_number: 0,
            active_tools: vec![],
            context_size_tokens: 0,
        };
        let _dbg = format!("{:?}", state);
        let _cloned = state.clone();
    }
}
