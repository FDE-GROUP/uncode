//! 情景记忆 — 按重要性评分的事件记忆层
//!
//! ## 认知显化与决策驱动设计中的定位
//!
//! 认知心理学中有三层记忆：
//! 1. 工作记忆 (Working Memory) — 当前 turn 的临时 scratchpad
//! 2. 情景记忆 (Episodic Memory) — 本次会话中按重要性保留的事件
//! 3. 语义记忆 (Semantic Memory) — 跨会话的知识提取（向量检索）
//!
//! `EpisodeMemory` 实现第二层：对会话内事件评分，按重要性选择保留。
//! 治理层（`SessionStore`）仍然记录一切——情景记忆是认知层的"选择性视图"。
//!
//! ## 重要性评分规则
//!
//! | 事件类型 | 基础分 | 加权规则 |
//! |:---|:---:|:---|
//! | 关键决策 (DecisionMade, Error, CompactionComplete) | 10 | 每次出现 +10 |
//! | 工具结果 (success) | 5 | 文件变更 +3 |
//! | 工具结果 (failure) | 8 | 失败比成功更重要（从错误中学习） |
//! | turn 边界 (TurnStart/TurnEnd) | 3 | 结构标记 |
//! | 常规内容 (ContentDelta) | 1 | 大量出现时累加 |
//! | 用户消息 | 6 | 用户意图最重要 |

/// 记忆条目的重要性分数
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImportanceScore(pub u32);

impl ImportanceScore {
    pub const CRITICAL: Self = Self(10);
    pub const HIGH: Self = Self(7);
    pub const MEDIUM: Self = Self(4);
    pub const LOW: Self = Self(1);
    pub const TRIVIAL: Self = Self(0);

    pub fn is_at_least(&self, threshold: Self) -> bool {
        self.0 >= threshold.0
    }
}

/// 情景记忆条目
#[derive(Debug, Clone)]
pub struct EpisodeEntry {
    /// 事件类型标签
    pub event_type: String,
    /// 事件摘要（一行描述）
    pub summary: String,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 重要性分数
    pub importance: ImportanceScore,
    /// 关联的 turn 编号
    pub turn_number: u64,
    /// 是否参与 LLM 上下文构建
    pub retained: bool,
}

/// 情景记忆 — 会话内按重要性保留的事件集合
pub struct EpisodeMemory {
    entries: Vec<EpisodeEntry>,
    capacity: usize,
    /// token 预算上限（估算值）；0 表示不限制，仅用 count-based 驱逐
    token_budget: usize,
}

impl EpisodeMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            capacity,
            token_budget: 0,
        }
    }

    /// 设置 token 预算上限；超过时基于重要性自适应驱逐
    pub fn with_token_budget(mut self, budget: usize) -> Self {
        self.token_budget = budget;
        self
    }

    /// 记录一个事件，自动评分
    pub fn record(
        &mut self,
        event_type: impl Into<String>,
        summary: impl Into<String>,
        turn_number: u64,
    ) {
        let event_type: String = event_type.into();
        let summary: String = summary.into();
        let importance = Self::score_event(&event_type, &summary);
        let entry = EpisodeEntry {
            event_type,
            summary,
            timestamp: chrono::Utc::now(),
            importance,
            turn_number,
            retained: importance.is_at_least(ImportanceScore::MEDIUM),
        };
        self.entries.push(entry);
        self.maybe_evict();
    }

    /// 记录带自定义重要性的事件
    pub fn record_with_importance(
        &mut self,
        event_type: impl Into<String>,
        summary: impl Into<String>,
        turn_number: u64,
        importance: ImportanceScore,
    ) {
        self.entries.push(EpisodeEntry {
            event_type: event_type.into(),
            summary: summary.into(),
            timestamp: chrono::Utc::now(),
            importance,
            turn_number,
            retained: importance.is_at_least(ImportanceScore::MEDIUM),
        });
        self.maybe_evict();
    }

    /// 按阈值查询保留的记忆
    pub fn query_by_importance(&self, threshold: ImportanceScore) -> Vec<&EpisodeEntry> {
        self.entries
            .iter()
            .filter(|e| e.importance.is_at_least(threshold))
            .collect()
    }

    /// 查询最近 N 条记忆
    pub fn query_recent(&self, n: usize) -> &[EpisodeEntry] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }

    /// 查询指定 turn 的记忆
    pub fn query_by_turn(&self, turn_number: u64) -> Vec<&EpisodeEntry> {
        self.entries
            .iter()
            .filter(|e| e.turn_number == turn_number)
            .collect()
    }

    /// 生成压缩候选：返回低于阈值的条目索引（按重要性从低到高）
    pub fn eviction_candidates(&self, threshold: ImportanceScore) -> Vec<usize> {
        let mut indices: Vec<(usize, ImportanceScore)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.importance.is_at_least(threshold))
            .map(|(i, e)| (i, e.importance))
            .collect();
        indices.sort_by_key(|(_, s)| *s);
        indices.into_iter().map(|(i, _)| i).collect()
    }

    /// 构建 LLM 上下文摘要（保留的事件拼接为文本）
    pub fn build_context_summary(&self) -> Option<String> {
        let retained: Vec<&EpisodeEntry> = self.entries.iter().filter(|e| e.retained).collect();
        if retained.is_empty() {
            return None;
        }
        let mut summary = String::from("## 重要事件摘要\n\n");
        for entry in &retained {
            let _ = std::fmt::Write::write_fmt(
                &mut summary,
                format_args!(
                    "- [t{}] {}: {}\n",
                    entry.turn_number, entry.event_type, entry.summary
                ),
            );
        }
        Some(summary)
    }

    /// 总条目数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // ── 内部 ──

    fn score_event(event_type: &str, summary: &str) -> ImportanceScore {
        let summary = summary.to_lowercase();
        let base = match event_type {
            "decision_made" | "error" | "compaction_complete" => ImportanceScore::CRITICAL,
            "tool_result_failure" | "agent_interrupted" => ImportanceScore::HIGH,
            "tool_result_success" => ImportanceScore::MEDIUM,
            "user_message" => ImportanceScore(6),
            "turn_start" | "turn_end" => ImportanceScore(3),
            "content_delta" => ImportanceScore::LOW,
            _ => ImportanceScore::LOW,
        };

        // 加权：包含错误关键词 +2
        let weighted = if summary.contains("error")
            || summary.contains("fail")
            || summary.contains("denied")
        {
            ImportanceScore((base.0 + 2).min(10))
        } else if summary.contains("test") {
            ImportanceScore((base.0 + 1).min(10))
        } else {
            base
        };

        weighted
    }

    fn maybe_evict(&mut self) {
        // Token-budget-based eviction: 当估算 token 超过预算时驱逐
        if self.token_budget > 0 {
            let estimated_tokens = self.estimate_tokens();
            if estimated_tokens <= self.token_budget {
                return;
            }
            // 收集低重要性条目，按重要性排序后批量移除
            let mut candidates: Vec<(usize, ImportanceScore)> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.importance.is_at_least(ImportanceScore::MEDIUM))
                .map(|(i, e)| (i, e.importance))
                .collect();
            candidates.sort_by_key(|(_, s)| *s);

            // 从后往前移除（保持 index 有效）
            let mut freed = 0usize;
            let mut to_remove: Vec<usize> = Vec::new();
            for (idx, _) in &candidates {
                if estimated_tokens - freed <= self.token_budget {
                    break;
                }
                freed += self.estimate_entry_tokens(&self.entries[*idx]);
                to_remove.push(*idx);
            }
            // 按逆序移除
            for idx in to_remove.into_iter().rev() {
                self.entries.remove(idx);
            }
            if self.estimate_tokens() > self.token_budget {
                // 仍然超预算，扩容 token_budget 50%
                self.token_budget = (self.token_budget * 3) / 2;
            }
            return;
        }
        // Fallback: count-based eviction (2x capacity)
        while self.entries.len() > self.capacity * 2 {
            let candidates = self.eviction_candidates(ImportanceScore::MEDIUM);
            if candidates.is_empty() {
                self.capacity = (self.capacity * 3) / 2;
                break;
            }
            self.entries.remove(candidates[0]);
        }
    }

    /// 估算总 token 数（4 字符 ≈ 1 token）
    fn estimate_tokens(&self) -> usize {
        self.entries
            .iter()
            .map(|e| self.estimate_entry_tokens(e))
            .sum()
    }

    fn estimate_entry_tokens(&self, entry: &EpisodeEntry) -> usize {
        let chars = entry.event_type.len() + entry.summary.len();
        chars / 4 + 1
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_importance_ordering() {
        assert!(ImportanceScore::CRITICAL > ImportanceScore::HIGH);
        assert!(ImportanceScore::HIGH > ImportanceScore::MEDIUM);
        assert!(ImportanceScore::MEDIUM > ImportanceScore::LOW);
    }

    #[test]
    fn test_critical_events_scored_high() {
        let mut mem = EpisodeMemory::new(100);
        mem.record("decision_made", "blocked rm -rf", 1);
        mem.record("error", "network timeout", 1);
        assert_eq!(mem.query_by_importance(ImportanceScore::CRITICAL).len(), 2);
    }

    #[test]
    fn test_failure_scored_higher_than_success() {
        let mut mem = EpisodeMemory::new(100);
        mem.record("tool_result_success", "read file ok", 1);
        mem.record("tool_result_failure", "test failed: assertion error", 1);
        let high = mem.query_by_importance(ImportanceScore::HIGH);
        assert_eq!(high.len(), 1);
        assert!(high[0].summary.contains("failed"));
    }

    #[test]
    fn test_eviction_removes_low_importance() {
        let mut mem = EpisodeMemory::new(5);
        // 填充 15 个低重要性条目（content_delta = LOW）
        for i in 0..15 {
            mem.record("content_delta", format!("chunk {i}"), 1);
        }
        // 驱逐将条目数限制在 capacity * 2 = 10
        // 所有条目都低于 MEDIUM，所以会被逐个逐出直到 ≤ 10
        assert!(
            mem.len() <= 10,
            "expected <= 10 after eviction, got {}",
            mem.len()
        );
        // 再插入一个 CRITICAL 条目——它应被保留
        mem.record("decision_made", "important decision", 1);
        // CRITICAL 条目不应被驱逐
        let critical = mem.query_by_importance(ImportanceScore::CRITICAL);
        assert!(
            !critical.is_empty(),
            "critical entry should survive eviction"
        );
    }

    #[test]
    fn test_build_summary_only_includes_retained() {
        let mut mem = EpisodeMemory::new(100);
        mem.record("decision_made", "approved read command", 1);
        mem.record("content_delta", "some text", 1); // 低于 MEDIUM，不保留
        mem.record("error", "compilation failed", 1);

        let summary = mem.build_context_summary().unwrap();
        assert!(summary.contains("decision_made"));
        assert!(summary.contains("error"));
        assert!(!summary.contains("content_delta"));
    }

    #[test]
    fn test_error_keyword_boosts_score() {
        let mut mem = EpisodeMemory::new(100);
        // tool_result_success 基础分是 MEDIUM(4)
        // 但包含 "error" → +2 = 6
        mem.record(
            "tool_result_success",
            "command failed with error: ENOENT",
            1,
        );
        let high = mem.query_by_importance(ImportanceScore(6));
        assert_eq!(high.len(), 1);
    }

    #[test]
    fn test_token_budget_eviction() {
        let mut mem = EpisodeMemory::new(100).with_token_budget(50); // 50 tokens budget

        // Insert many low-importance entries (~100 chars each ≈ 25 tokens each)
        for i in 0..10 {
            mem.record(
                "content_delta",
                format!("chunk_{i}_padding_text_to_add_length_here"),
                1,
            );
        }

        // Should have evicted down to stay within budget
        let tokens = mem.estimate_tokens();
        assert!(
            tokens <= 75, // allow some overshoot margin
            "expected <= ~75 tokens after eviction, got {tokens}"
        );
    }

    #[test]
    fn test_token_budget_preserves_critical() {
        let mut mem = EpisodeMemory::new(100).with_token_budget(20);

        // Fill with low-importance
        for i in 0..5 {
            mem.record("content_delta", format!("padding_{i}_text"), 1);
        }
        // Add critical
        mem.record("decision_made", "critical decision", 1);

        let critical = mem.query_by_importance(ImportanceScore::CRITICAL);
        assert!(
            !critical.is_empty(),
            "critical entry should survive token-budget eviction"
        );
    }
}
