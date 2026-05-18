//! 结构化 diff 引擎 — 计算 + 渲染 + 可组合
//!
//! 基于 `similar` crate (Myers diff 算法)，提供：
//! - `Patch::compute()`: 从两段文本计算多 hunk diff
//! - `Patch::to_unified()`: 渲染为 unified diff 文本
//! - `Patch::stats()`: 变更统计（+N/-M 行）
//! - 结构化类型 (`Hunk`, `DiffLine`) 供 TUI/RPC 直接消费

use similar::{ChangeTag, TextDiff};

/// 单行 diff 标记
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Context {
        text: String,
        old_line: usize,
        new_line: usize,
    },
    Added {
        text: String,
        new_line: usize,
    },
    Removed {
        text: String,
        old_line: usize,
    },
}

impl DiffLine {
    pub fn text(&self) -> &str {
        match self {
            DiffLine::Context { text, .. }
            | DiffLine::Added { text, .. }
            | DiffLine::Removed { text, .. } => text,
        }
    }

    pub fn is_add(&self) -> bool {
        matches!(self, DiffLine::Added { .. })
    }

    pub fn is_remove(&self) -> bool {
        matches!(self, DiffLine::Removed { .. })
    }
}

/// 一个 hunk（连续变更区域 + 上下文）
#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

/// 单文件 diff 结果
#[derive(Debug, Clone)]
pub struct Patch {
    pub path: String,
    pub hunks: Vec<Hunk>,
    pub old_bytes: usize,
    pub new_bytes: usize,
}

/// 变更统计
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
    pub unchanged: usize,
}

impl Patch {
    /// 从两段文本计算 diff（Myers 算法，行级比较）
    pub fn compute(old: &str, new: &str, path: &str) -> Self {
        let diff = TextDiff::from_lines(old, new);
        let mut hunks = Vec::new();

        for hunk in diff.unified_diff().iter_hunks() {
            let header = hunk.header();
            let header_str = header.to_string();
            // Parse "@@ -old_start,old_count +new_start,new_count @@"
            let (old_start, old_count, new_start, new_count) = parse_hunk_header(&header_str);

            let mut lines = Vec::new();
            for change in hunk.iter_changes() {
                let old_idx = change.old_index().map(|i| i + 1).unwrap_or(0);
                let new_idx = change.new_index().map(|i| i + 1).unwrap_or(0);
                let text = change.to_string_lossy().to_string();
                let text = text.trim_end_matches('\n').to_string();

                lines.push(match change.tag() {
                    ChangeTag::Equal => DiffLine::Context {
                        text,
                        old_line: old_idx,
                        new_line: new_idx,
                    },
                    ChangeTag::Insert => DiffLine::Added {
                        text,
                        new_line: new_idx,
                    },
                    ChangeTag::Delete => DiffLine::Removed {
                        text,
                        old_line: old_idx,
                    },
                });
            }

            hunks.push(Hunk {
                old_start,
                old_count,
                new_start,
                new_count,
                lines,
            });
        }

        Patch {
            path: path.to_string(),
            hunks,
            old_bytes: old.len(),
            new_bytes: new.len(),
        }
    }

    /// 是否没有变更
    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }

    /// 统计信息
    pub fn stats(&self) -> DiffStats {
        let mut added = 0;
        let mut removed = 0;
        let mut unchanged = 0;
        for hunk in &self.hunks {
            for line in &hunk.lines {
                match line {
                    DiffLine::Added { .. } => added += 1,
                    DiffLine::Removed { .. } => removed += 1,
                    DiffLine::Context { .. } => unchanged += 1,
                }
            }
        }
        DiffStats {
            added,
            removed,
            unchanged,
        }
    }

    /// 渲染为 unified diff 文本
    pub fn to_unified(&self, max_lines: usize) -> String {
        if self.hunks.is_empty() {
            return format!("no changes: {}", self.path);
        }

        let mut out = String::new();

        // New file case (no old content)
        if self.old_bytes == 0 && self.new_bytes > 0 {
            out.push_str(&format!("--- /dev/null\n+++ {}\n", self.path));
        } else {
            out.push_str(&format!("--- {}\n+++ {}\n", self.path, self.path));
        }

        let mut total_lines = 0;
        for hunk in &self.hunks {
            if total_lines >= max_lines {
                break;
            }
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
            ));
            total_lines += 1;

            for line in &hunk.lines {
                if total_lines >= max_lines {
                    break;
                }
                match line {
                    DiffLine::Context { text, .. } => {
                        out.push_str(&format!(" {text}\n"));
                    }
                    DiffLine::Added { text, .. } => {
                        out.push_str(&format!("+{text}\n"));
                    }
                    DiffLine::Removed { text, .. } => {
                        out.push_str(&format!("-{text}\n"));
                    }
                }
                total_lines += 1;
            }
        }

        if total_lines >= max_lines {
            out.push_str("...\n");
        }

        out.push_str(&format!(
            "{} bytes written to {}",
            self.new_bytes, self.path
        ));
        out
    }
}

/// Parse "@@ -old_start,old_count +new_start,new_count @@" → (old_start, old_count, new_start, new_count)
fn parse_hunk_header(s: &str) -> (usize, usize, usize, usize) {
    // similar formats as "@@ -3,2 +3,3 @@" or "@@ -1 +1 @@" (count=1 omitted)
    let s = s
        .trim_start_matches('@')
        .trim_start_matches(' ')
        .trim_end_matches('@')
        .trim_end_matches(' ');
    let parts: Vec<&str> = s.split(' ').collect();
    let old_part = parts.first().unwrap_or(&"").trim_start_matches('-');
    let new_part = parts.get(1).unwrap_or(&"").trim_start_matches('+');

    let (old_start, old_count) = parse_range(old_part);
    let (new_start, new_count) = parse_range(new_part);
    (old_start, old_count, new_start, new_count)
}

fn parse_range(s: &str) -> (usize, usize) {
    match s.split_once(',') {
        Some((start, count)) => (start.parse().unwrap_or(1), count.parse().unwrap_or(0)),
        None => (s.parse().unwrap_or(1), 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_no_changes() {
        let patch = Patch::compute("same\n", "same\n", "test.txt");
        assert!(patch.is_empty());
        assert_eq!(patch.hunks.len(), 0);
        assert_eq!(patch.to_unified(50), "no changes: test.txt");
    }

    #[test]
    fn test_patch_single_change() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\n";
        let patch = Patch::compute(old, new, "f.txt");

        assert_eq!(patch.hunks.len(), 1);
        assert_eq!(patch.stats().removed, 1);
        assert_eq!(patch.stats().added, 1);
        assert_eq!(patch.stats().unchanged, 2); // line1 + line3

        let unified = patch.to_unified(50);
        assert!(unified.contains("-line2"));
        assert!(unified.contains("+modified"));
        assert!(unified.contains("@@"));
    }

    #[test]
    fn test_patch_multiple_hunks() {
        // Changes must be far enough apart that context radius (3 lines) doesn't merge them
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\nP\n";
        let patch = Patch::compute(old, new, "multi.txt");

        assert!(
            patch.hunks.len() >= 2,
            "expected >= 2 hunks for two separate changes, got {}",
            patch.hunks.len()
        );
        assert_eq!(patch.stats().removed, 2);
        assert_eq!(patch.stats().added, 2);

        let unified = patch.to_unified(50);
        assert!(unified.contains("-a"));
        assert!(unified.contains("+A"));
        assert!(unified.contains("-p"));
        assert!(unified.contains("+P"));
    }

    #[test]
    fn test_patch_new_file() {
        let patch = Patch::compute("", "hello\nworld\n", "new.txt");
        assert!(!patch.is_empty());
        let unified = patch.to_unified(50);
        assert!(unified.contains("--- /dev/null"));
        assert!(unified.contains("+hello"));
        assert!(unified.contains("+world"));
    }

    #[test]
    fn test_patch_to_unified_max_lines() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "A\nB\nC\nD\nE\n";
        let patch = Patch::compute(old, new, "f.txt");
        let limited = patch.to_unified(3);
        assert!(limited.contains("..."));
    }

    #[test]
    fn test_diff_line_accessors() {
        let ctx = DiffLine::Context {
            text: "hello".into(),
            old_line: 1,
            new_line: 1,
        };
        assert_eq!(ctx.text(), "hello");
        assert!(!ctx.is_add());
        assert!(!ctx.is_remove());

        let add = DiffLine::Added {
            text: "new".into(),
            new_line: 2,
        };
        assert!(add.is_add());
        assert!(!add.is_remove());

        let rem = DiffLine::Removed {
            text: "old".into(),
            old_line: 1,
        };
        assert!(rem.is_remove());
        assert!(!rem.is_add());
    }

    #[test]
    fn test_stats() {
        let old = "a\nb\nc\n";
        let new = "a\nx\nc\n";
        let patch = Patch::compute(old, new, "f.txt");
        let stats = patch.stats();
        assert_eq!(stats.added, 1);
        assert_eq!(stats.removed, 1);
    }

    #[test]
    fn test_patch_delete_lines() {
        let old = "a\nb\nc\n";
        let new = "a\nc\n";
        let patch = Patch::compute(old, new, "f.txt");
        let unified = patch.to_unified(50);
        assert!(unified.contains("-b"));
        assert!(!unified.contains("+b"));
    }

    #[test]
    fn test_patch_add_lines() {
        let old = "a\nc\n";
        let new = "a\nb\nc\n";
        let patch = Patch::compute(old, new, "f.txt");
        let unified = patch.to_unified(50);
        assert!(unified.contains("+b"));
        assert!(!unified.contains("-b"));
    }

    #[test]
    fn test_unified_output_has_bytes_written() {
        let patch = Patch::compute("old\n", "new\n", "f.txt");
        let unified = patch.to_unified(50);
        assert!(unified.contains("bytes written to f.txt"));
    }
}
