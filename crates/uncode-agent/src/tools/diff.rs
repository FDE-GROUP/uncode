//! Diff 引擎 — 委托 uncode_core::diff::Patch，提供工具层便利函数

pub use uncode_core::diff::{DiffLine, DiffStats, Hunk, Patch};

pub const MAX_DIFF_LINES: usize = 50;

/// Generate a unified diff string between old and new content.
pub fn unified_diff(old: &str, new: &str, path: &str) -> String {
    let patch = Patch::compute(old, new, path);
    patch.to_unified(MAX_DIFF_LINES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_new_file() {
        let diff = unified_diff("", "hello\nworld\n", "test.txt");
        assert!(diff.starts_with("--- /dev/null\n+++ test.txt"));
        assert!(diff.contains("+hello"));
        assert!(diff.contains("+world"));
        assert!(diff.contains("bytes written"));
    }

    #[test]
    fn test_diff_no_changes() {
        let diff = unified_diff("same\n", "same\n", "test.txt");
        assert_eq!(diff, "no changes: test.txt");
    }

    #[test]
    fn test_diff_modify_middle() {
        let old = "line1\nline2\nline3\nline4\n";
        let new = "line1\nline2\nmodified\nline4\n";
        let diff = unified_diff(old, new, "test.txt");
        assert!(diff.contains("-line3"));
        assert!(diff.contains("+modified"));
        assert!(diff.contains("@@"));
    }

    #[test]
    fn test_diff_delete_lines() {
        let old = "a\nb\nc\n";
        let new = "a\nc\n";
        let diff = unified_diff(old, new, "f.txt");
        assert!(diff.contains("-b"));
        assert!(!diff.contains("+b"));
    }

    #[test]
    fn test_diff_add_lines() {
        let old = "a\nc\n";
        let new = "a\nb\nc\n";
        let diff = unified_diff(old, new, "f.txt");
        assert!(diff.contains("+b"));
        assert!(!diff.contains("-b"));
    }

    #[test]
    fn test_patch_multiple_hunks() {
        // Changes must be far enough apart that context radius (3 lines) doesn't merge them
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\nP\n";
        let patch = Patch::compute(old, new, "multi.txt");
        assert!(
            patch.hunks.len() >= 2,
            "expected >= 2 hunks, got {}",
            patch.hunks.len()
        );
    }
}
