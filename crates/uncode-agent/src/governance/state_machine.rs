//! 六状态 PhaseStateMachine — 治理层的核心可观测性组件
//!
//! 显式建模 Agent 在每个 turn 内的认知→裁决→执行循环。
//! 不替代 `AgentHarnessPhase`，而是作为其子状态提供细粒度追踪。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Agent 在 turn 内的六阶段状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPhase {
    Init,
    Cognizing,
    Adjudicating,
    Executing,
    WaitingForUser,
    Terminated,
}

impl std::fmt::Display for AgentPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init => write!(f, "Init"),
            Self::Cognizing => write!(f, "Cognizing"),
            Self::Adjudicating => write!(f, "Adjudicating"),
            Self::Executing => write!(f, "Executing"),
            Self::WaitingForUser => write!(f, "WaitingForUser"),
            Self::Terminated => write!(f, "Terminated"),
        }
    }
}

/// 一次状态转换记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTransition {
    pub from: AgentPhase,
    pub to: AgentPhase,
    pub timestamp: DateTime<Utc>,
    pub trigger: String,
}

/// 合法转换表
static ALLOWED_TRANSITIONS: &[(AgentPhase, &[AgentPhase])] = &[
    (AgentPhase::Init, &[AgentPhase::Cognizing]),
    (
        AgentPhase::Cognizing,
        &[
            AgentPhase::Adjudicating,
            AgentPhase::WaitingForUser,
            AgentPhase::Terminated,
        ],
    ),
    (
        AgentPhase::Adjudicating,
        &[
            AgentPhase::Executing,
            AgentPhase::Cognizing,
            AgentPhase::Terminated,
        ],
    ),
    (
        AgentPhase::Executing,
        &[
            AgentPhase::Cognizing,
            AgentPhase::WaitingForUser,
            AgentPhase::Terminated,
        ],
    ),
    (
        AgentPhase::WaitingForUser,
        &[AgentPhase::Cognizing, AgentPhase::Terminated],
    ),
    (AgentPhase::Terminated, &[]),
];

/// 六状态机 — 跟踪 Agent 在每个 turn 内的认知→裁决→执行循环
pub struct PhaseStateMachine {
    current: AgentPhase,
    history: Vec<PhaseTransition>,
}

impl PhaseStateMachine {
    pub fn new() -> Self {
        Self {
            current: AgentPhase::Init,
            history: Vec::new(),
        }
    }

    pub fn current(&self) -> AgentPhase {
        self.current
    }

    pub fn transition(&mut self, to: AgentPhase, trigger: &str) -> Result<(), GovernanceError> {
        let allowed = ALLOWED_TRANSITIONS
            .iter()
            .find(|(from, _)| *from == self.current)
            .map(|(_, targets)| targets.contains(&to))
            .unwrap_or(false);

        if !allowed {
            return Err(GovernanceError::InvalidPhaseTransition {
                from: self.current,
                to,
            });
        }

        self.history.push(PhaseTransition {
            from: self.current,
            to,
            timestamp: Utc::now(),
            trigger: trigger.to_string(),
        });
        self.current = to;
        Ok(())
    }

    pub fn history(&self) -> &[PhaseTransition] {
        &self.history
    }
}

impl Default for PhaseStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("invalid phase transition: {from:?} → {to:?}")]
    InvalidPhaseTransition { from: AgentPhase, to: AgentPhase },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_full_lifecycle() {
        let mut sm = PhaseStateMachine::new();
        assert_eq!(sm.current(), AgentPhase::Init);

        sm.transition(AgentPhase::Cognizing, "user_prompt").unwrap();
        sm.transition(AgentPhase::Adjudicating, "tool_call_received")
            .unwrap();
        sm.transition(AgentPhase::Executing, "adjudication_approved")
            .unwrap();
        sm.transition(AgentPhase::Cognizing, "tool_execution_complete")
            .unwrap();
        sm.transition(AgentPhase::WaitingForUser, "turn_complete")
            .unwrap();
        sm.transition(AgentPhase::Terminated, "session_end")
            .unwrap();

        assert_eq!(sm.history().len(), 6);
    }

    #[test]
    fn test_invalid_transition_from_init() {
        let mut sm = PhaseStateMachine::new();
        let result = sm.transition(AgentPhase::Executing, "skip");
        assert!(result.is_err());
        assert_eq!(sm.current(), AgentPhase::Init);
    }

    #[test]
    fn test_terminated_no_exit() {
        let mut sm = PhaseStateMachine::new();
        sm.transition(AgentPhase::Cognizing, "start").unwrap();
        sm.transition(AgentPhase::Terminated, "end").unwrap();

        let result = sm.transition(AgentPhase::Init, "restart");
        assert!(result.is_err());
        assert_eq!(sm.current(), AgentPhase::Terminated);
    }

    #[test]
    fn test_cognizing_to_waiting_directly() {
        let mut sm = PhaseStateMachine::new();
        sm.transition(AgentPhase::Cognizing, "start").unwrap();
        sm.transition(AgentPhase::WaitingForUser, "no_tool_calls")
            .unwrap();
        assert_eq!(sm.current(), AgentPhase::WaitingForUser);
    }

    #[test]
    fn test_react_loop_cycles() {
        let mut sm = PhaseStateMachine::new();
        sm.transition(AgentPhase::Cognizing, "start").unwrap();

        // Two ReAct cycles
        for i in 0..2 {
            sm.transition(AgentPhase::Adjudicating, &format!("tool_{i}"))
                .unwrap();
            sm.transition(AgentPhase::Executing, &format!("approved_{i}"))
                .unwrap();
            sm.transition(AgentPhase::Cognizing, &format!("done_{i}"))
                .unwrap();
        }

        sm.transition(AgentPhase::WaitingForUser, "turn_complete")
            .unwrap();
        assert_eq!(sm.history().len(), 8); // start + 3*2 + waiting
    }
}
