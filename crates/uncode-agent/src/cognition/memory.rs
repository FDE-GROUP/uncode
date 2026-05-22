//! 认知记忆管理 — 压缩边界 + 摘要注入 + 上下文窗口策略
//!
//! ## 认知与决策驱动设计中的定位
//!
//! 记忆管理属于认知层的"记忆与检索建模"职责。
//! 它决定 Agent 的认知上下文中保留什么、压缩什么、遗忘什么。
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 认知层

/// 上下文窗口策略
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// 触发压缩的 token 使用率阈值（百分比，默认 80）
    pub compaction_threshold_percent: u8,
    /// 压缩后保留的最近 token 数
    pub keep_recent_tokens: u64,
    /// 为 LLM 响应预留的 token 数
    pub reserve_tokens: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            compaction_threshold_percent: 80,
            keep_recent_tokens: 4096,
            reserve_tokens: 8192,
        }
    }
}

/// 压缩决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionDecision {
    /// 不需要压缩
    Noop,
    /// 应该触发压缩
    ShouldCompact {
        current_tokens: u64,
        threshold: u64,
        reason: String,
    },
    /// 强制压缩（上下文溢出紧急恢复）
    ForceCompact {
        current_tokens: u64,
        max_tokens: u64,
    },
}

/// 记忆压缩管理器
///
/// 负责：
/// - 监控上下文 token 使用量
/// - 决定何时触发压缩
/// - 管理摘要注入策略
/// - 桥接 EpisodeMemory（情景记忆按重要性过滤）
pub struct MemoryManager {
    config: MemoryConfig,
}

impl MemoryManager {
    pub fn new(config: MemoryConfig) -> Self {
        Self { config }
    }

    /// 评估是否需要压缩（基于 token 阈值）
    pub fn evaluate(&self, current_tokens: u64, max_context: u64) -> CompactionDecision {
        let usage_percent = if max_context > 0 {
            (current_tokens * 100) / max_context
        } else {
            0
        };

        // 紧急溢出
        if current_tokens + self.config.reserve_tokens > max_context {
            return CompactionDecision::ForceCompact {
                current_tokens,
                max_tokens: max_context,
            };
        }

        // 超过阈值
        if usage_percent >= self.config.compaction_threshold_percent as u64 {
            let threshold = (max_context * self.config.compaction_threshold_percent as u64) / 100;
            return CompactionDecision::ShouldCompact {
                current_tokens,
                threshold,
                reason: format!(
                    "context usage {usage_percent}% >= threshold {}%",
                    self.config.compaction_threshold_percent
                ),
            };
        }

        CompactionDecision::Noop
    }

    /// 压缩后应保留的最近消息数估计
    pub fn keep_recent_count(&self, avg_message_tokens: u64) -> usize {
        if avg_message_tokens == 0 {
            return 10; // fallback
        }
        (self.config.keep_recent_tokens / avg_message_tokens) as usize
    }

    /// 基于情景记忆的智能压缩建议
    ///
    /// 与 `evaluate` 不同，此方法使用 EpisodeMemory 的重要性评分
    /// 来决定哪些事件应保留在 LLM 上下文中。
    ///
    /// 返回 `Some(threshold)` 表示应保留重要性 >= threshold 的条目；
    /// `None` 表示无需调整。
    pub fn smart_retention_threshold(
        &self,
        current_tokens: u64,
        max_context: u64,
        episode_importance_ratio: f64, // 0.0-1.0: 高重要性条目占总 token 的比例
    ) -> Option<u32> {
        let usage = if max_context > 0 {
            (current_tokens * 100) / max_context
        } else {
            0
        };

        // 仅在超过阈值时建议
        if usage < self.config.compaction_threshold_percent as u64 {
            return None;
        }

        // 高重要性条目占比 > 50% → 提高保留阈值（更激进驱逐）
        if episode_importance_ratio > 0.5 {
            return Some(7); // 只保留 HIGH 及以上
        }
        // 中等 → 标准阈值
        if episode_importance_ratio > 0.3 {
            return Some(4); // 保留 MEDIUM 及以上
        }
        // 低重要性 → 宽松保留
        Some(1) // 几乎全部保留
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_when_below_threshold() {
        let mgr = MemoryManager::new(MemoryConfig::default());
        let decision = mgr.evaluate(10_000, 100_000);
        assert_eq!(decision, CompactionDecision::Noop);
    }

    #[test]
    fn test_should_compact_at_threshold() {
        let mut config = MemoryConfig::default();
        config.compaction_threshold_percent = 80;
        let mgr = MemoryManager::new(config);
        let decision = mgr.evaluate(80_000, 100_000);
        assert!(matches!(decision, CompactionDecision::ShouldCompact { .. }));
    }

    #[test]
    fn test_force_compact_on_overflow() {
        let mgr = MemoryManager::new(MemoryConfig::default());
        // 当前 95000 + 预留 8192 > 100000 → 溢出
        let decision = mgr.evaluate(95_000, 100_000);
        assert!(matches!(decision, CompactionDecision::ForceCompact { .. }));
    }

    #[test]
    fn test_keep_recent_count() {
        let mgr = MemoryManager::new(MemoryConfig::default());
        // 4096 / 256 = 16 messages
        let count = mgr.keep_recent_count(256);
        assert_eq!(count, 16);
    }
}
