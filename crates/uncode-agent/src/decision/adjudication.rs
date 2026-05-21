//! 裁决器 — 决策层的合法性判定中心
//!
//! ## 角色
//!
//! 裁决器不执行动作，只判断"能不能做"。
//! LLM 负责生成候选方案，裁决器负责决定哪些方案被批准。
//!
//! ## DecisionPolicy 实现
//!
//! | Policy | 包装的现有逻辑 | 状态 |
//! |:---|:---|:---|
//! | `PhaseGuardPolicy` | `AgentHarnessPhase` 状态机 | ✅ |
//! | `TurnLimitPolicy` | `MAX_TURNS=50` 常量 | ✅ |
//! | `CancellationPolicy` | `CancellationToken` | ✅ |
//! | `ConcurrencyPolicy` | `active_run` CAS | ✅ |
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 决策层

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::harness::AgentHarnessPhase;

use super::types::{ApprovedAction, DecisionContext, DecisionVerdict, NormalizedAction};

// ── Adjudicator ─────────────────────────────────────────

/// 裁决器 — 编排 DecisionPolicy 链
pub struct Adjudicator {
    policies: Vec<Box<dyn DecisionPolicy>>,
}

/// 单条裁决策略
pub trait DecisionPolicy: Send + Sync {
    fn evaluate(
        &self,
        context: &DecisionContext,
        action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError>;
    fn name(&self) -> &'static str;
}

#[derive(Debug, thiserror::Error)]
pub enum AdjudicationError {
    #[error("denied by policy '{policy}': {reason}")]
    Denied { policy: String, reason: String },
}

impl Adjudicator {
    pub fn new(policies: Vec<Box<dyn DecisionPolicy>>) -> Self {
        Self { policies }
    }

    /// 对所有策略依次裁决。任一策略拒绝则立即返回。
    pub fn adjudicate(
        &self,
        action: &NormalizedAction,
        context: &DecisionContext,
    ) -> Result<ApprovedAction, AdjudicationError> {
        for policy in &self.policies {
            let verdict = policy.evaluate(context, action)?;
            if !verdict.allowed {
                return Err(AdjudicationError::Denied {
                    policy: policy.name().to_string(),
                    reason: verdict.reason.unwrap_or_default(),
                });
            }
        }
        Ok(ApprovedAction {
            action: action.clone(),
            adjudicated_at: chrono::Utc::now(),
        })
    }
}

// ═══════════════════════════════════════════════════════════
// 内置 DecisionPolicy 实现
// ═══════════════════════════════════════════════════════════

// ── PhaseGuardPolicy ────────────────────────────────────

/// 仅在 Idle/Turn 阶段允许接收提案
///
/// 包装 `AgentHarnessPhase` 状态机。
/// Compaction/BranchSummary/Retry 期间拒绝新动作。
pub struct PhaseGuardPolicy {
    phase: std::sync::Mutex<AgentHarnessPhase>,
}

impl PhaseGuardPolicy {
    pub fn new(initial: AgentHarnessPhase) -> Self {
        Self { phase: std::sync::Mutex::new(initial) }
    }

    /// 更新当前 Phase（由 AgentHarness 在状态转换时调用）
    pub fn set_phase(&self, new_phase: AgentHarnessPhase) {
        if let Ok(mut p) = self.phase.lock() {
            *p = new_phase;
        }
    }

    /// 读取当前 Phase
    pub fn current_phase(&self) -> AgentHarnessPhase {
        self.phase.lock().map(|p| p.clone()).unwrap_or(AgentHarnessPhase::Idle)
    }
}

impl DecisionPolicy for PhaseGuardPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        _action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        let phase = self.current_phase();
        match phase {
            AgentHarnessPhase::Idle | AgentHarnessPhase::Turn => {
                Ok(DecisionVerdict::approved())
            }
            _ => Ok(DecisionVerdict::denied(format!(
                "agent is in {phase} phase, not accepting new actions"
            ))),
        }
    }
    fn name(&self) -> &'static str { "phase_guard" }
}

// ── TurnLimitPolicy ─────────────────────────────────────

/// 超过 MAX_TURNS 时拒绝新动作
pub struct TurnLimitPolicy {
    max_turns: u32,
}

impl TurnLimitPolicy {
    pub fn new(max_turns: u32) -> Self {
        Self { max_turns }
    }
}

impl DecisionPolicy for TurnLimitPolicy {
    fn evaluate(
        &self,
        context: &DecisionContext,
        _action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        if context.turn_number >= self.max_turns {
            return Ok(DecisionVerdict::denied(format!(
                "turn limit reached: {} >= {}",
                context.turn_number, self.max_turns
            )));
        }
        Ok(DecisionVerdict::approved())
    }
    fn name(&self) -> &'static str { "turn_limit" }
}

// ── CancellationPolicy ──────────────────────────────────

/// 检查 CancellationToken 是否已被触发
pub struct CancellationPolicy {
    token: CancellationToken,
}

impl CancellationPolicy {
    pub fn new(token: CancellationToken) -> Self {
        Self { token }
    }
}

impl DecisionPolicy for CancellationPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        _action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        if self.token.is_cancelled() {
            return Ok(DecisionVerdict::denied("cancellation requested"));
        }
        Ok(DecisionVerdict::approved())
    }
    fn name(&self) -> &'static str { "cancellation" }
}

// ── ConcurrencyPolicy ───────────────────────────────────

/// 通过 CAS 保证单实例运行
pub struct ConcurrencyPolicy {
    active: Arc<AtomicBool>,
}

impl ConcurrencyPolicy {
    pub fn new(active: Arc<AtomicBool>) -> Self {
        Self { active }
    }
}

impl DecisionPolicy for ConcurrencyPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        _action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        if self.active.load(Ordering::Acquire) {
            // 已经有一个活跃运行，但这是裁决新动作，不应拒绝
            // 这个 policy 在 AgentHarness 初始化时使用 CAS 做准入控制
            // 在正常的 turn 内裁决时，active 应该是 true（正在运行）
            // 所以这里只检查 spurious 情况
            return Ok(DecisionVerdict::approved());
        }
        Ok(DecisionVerdict::denied("no active agent run"))
    }
    fn name(&self) -> &'static str { "concurrency" }
}

// ── Builder ─────────────────────────────────────────────

/// 使用默认参数构建完整的 Adjudicator
///
/// 策略顺序：Phase → TurnLimit → Cancellation → Concurrency
pub fn build_default_adjudicator(
    phase_policy: PhaseGuardPolicy,
    token: CancellationToken,
    max_turns: u32,
    active: Arc<AtomicBool>,
) -> Adjudicator {
    Adjudicator::new(vec![
        Box::new(phase_policy),
        Box::new(TurnLimitPolicy::new(max_turns)),
        Box::new(CancellationPolicy::new(token)),
        Box::new(ConcurrencyPolicy::new(active)),
    ])
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_action() -> NormalizedAction {
        NormalizedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
            normalized_fields: vec![],
        }
    }

    fn make_context(turn: u32) -> DecisionContext {
        DecisionContext {
            turn_number: turn,
            max_turns: 50,
            active_tools: vec!["read".into()],
        }
    }

    // ── PhaseGuardPolicy ──

    #[test]
    fn test_phase_guard_allows_idle() {
        let policy = PhaseGuardPolicy::new(AgentHarnessPhase::Idle);
        let verdict = policy.evaluate(&make_context(1), &make_action()).unwrap();
        assert!(verdict.allowed);
    }

    #[test]
    fn test_phase_guard_allows_turn() {
        let policy = PhaseGuardPolicy::new(AgentHarnessPhase::Turn);
        let verdict = policy.evaluate(&make_context(1), &make_action()).unwrap();
        assert!(verdict.allowed);
    }

    #[test]
    fn test_phase_guard_blocks_compaction() {
        let policy = PhaseGuardPolicy::new(AgentHarnessPhase::Compaction);
        let verdict = policy.evaluate(&make_context(1), &make_action()).unwrap();
        assert!(!verdict.allowed);
    }

    // ── TurnLimitPolicy ──

    #[test]
    fn test_turn_limit_allows_below_max() {
        let policy = TurnLimitPolicy::new(50);
        let verdict = policy.evaluate(&make_context(30), &make_action()).unwrap();
        assert!(verdict.allowed);
    }

    #[test]
    fn test_turn_limit_blocks_at_max() {
        let policy = TurnLimitPolicy::new(50);
        let verdict = policy.evaluate(&make_context(50), &make_action()).unwrap();
        assert!(!verdict.allowed);
    }

    #[test]
    fn test_turn_limit_blocks_above_max() {
        let policy = TurnLimitPolicy::new(50);
        let verdict = policy.evaluate(&make_context(51), &make_action()).unwrap();
        assert!(!verdict.allowed);
    }

    // ── CancellationPolicy ──

    #[test]
    fn test_cancellation_allows_when_not_cancelled() {
        let token = CancellationToken::new();
        let policy = CancellationPolicy::new(token);
        let verdict = policy.evaluate(&make_context(1), &make_action()).unwrap();
        assert!(verdict.allowed);
    }

    #[test]
    fn test_cancellation_blocks_when_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        let policy = CancellationPolicy::new(token);
        let verdict = policy.evaluate(&make_context(1), &make_action()).unwrap();
        assert!(!verdict.allowed);
    }

    // ── Adjudicator chain ──

    #[test]
    fn test_adjudicator_chain_rejects_on_first_failure() {
        let token = CancellationToken::new();
        token.cancel(); // 第一个就拒绝
        let adj = build_default_adjudicator(
            PhaseGuardPolicy::new(AgentHarnessPhase::Idle),
            token,
            50,
            Arc::new(AtomicBool::new(true)),
        );
        let result = adj.adjudicate(&make_action(), &make_context(1));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cancellation"), "expected cancellation error, got: {err}");
    }

    #[test]
    fn test_adjudicator_chain_allows_when_all_pass() {
        let adj = build_default_adjudicator(
            PhaseGuardPolicy::new(AgentHarnessPhase::Idle),
            CancellationToken::new(),
            50,
            Arc::new(AtomicBool::new(true)),
        );
        let result = adj.adjudicate(&make_action(), &make_context(1));
        assert!(result.is_ok());
    }
}
