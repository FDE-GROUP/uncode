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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    fn name(&self) -> &str;
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

    pub fn add_policy(&mut self, policy: Box<dyn DecisionPolicy>) {
        self.policies.push(policy);
    }

    pub fn remove_policy_by_name(&mut self, name: &str) {
        self.policies.retain(|p| p.name() != name);
    }

    pub fn replace_policy_by_name(&mut self, name: &str, policy: Box<dyn DecisionPolicy>) {
        if let Some(pos) = self.policies.iter().position(|p| p.name() == name) {
            self.policies[pos] = policy;
        } else {
            self.policies.push(policy);
        }
    }

    /// 对所有策略依次裁决。任一策略拒绝则立即返回。
    /// 通过时聚合所有策略的 violations 到 warnings。
    #[must_use]
    pub fn adjudicate(
        &self,
        action: &NormalizedAction,
        context: &DecisionContext,
    ) -> Result<ApprovedAction, AdjudicationError> {
        let mut warnings = Vec::new();
        for policy in &self.policies {
            let verdict = policy.evaluate(context, action)?;
            if !verdict.allowed {
                return Err(AdjudicationError::Denied {
                    policy: policy.name().to_owned(),
                    reason: verdict.reason.unwrap_or_default(),
                });
            }
            warnings.extend(verdict.violations);
        }
        Ok(ApprovedAction {
            action: Arc::new(action.clone()),
            adjudicated_at: chrono::Utc::now(),
            warnings,
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

impl Clone for PhaseGuardPolicy {
    fn clone(&self) -> Self {
        Self {
            phase: std::sync::Mutex::new(self.current_phase()),
        }
    }
}

impl PhaseGuardPolicy {
    pub fn new(initial: AgentHarnessPhase) -> Self {
        Self {
            phase: std::sync::Mutex::new(initial),
        }
    }

    /// 更新当前 Phase（由 AgentHarness 在状态转换时调用）
    pub fn set_phase(&self, new_phase: AgentHarnessPhase) {
        match self.phase.lock() {
            Ok(mut p) => *p = new_phase,
            Err(e) => {
                tracing::warn!("phase lock poisoned, recovering");
                *e.into_inner() = new_phase;
                self.phase.clear_poison();
            }
        }
    }

    pub fn current_phase(&self) -> AgentHarnessPhase {
        match self.phase.lock() {
            Ok(p) => p.clone(),
            Err(e) => {
                let guard = e.into_inner();
                let phase = guard.clone();
                drop(guard);
                self.phase.clear_poison();
                phase
            }
        }
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
            AgentHarnessPhase::Idle | AgentHarnessPhase::Turn => Ok(DecisionVerdict::approved()),
            _ => Ok(DecisionVerdict::denied(format!(
                "agent is in {phase} phase, not accepting new actions"
            ))),
        }
    }
    fn name(&self) -> &str {
        "phase_guard"
    }
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
    fn name(&self) -> &str {
        "turn_limit"
    }
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
    fn name(&self) -> &str {
        "cancellation"
    }
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
    fn name(&self) -> &str {
        "concurrency"
    }
}

// ── EffectBasedPolicy ────────────────────────────────────

/// 基于本体 Effect 的裁决策略 — 记录只读/非只读状态
///
/// 在 Policy chain 中，每个 policy 返回 `approved()` 表示"本策略不拒绝"，
/// 返回 `Denied` 表示"本策略阻止"。
///
/// 当前实现不阻止任何动作（所有分支都 approved），仅作为 Effect 信息的占位。
/// 后续可通过在 verdict.violations 中记录非只读警告，或对特定非只读动作
/// 返回 Denied 来增强。
pub struct EffectBasedPolicy {
    registry: uncode_ontology::TypeRegistry,
    auto_approve_readonly: bool,
}

impl EffectBasedPolicy {
    pub fn new(registry: uncode_ontology::TypeRegistry, auto_approve_readonly: bool) -> Self {
        Self {
            registry,
            auto_approve_readonly,
        }
    }
}

impl DecisionPolicy for EffectBasedPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        if !self.auto_approve_readonly {
            return Ok(DecisionVerdict::approved());
        }

        let Some(action_def) = self.registry.get_action(&action.tool_name) else {
            return Ok(DecisionVerdict::approved());
        };

        if action_def.is_read_only() {
            Ok(DecisionVerdict::approved())
        } else {
            let effect_list = action_def
                .effects
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Ok(DecisionVerdict {
                allowed: true,
                reason: Some(format!(
                    "action '{}' has non-read effects: {effect_list}",
                    action.tool_name,
                )),
                violations: vec![format!(
                    "non_read_effects: {} — {effect_list}",
                    action.tool_name
                )],
            })
        }
    }

    fn name(&self) -> &str {
        "effect_based"
    }
}

// ── CustomPolicy ────────────────────────────────────────

/// 自定义裁决策略 — 从 AdjudicationPolicyConfig 的 rules[] 构建
///
/// 每条 PolicyRule 包含一个 pattern（工具名匹配）和 action（Block/Allow/AskUser）。
/// AskUser 当前按 Block 处理，完整集成待后续迭代。
pub struct CustomPolicy {
    policy_name: String,
    rules: Vec<uncode_shared::guardrails::PolicyRule>,
}

impl CustomPolicy {
    pub fn from_config(config: &uncode_shared::guardrails::AdjudicationPolicyConfig) -> Self {
        Self {
            policy_name: config.name.clone(),
            rules: config.rules.clone(),
        }
    }
}

impl DecisionPolicy for CustomPolicy {
    fn evaluate(
        &self,
        _context: &DecisionContext,
        action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        for rule in &self.rules {
            if action.tool_name == rule.pattern || rule.pattern == "*" {
                match rule.action {
                    uncode_shared::guardrails::PolicyAction::Block => {
                        return Ok(DecisionVerdict::denied(format!(
                            "blocked by policy '{}': tool '{}' matches rule '{}'",
                            self.policy_name, action.tool_name, rule.pattern
                        )));
                    }
                    uncode_shared::guardrails::PolicyAction::BlockAndWarn => {
                        return Ok(DecisionVerdict::denied(format!(
                            "blocked by policy '{}': tool '{}' (warn)",
                            self.policy_name, action.tool_name
                        )));
                    }
                    uncode_shared::guardrails::PolicyAction::AskUser => {
                        // First version: treat as block
                        return Ok(DecisionVerdict::denied(format!(
                            "policy '{}' requires user approval for tool '{}'",
                            self.policy_name, action.tool_name
                        )));
                    }
                    uncode_shared::guardrails::PolicyAction::Allow => {
                        // Explicitly allowed, continue to next rule
                        continue;
                    }
                }
            }
        }
        Ok(DecisionVerdict::approved())
    }

    fn name(&self) -> &str {
        &self.policy_name
    }
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
            total_input_tokens: 0,
            total_output_tokens: 0,
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

    #[test]
    fn test_phase_guard_poison_recovery() {
        // 通过故意 poison 内部 mutex 来验证 set_phase 的恢复能力
        let policy = PhaseGuardPolicy::new(AgentHarnessPhase::Idle);

        // Poison the internal mutex: lock it, then panic while holding the lock
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = policy.phase.lock().unwrap();
            panic!("intentional poison for PhaseGuardPolicy");
        }));
        assert!(result.is_err(), "expected panic to poison the mutex");

        // set_phase should recover from poison and set the new phase
        policy.set_phase(AgentHarnessPhase::Turn);
        assert_eq!(policy.current_phase(), AgentHarnessPhase::Turn);

        // Verify it still works for evaluation
        let verdict = policy.evaluate(&make_context(1), &make_action()).unwrap();
        assert!(verdict.allowed);
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
        assert!(
            err.contains("cancellation"),
            "expected cancellation error, got: {err}"
        );
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

    // ── EffectBasedPolicy ──

    #[test]
    fn test_effect_based_approves_readonly_when_enabled() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let policy = EffectBasedPolicy::new(ontology, true);
        let action = NormalizedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
            normalized_fields: vec![],
        };
        let verdict = policy.evaluate(&make_context(1), &action).unwrap();
        assert!(verdict.allowed);
    }

    #[test]
    fn test_effect_based_approves_non_readonly() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let policy = EffectBasedPolicy::new(ontology, true);
        let action = NormalizedAction {
            tool_name: "write".into(),
            arguments: serde_json::json!({"path": "src/main.rs", "content": "hello"}),
            normalized_fields: vec![],
        };
        let verdict = policy.evaluate(&make_context(1), &action).unwrap();
        // EffectBasedPolicy always approves — non-readonly goes through to permission gate
        assert!(verdict.allowed);
    }

    #[test]
    fn test_effect_based_disabled_passes_through() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let policy = EffectBasedPolicy::new(ontology, false);
        let action = NormalizedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
            normalized_fields: vec![],
        };
        let verdict = policy.evaluate(&make_context(1), &action).unwrap();
        assert!(verdict.allowed);
    }

    #[test]
    fn test_effect_based_unknown_tool_passes() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let policy = EffectBasedPolicy::new(ontology, true);
        let action = NormalizedAction {
            tool_name: "custom_tool".into(),
            arguments: serde_json::json!({}),
            normalized_fields: vec![],
        };
        let verdict = policy.evaluate(&make_context(1), &action).unwrap();
        assert!(verdict.allowed, "unknown tools should pass through");
    }

    // ── CustomPolicy ──

    #[test]
    fn test_custom_policy_blocks_matching_tool() {
        let config = uncode_shared::guardrails::AdjudicationPolicyConfig {
            name: "no_bash".into(),
            enabled: true,
            rules: vec![uncode_shared::guardrails::PolicyRule {
                pattern: "bash".into(),
                action: uncode_shared::guardrails::PolicyAction::Block,
            }],
        };
        let policy = CustomPolicy::from_config(&config);
        let action = NormalizedAction {
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command": "ls"}),
            normalized_fields: vec![],
        };
        let verdict = policy.evaluate(&make_context(1), &action).unwrap();
        assert!(!verdict.allowed);
    }

    #[test]
    fn test_custom_policy_allows_non_matching() {
        let config = uncode_shared::guardrails::AdjudicationPolicyConfig {
            name: "no_bash".into(),
            enabled: true,
            rules: vec![uncode_shared::guardrails::PolicyRule {
                pattern: "bash".into(),
                action: uncode_shared::guardrails::PolicyAction::Block,
            }],
        };
        let policy = CustomPolicy::from_config(&config);
        let action = NormalizedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "a.rs"}),
            normalized_fields: vec![],
        };
        let verdict = policy.evaluate(&make_context(1), &action).unwrap();
        assert!(verdict.allowed);
    }

    #[test]
    fn test_custom_policy_wildcard_blocks_all() {
        let config = uncode_shared::guardrails::AdjudicationPolicyConfig {
            name: "block_all".into(),
            enabled: true,
            rules: vec![uncode_shared::guardrails::PolicyRule {
                pattern: "*".into(),
                action: uncode_shared::guardrails::PolicyAction::Block,
            }],
        };
        let policy = CustomPolicy::from_config(&config);
        let action = NormalizedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "a.rs"}),
            normalized_fields: vec![],
        };
        let verdict = policy.evaluate(&make_context(1), &action).unwrap();
        assert!(!verdict.allowed, "wildcard should block all tools");
    }

    #[test]
    fn test_add_policy_appends_to_chain() {
        let adj = Adjudicator::new(vec![]);
        assert_eq!(adj.policies.len(), 0);

        let mut adj = adj;
        adj.add_policy(Box::new(TurnLimitPolicy::new(50)));
        assert_eq!(adj.policies.len(), 1);

        adj.add_policy(Box::new(CancellationPolicy::new(CancellationToken::new())));
        assert_eq!(adj.policies.len(), 2);
    }

    #[test]
    fn test_remove_policy_by_name() {
        let mut adj = Adjudicator::new(vec![
            Box::new(TurnLimitPolicy::new(50)),
            Box::new(CancellationPolicy::new(CancellationToken::new())),
        ]);
        assert_eq!(adj.policies.len(), 2);
        adj.remove_policy_by_name("turn_limit");
        assert_eq!(adj.policies.len(), 1);
        assert_eq!(adj.policies[0].name(), "cancellation");
    }

    #[test]
    fn test_remove_policy_by_name_nonexistent() {
        let mut adj = Adjudicator::new(vec![Box::new(TurnLimitPolicy::new(50))]);
        adj.remove_policy_by_name("nonexistent");
        assert_eq!(adj.policies.len(), 1);
    }

    #[test]
    fn test_replace_policy_by_name_existing() {
        let mut adj = Adjudicator::new(vec![
            Box::new(TurnLimitPolicy::new(50)),
            Box::new(CancellationPolicy::new(CancellationToken::new())),
        ]);
        adj.replace_policy_by_name("turn_limit", Box::new(TurnLimitPolicy::new(100)));
        assert_eq!(adj.policies.len(), 2);
        assert_eq!(adj.policies[0].name(), "turn_limit");
        let ctx = make_context(1);
        let action = make_action();
        let result = adj.policies[0].evaluate(&ctx, &action).unwrap();
        assert!(result.allowed);
    }

    #[test]
    fn test_replace_policy_by_name_nonexistent_appends() {
        let mut adj = Adjudicator::new(vec![Box::new(TurnLimitPolicy::new(50))]);
        adj.replace_policy_by_name(
            "cancellation",
            Box::new(CancellationPolicy::new(CancellationToken::new())),
        );
        assert_eq!(adj.policies.len(), 2);
        assert_eq!(adj.policies[0].name(), "turn_limit");
        assert_eq!(adj.policies[1].name(), "cancellation");
    }

    // ── ConcurrencyPolicy ──

    #[test]
    fn test_concurrency_policy_denies_when_inactive() {
        let active = Arc::new(AtomicBool::new(false));
        let policy = ConcurrencyPolicy::new(active);
        let result = policy.evaluate(&make_context(1), &make_action()).unwrap();
        assert!(!result.allowed);
    }

    // ── PhaseGuardPolicy clone ──

    #[test]
    fn test_phase_guard_clone() {
        let pg1 = PhaseGuardPolicy::new(AgentHarnessPhase::Idle);
        let pg2 = pg1.clone();
        assert_eq!(pg1.current_phase(), pg2.current_phase());
        pg1.set_phase(AgentHarnessPhase::Turn);
        // clone is independent — pg2 should still be Idle
        assert_eq!(pg2.current_phase(), AgentHarnessPhase::Idle);
        assert_eq!(pg1.current_phase(), AgentHarnessPhase::Turn);
    }
}
