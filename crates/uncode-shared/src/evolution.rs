//! 自适应进化 — Harness Engineering §5
//!
//! ## 定位
//!
//! 认知显化与决策驱动设计 治理层 工程实践子层的第五模块。
//! 学术论文《AI Harness Engineering》的 Harness Evolution Loop：
//!
//! ```text
//! Agent 执行 → Evaluator 评估 → Evolution Engine 识别模式
//!   → 建议 Mutation → 应用 Mutation → 新配置生效 → 继续执行
//! ```
//!
//! ## 当前实现
//!
//! 实现"模式识别 + 建议"阶段（生成 Mutation 建议）。
//! "自动应用"阶段需要人工审核（安全考虑）。
//!
//! 参见 `docs/others/Harness Engineering Archi.md` §五

use serde::{Deserialize, Serialize};

use super::guardrails::GuardrailConfig;

/// 演化日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionEntry {
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 失败类型
    pub failure_type: FailurePattern,
    /// 来源 turn
    pub turn_number: u32,
    /// 相关工具
    pub tool_name: String,
    /// 错误信息
    pub error_message: String,
}

/// 失败模式 — 用于模式识别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailurePattern {
    /// 工具重复失败（同一工具连续失败）
    RepeatedToolFailure { tool_name: String, count: u32 },
    /// Turn 超限
    TurnLimitExceeded { max_turns: u32 },
    /// 上下文溢出
    ContextOverflow { tokens: u64, max_tokens: u64 },
    /// 权限拒绝
    PermissionDenied { reason: String },
    /// 测试反复失败
    TestLoopDetected { test_name: String, attempts: u32 },
    /// 未知
    Unknown { message: String },
}

/// Harness Mutation — 结构化配置修改
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HarnessMutation {
    /// 收紧 turn 限制
    TightenTurnLimit { from: u32, to: u32 },
    /// 增加工具到 blocklist
    BlockTool { tool_name: String, reason: String },
    /// 减少并发工具数
    ReduceConcurrency { from: u32, to: u32 },
    /// 延长工具超时
    IncreaseToolTimeout { from_seconds: u64, to_seconds: u64 },
    /// 启用某个禁用的护栏策略
    EnablePolicy { policy_name: String },
    /// 自定义
    Custom { description: String },
}

/// 演化引擎 — 模式识别与建议
pub struct EvolutionEngine {
    log: Vec<EvolutionEntry>,
    min_pattern_count: u32,
}

impl EvolutionEngine {
    pub fn new(min_pattern_count: u32) -> Self {
        Self {
            log: Vec::new(),
            min_pattern_count,
        }
    }

    /// 记录一次失败
    pub fn record_failure(
        &mut self,
        turn_number: u32,
        tool_name: impl Into<String>,
        error_message: impl Into<String>,
    ) {
        let tool_name: String = tool_name.into();
        let error_message: String = error_message.into();

        let failure_type = Self::classify_failure(&tool_name, &error_message, turn_number);

        self.log.push(EvolutionEntry {
            timestamp: chrono::Utc::now(),
            failure_type,
            turn_number,
            tool_name,
            error_message,
        });
    }

    /// 识别重复模式，生成 Mutation 建议
    pub fn analyze(&self) -> Vec<HarnessMutation> {
        let mut mutations = Vec::new();

        // 模式 1: 同一工具连续失败 ≥ min_pattern_count 次
        let repeated_tools = self.find_repeated_tool_failures();
        for (tool, count) in repeated_tools {
            if count >= self.min_pattern_count {
                mutations.push(HarnessMutation::BlockTool {
                    tool_name: tool.clone(),
                    reason: format!("failed {count} consecutive times"),
                });
            }
        }

        // 模式 2: Turn 超限
        if self
            .count_pattern(|e| matches!(e.failure_type, FailurePattern::TurnLimitExceeded { .. }))
            >= self.min_pattern_count
        {
            mutations.push(HarnessMutation::TightenTurnLimit { from: 50, to: 40 });
        }

        // 模式 3: 上下文频繁溢出
        if self.count_pattern(|e| matches!(e.failure_type, FailurePattern::ContextOverflow { .. }))
            >= self.min_pattern_count
        {
            // 建议减少并发（更多并发 = 更快填满上下文）
            mutations.push(HarnessMutation::ReduceConcurrency { from: 8, to: 4 });
        }

        // 模式 4: 测试循环（重复失败同一测试）
        let test_loops = self.find_test_loops();
        for (test_name, attempts) in test_loops {
            if attempts >= self.min_pattern_count {
                mutations.push(HarnessMutation::Custom {
                    description: format!(
                        "test '{test_name}' failed {attempts} times — consider adding to skip list"
                    ),
                });
            }
        }

        mutations
    }

    /// 应用 Mutation 到 GuardrailConfig
    pub fn apply_to_config(mutations: &[HarnessMutation], config: &mut GuardrailConfig) {
        for m in mutations {
            match m {
                HarnessMutation::TightenTurnLimit { to, .. } => {
                    config.decision.turn_limit = *to;
                }
                HarnessMutation::BlockTool {
                    tool_name,
                    reason: _,
                } => {
                    if !config.firewall.tool_whitelist.blocked.contains(tool_name) {
                        config
                            .firewall
                            .tool_whitelist
                            .blocked
                            .push(tool_name.clone());
                    }
                }
                HarnessMutation::ReduceConcurrency { to, .. } => {
                    config.decision.max_concurrent_tools = *to;
                }
                HarnessMutation::IncreaseToolTimeout { to_seconds, .. } => {
                    config.decision.tool_timeout_seconds = *to_seconds;
                }
                HarnessMutation::EnablePolicy { policy_name } => {
                    for policy in &mut config.adjudication.policies {
                        if &policy.name == policy_name {
                            policy.enabled = true;
                        }
                    }
                }
                HarnessMutation::Custom { .. } => {
                    // 自定义变更需人工审核，日志记录
                }
            }
        }
    }

    /// 日志条目数
    pub fn entry_count(&self) -> usize {
        self.log.len()
    }

    /// 清空日志
    pub fn clear(&mut self) {
        self.log.clear();
    }

    // ── 内部 ──

    fn classify_failure(tool_name: &str, error: &str, turn: u32) -> FailurePattern {
        let error_lower = error.to_lowercase();
        if error_lower.contains("context length") || error_lower.contains("too many tokens") {
            FailurePattern::ContextOverflow {
                tokens: 0,
                max_tokens: 0,
            }
        } else if error_lower.contains("permission")
            || error_lower.contains("denied")
            || error_lower.contains("blocked")
        {
            FailurePattern::PermissionDenied {
                reason: error.to_string(),
            }
        } else if error_lower.contains("test") && error_lower.contains("fail") {
            FailurePattern::TestLoopDetected {
                test_name: error.to_string(),
                attempts: 1,
            }
        } else if turn >= 50 {
            FailurePattern::TurnLimitExceeded { max_turns: 50 }
        } else {
            FailurePattern::RepeatedToolFailure {
                tool_name: tool_name.to_string(),
                count: 1,
            }
        }
    }

    fn find_repeated_tool_failures(&self) -> Vec<(String, u32)> {
        let mut results = Vec::new();
        let mut i = 0;
        while i < self.log.len() {
            if let FailurePattern::RepeatedToolFailure { tool_name, .. } = &self.log[i].failure_type
            {
                let name = tool_name.clone();
                let mut count = 1u32;
                let mut j = i + 1;
                while j < self.log.len() {
                    if self.log[j].tool_name == name {
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                results.push((name, count));
                i = j;
            } else {
                i += 1;
            }
        }
        results
    }

    fn find_test_loops(&self) -> Vec<(String, u32)> {
        let mut results = Vec::new();
        let mut test_counts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        for entry in &self.log {
            if let FailurePattern::TestLoopDetected { test_name, .. } = &entry.failure_type {
                *test_counts.entry(test_name.clone()).or_insert(0) += 1;
            }
        }
        for (name, count) in test_counts {
            if count >= self.min_pattern_count {
                results.push((name, count));
            }
        }
        results
    }

    fn count_pattern(&self, predicate: impl Fn(&EvolutionEntry) -> bool) -> u32 {
        self.log.iter().filter(|e| predicate(e)).count() as u32
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::super::guardrails::GuardrailConfig;
    use super::*;

    #[test]
    fn test_classify_context_overflow() {
        let pattern = EvolutionEngine::classify_failure("bash", "context length exceeded", 1);
        assert!(matches!(pattern, FailurePattern::ContextOverflow { .. }));
    }

    #[test]
    fn test_classify_permission_denied() {
        let pattern = EvolutionEngine::classify_failure("write", "permission denied: .env", 1);
        assert!(matches!(pattern, FailurePattern::PermissionDenied { .. }));
    }

    #[test]
    fn test_repeated_tool_detection() {
        let mut engine = EvolutionEngine::new(2);
        engine.record_failure(1, "bash", "command not found");
        engine.record_failure(2, "bash", "command not found");
        engine.record_failure(3, "bash", "command not found");
        let mutations = engine.analyze();
        assert!(
            !mutations.is_empty(),
            "should detect repeated bash failures"
        );
        let has_block = mutations.iter().any(
            |m| matches!(m, HarnessMutation::BlockTool { tool_name, .. } if tool_name == "bash"),
        );
        assert!(
            has_block,
            "should suggest blocking bash after 3 consecutive failures"
        );
    }

    #[test]
    fn test_no_mutation_below_threshold() {
        let mut engine = EvolutionEngine::new(3);
        engine.record_failure(1, "bash", "error");
        engine.record_failure(2, "bash", "error");
        // Only 2 failures, threshold is 3 → no mutation
        let mutations = engine.analyze();
        let has_block = mutations
            .iter()
            .any(|m| matches!(m, HarnessMutation::BlockTool { .. }));
        assert!(!has_block, "should not suggest mutation below threshold");
    }

    #[test]
    fn test_apply_tighten_turn_limit() {
        let mut config = GuardrailConfig::default();
        let mutations = vec![HarnessMutation::TightenTurnLimit { from: 50, to: 30 }];
        EvolutionEngine::apply_to_config(&mutations, &mut config);
        assert_eq!(config.decision.turn_limit, 30);
    }

    #[test]
    fn test_apply_block_tool() {
        let mut config = GuardrailConfig::default();
        let mutations = vec![HarnessMutation::BlockTool {
            tool_name: "rm".into(),
            reason: "dangerous".into(),
        }];
        EvolutionEngine::apply_to_config(&mutations, &mut config);
        assert!(
            config
                .firewall
                .tool_whitelist
                .blocked
                .contains(&"rm".to_string())
        );
    }

    #[test]
    fn test_context_overflow_mutations() {
        let mut engine = EvolutionEngine::new(2);
        engine.record_failure(1, "bash", "context length exceeded: 100000 tokens");
        engine.record_failure(2, "bash", "too many tokens in context");
        let mutations = engine.analyze();
        assert!(
            mutations
                .iter()
                .any(|m| matches!(m, HarnessMutation::ReduceConcurrency { .. })),
            "context overflow should trigger concurrency reduction"
        );
    }
}
