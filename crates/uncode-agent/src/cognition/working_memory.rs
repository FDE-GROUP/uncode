//! 工作记忆 — turn 内临时 scratchpad
//!
//! ## 定位
//!
//! 工作记忆是认知心理学三层记忆的第一层（最短期）。
//! 存储当前 turn 内的中间推理、观察和决策结果——
//! 这些信息在 turn 结束时被冲刷到 EpisodeMemory，
//! 但不直接出现在 LLM 的对话历史中。
//!
//! ## 与对话历史的区别
//!
//! ```text
//! 对话历史 (messages)     工作记忆 (scratchpad)
//! ─────────────────       ─────────────────
//! LLM 看到并参与           仅 Agent 内部使用
//! 跨 turn 保留             每个 turn 清空
//! 不可变追加               可读写覆盖
//! 影响 token 消耗          不占 LLM 上下文
//! ```

/// 工作记忆条目
#[derive(Debug, Clone)]
pub enum ScratchEntry {
    /// 观察记录："文件 X 被修改了"
    Observation { content: String, importance: u8 },
    /// 决策记录："选择了方案 B"
    Decision { content: String, outcome: String },
    /// 待办事项："还需要检查 Y"
    PendingTask { description: String },
    /// 假设/疑问："可能是缓存问题"
    Hypothesis { content: String, confidence: f32 },
    /// 原始备注
    Note(String),
}

/// 工作记忆 — 当前 turn 的临时 scratchpad
///
/// 不与 LLM 共享。turn 结束时内容被评估，
/// 高重要性条目转移到 EpisodeMemory。
pub struct WorkingMemory {
    entries: Vec<ScratchEntry>,
    turn_number: u64,
}

impl WorkingMemory {
    pub fn new(turn_number: u64) -> Self {
        Self {
            entries: Vec::with_capacity(32),
            turn_number,
        }
    }

    /// 记录一次观察
    pub fn observe(&mut self, content: impl Into<String>) {
        self.entries.push(ScratchEntry::Observation {
            content: content.into(),
            importance: 5,
        });
    }

    /// 记录一次重要观察
    pub fn observe_important(&mut self, content: impl Into<String>) {
        self.entries.push(ScratchEntry::Observation {
            content: content.into(),
            importance: 8,
        });
    }

    /// 记录一次决策
    pub fn decide(&mut self, content: impl Into<String>, outcome: impl Into<String>) {
        self.entries.push(ScratchEntry::Decision {
            content: content.into(),
            outcome: outcome.into(),
        });
    }

    /// 添加待办
    pub fn todo(&mut self, description: impl Into<String>) {
        self.entries.push(ScratchEntry::PendingTask {
            description: description.into(),
        });
    }

    /// 提出假设
    pub fn hypothesize(&mut self, content: impl Into<String>, confidence: f32) {
        self.entries.push(ScratchEntry::Hypothesis {
            content: content.into(),
            confidence: confidence.clamp(0.0, 1.0),
        });
    }

    /// 自由备注
    pub fn note(&mut self, text: impl Into<String>) {
        self.entries.push(ScratchEntry::Note(text.into()));
    }

    /// 清空并重置 turn（turn 结束时调用）
    pub fn flush(&mut self, next_turn: u64) -> Vec<ScratchEntry> {
        self.turn_number = next_turn;
        std::mem::take(&mut self.entries)
    }

    /// 获取当前所有条目
    pub fn entries(&self) -> &[ScratchEntry] {
        &self.entries
    }

    /// 筛选高重要性条目（importance >= threshold）
    pub fn extract_important(&self, threshold: u8) -> Vec<&ScratchEntry> {
        self.entries
            .iter()
            .filter(|e| e.importance() >= threshold)
            .collect()
    }

    /// 生成给下一 turn 的简要摘要（喂给 LLM 的 system 消息）
    pub fn to_context_hint(&self) -> Option<String> {
        let important: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.importance() >= 6)
            .map(|e| e.one_liner())
            .collect();

        if important.is_empty() {
            return None;
        }

        let mut hint = String::from("## 当前 turn 关键发现\n\n");
        for line in &important {
            hint.push_str("- ");
            hint.push_str(line);
            hint.push('\n');
        }
        Some(hint)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl ScratchEntry {
    /// 条目重要性（0-10）
    pub fn importance(&self) -> u8 {
        match self {
            Self::Observation { importance, .. } => *importance,
            Self::Decision { .. } => 7,
            Self::PendingTask { .. } => 4,
            Self::Hypothesis { confidence, .. } => (*confidence * 5.0) as u8,
            Self::Note(_) => 2,
        }
    }

    /// 单行摘要
    pub fn one_liner(&self) -> String {
        match self {
            Self::Observation { content, .. } => format!("[观察] {content}"),
            Self::Decision { content, outcome } => format!("[决策] {content} → {outcome}"),
            Self::PendingTask { description } => format!("[待办] {description}"),
            Self::Hypothesis {
                content,
                confidence,
            } => {
                format!("[假设📈{:.0}%] {content}", confidence * 100.0)
            }
            Self::Note(text) => format!("[备注] {text}"),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory_basic_operations() {
        let mut wm = WorkingMemory::new(1);
        wm.observe("file modified: src/main.rs");
        wm.decide("use edit tool", "success");
        wm.todo("check test passes");
        assert_eq!(wm.len(), 3);
    }

    #[test]
    fn test_flush_clears_and_returns() {
        let mut wm = WorkingMemory::new(1);
        wm.observe("test");
        wm.observe_important("critical finding");

        let flushed = wm.flush(2);
        assert_eq!(flushed.len(), 2);
        assert!(wm.is_empty());
    }

    #[test]
    fn test_context_hint_only_important() {
        let mut wm = WorkingMemory::new(1);
        wm.note("random thought"); // importance 2
        wm.observe_important("security vulnerability detected"); // importance 8
        wm.decide("rollback", "success"); // importance 7

        let hint = wm.to_context_hint().unwrap();
        assert!(hint.contains("security"));
        assert!(hint.contains("rollback"));
        assert!(!hint.contains("random"));
    }
}
