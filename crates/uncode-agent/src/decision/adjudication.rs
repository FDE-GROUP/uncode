//! 裁决器 — 决策层的合法性判定中心
//!
//! ## 角色
//!
//! 裁决器不执行动作，只判断"能不能做"。
//! LLM 负责生成候选方案，裁决器负责决定哪些方案被批准。
//!
//! ## 与现有代码的关系
//!
//! `DecisionPolicy` trait 实现**包装**现有的决策检查点：
//!
//! | DecisionPolicy 实现 | 包装的现有逻辑 |
//! |:---|:---|
//! | `PhaseGuardPolicy` | `AgentHarnessPhase` 状态机（Idle/Turn/...） |
//! | `TurnLimitPolicy` | `MAX_TURNS=50` 常量检查 |
//! | `CancellationPolicy` | `CancellationToken` 5 个检查点 |
//! | `ConcurrencyPolicy` | `active_run` CAS 检查 |
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 决策层

use super::types::{ApprovedAction, DecisionContext, DecisionVerdict, NormalizedAction};

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
    pub async fn adjudicate(
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

// ── 内置策略（后续 commit 中实现，包装现有逻辑）─────────

/// 包装 `AgentHarnessPhase` — 仅在 Idle/Turn 阶段接收提案
pub struct PhaseGuardPolicy;

impl DecisionPolicy for PhaseGuardPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        _action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        // TODO(decision-refactor): 检查 AgentHarnessPhase
        Ok(DecisionVerdict::approved())
    }
    fn name(&self) -> &'static str { "phase_guard" }
}

/// 包装 `MAX_TURNS=50` — 超过上限则拒绝新动作
pub struct TurnLimitPolicy { pub max_turns: u32 }

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

/// 包装 `CancellationToken` — 检查是否已被取消
pub struct CancellationPolicy;

impl DecisionPolicy for CancellationPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        _action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        // TODO(decision-refactor): 检查 CancellationToken
        Ok(DecisionVerdict::approved())
    }
    fn name(&self) -> &'static str { "cancellation" }
}

/// 包装 `active_run` CAS 检查 — 保证单实例运行
pub struct ConcurrencyPolicy;

impl DecisionPolicy for ConcurrencyPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        _action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        // TODO(decision-refactor): 检查 active_run AtomicBool
        Ok(DecisionVerdict::approved())
    }
    fn name(&self) -> &'static str { "concurrency" }
}
