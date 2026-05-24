//! Per-tool typed hook contexts for ToolCallBefore/ToolCallAfter.
//!
//! Extensions can downcast the generic `serde_json::Value` arguments/results
//! into typed structs based on tool name for safer access.
//!
//! **Pi:** 对照 Pi 的 `BashToolInput` / `ReadToolInput` / `EditToolInput` 等 TypeScript 类型。

use serde::{Deserialize, Serialize};

// ── Bash ──

/// Typed input for the `bash` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashInput {
    pub command: String,
}

/// Typed result for the `bash` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashResult {
    pub output: String,
    pub exit_code: i32,
}

// ── Read ──

/// Typed input for the `read` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadInput {
    pub path: String,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
}

/// Typed result for the `read` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    pub content: String,
}

// ── Write ──

/// Typed input for the `write` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteInput {
    pub path: String,
    pub content: String,
}

/// Typed result for the `write` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub bytes_written: usize,
}

// ── Edit ──

/// Typed input for the `edit` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditInput {
    pub path: String,
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: Option<bool>,
}

/// Typed result for the `edit` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub replacements: usize,
}

// ── Grep ──

/// Typed input for the `grep` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepInput {
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub include: Option<String>,
}

/// Typed result for the `grep` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepResult {
    pub matches: String,
}

// ── Find ──

/// Typed input for the `find` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindInput {
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
}

/// Typed result for the `find` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindResult {
    pub entries: String,
}

// ── Ls ──

/// Typed input for the `ls` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsInput {
    pub path: String,
}

/// Typed result for the `ls` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsResult {
    pub entries: String,
}

// ── WebFetch ──

/// Typed input for the `web_fetch` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchInput {
    pub url: String,
}

/// Typed result for the `web_fetch` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchResult {
    pub content: String,
}

// ── WebSearch ──

/// Typed input for the `web_search` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchInput {
    pub query: String,
}

/// Typed result for the `web_search` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    pub results: String,
}

/// Try to deserialize tool arguments into a typed input struct.
///
/// Returns `None` if deserialization fails (e.g., wrong tool or malformed args).
pub fn try_parse_input<T: serde::de::DeserializeOwned>(args: &serde_json::Value) -> Option<T> {
    serde_json::from_value(args.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bash_input() {
        let args = serde_json::json!({"command": "cargo test"});
        let input: BashInput = try_parse_input(&args).unwrap();
        assert_eq!(input.command, "cargo test");
    }

    #[test]
    fn parse_read_input() {
        let args = serde_json::json!({"path": "/tmp/test.rs", "offset": 10, "limit": 50});
        let input: ReadInput = try_parse_input(&args).unwrap();
        assert_eq!(input.path, "/tmp/test.rs");
        assert_eq!(input.offset, Some(10));
        assert_eq!(input.limit, Some(50));
    }

    #[test]
    fn parse_edit_input() {
        let args = serde_json::json!({
            "path": "main.rs",
            "old_string": "foo",
            "new_string": "bar"
        });
        let input: EditInput = try_parse_input(&args).unwrap();
        assert_eq!(input.old_string, "foo");
        assert_eq!(input.new_string, "bar");
        assert_eq!(input.replace_all, None);
    }

    #[test]
    fn parse_wrong_type_returns_none() {
        let args = serde_json::json!({"path": "/tmp/test.rs"});
        let result: Option<BashInput> = try_parse_input(&args);
        assert!(result.is_none());
    }

    #[test]
    fn parse_write_input() {
        let args = serde_json::json!({"path": "out.rs", "content": "fn main() {}"});
        let input: WriteInput = try_parse_input(&args).unwrap();
        assert_eq!(input.path, "out.rs");
        assert_eq!(input.content, "fn main() {}");
    }

    #[test]
    fn parse_grep_input() {
        let args = serde_json::json!({"pattern": "TODO", "path": "src/", "include": "*.rs"});
        let input: GrepInput = try_parse_input(&args).unwrap();
        assert_eq!(input.pattern, "TODO");
        assert_eq!(input.path.as_deref(), Some("src/"));
        assert_eq!(input.include.as_deref(), Some("*.rs"));
    }

    #[test]
    fn parse_grep_input_minimal() {
        let args = serde_json::json!({"pattern": "fn main"});
        let input: GrepInput = try_parse_input(&args).unwrap();
        assert_eq!(input.pattern, "fn main");
        assert!(input.path.is_none());
        assert!(input.include.is_none());
    }

    #[test]
    fn parse_find_input() {
        let args = serde_json::json!({"pattern": "*.rs", "path": "crates/"});
        let input: FindInput = try_parse_input(&args).unwrap();
        assert_eq!(input.pattern, "*.rs");
        assert_eq!(input.path.as_deref(), Some("crates/"));
    }

    #[test]
    fn parse_ls_input() {
        let args = serde_json::json!({"path": "/home/user/project"});
        let input: LsInput = try_parse_input(&args).unwrap();
        assert_eq!(input.path, "/home/user/project");
    }

    #[test]
    fn parse_web_fetch_input() {
        let args = serde_json::json!({"url": "https://example.com"});
        let input: WebFetchInput = try_parse_input(&args).unwrap();
        assert_eq!(input.url, "https://example.com");
    }

    #[test]
    fn parse_web_search_input() {
        let args = serde_json::json!({"query": "rust async"});
        let input: WebSearchInput = try_parse_input(&args).unwrap();
        assert_eq!(input.query, "rust async");
    }

    #[test]
    fn parse_bash_result() {
        let val = serde_json::json!({"output": "hello", "exit_code": 0});
        let result: BashResult = try_parse_input(&val).unwrap();
        assert_eq!(result.output, "hello");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn parse_read_result() {
        let val = serde_json::json!({"content": "file contents"});
        let result: ReadResult = try_parse_input(&val).unwrap();
        assert_eq!(result.content, "file contents");
    }

    #[test]
    fn parse_edit_input_with_replace_all() {
        let args = serde_json::json!({
            "path": "main.rs",
            "old_string": "old",
            "new_string": "new",
            "replace_all": true
        });
        let input: EditInput = try_parse_input(&args).unwrap();
        assert_eq!(input.replace_all, Some(true));
    }
}
