//! 治理层 — PhaseStateMachine + EventRouter 集成
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.4 治理层

pub mod state_machine;

pub use state_machine::{AgentPhase, GovernanceError, PhaseStateMachine, PhaseTransition};
