use std::fs;
use std::sync::Mutex;
use uncode_core::tool::ToolExecutor;

use super::bash::BashTool;
use super::edit::EditTool;
use super::find::FindTool;
use super::ls::LsTool;
use super::read::ReadTool;
use super::registry::ToolRegistry;
use super::write::WriteTool;

/// Global mutex to serialize tests that change the process cwd.
/// Without this, parallel tokio tests overwrite each other's cwd.
static CWD_MUTEX: Mutex<()> = Mutex::new(());

fn temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("uncode-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Create temp dir and set it as cwd so sandbox checks pass.
/// Returns the temp dir and a guard that serializes cwd changes.
/// Tests must use relative paths to stay within sandbox.
fn sandbox_dir() -> (std::path::PathBuf, std::sync::MutexGuard<'static, ()>) {
    let guard = CWD_MUTEX.lock().unwrap();
    let dir = temp_dir();
    std::env::set_current_dir(&dir).unwrap();
    (dir, guard)
}

#[tokio::test]
async fn test_read_tool() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("test.txt", "line 1\nline 2\nline 3\nline 4\nline 5\n").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .execute(serde_json::json!({"path": "test.txt"}))
        .await
        .unwrap();
    assert!(result.contains("line 1"));
    assert!(result.contains("line 5"));

    let result = tool
        .execute(serde_json::json!({"path": "test.txt", "offset": 2, "limit": 2}))
        .await
        .unwrap();
    assert!(result.contains("line 3"));
    assert!(!result.contains("line 1"));
}

#[tokio::test]
async fn test_read_tool_missing_path() {
    let (_dir, _guard) = sandbox_dir();
    let tool = ReadTool::new();
    let result = tool
        .execute(serde_json::json!({"path": "nonexistent_file.txt"}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_read_tool_no_path_arg() {
    let tool = ReadTool::new();
    let result = tool.execute(serde_json::json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_write_tool() {
    let (_dir, _guard) = sandbox_dir();

    let tool = WriteTool;
    let result = tool
        .execute(serde_json::json!({"path": "output.txt", "content": "hello world"}))
        .await
        .unwrap();

    assert!(result.contains("bytes written"));
    assert_eq!(fs::read_to_string("output.txt").unwrap(), "hello world");
}

#[tokio::test]
async fn test_write_tool_creates_parent_dirs() {
    let (_dir, _guard) = sandbox_dir();

    let tool = WriteTool;
    tool.execute(serde_json::json!({
        "path": "sub/dir/deep/output.txt",
        "content": "deep write"
    }))
    .await
    .unwrap();

    assert_eq!(
        fs::read_to_string("sub/dir/deep/output.txt").unwrap(),
        "deep write"
    );
}

#[tokio::test]
async fn test_write_tool_no_content_arg() {
    let tool = WriteTool;
    let result = tool.execute(serde_json::json!({"path": "test.txt"})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_edit_tool() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("edit_test.txt", "hello world\nfoo bar\n").unwrap();

    let tool = EditTool;
    tool.execute(serde_json::json!({
        "path": "edit_test.txt",
        "old_string": "hello world",
        "new_string": "hi there"
    }))
    .await
    .unwrap();

    let content = fs::read_to_string("edit_test.txt").unwrap();
    assert!(content.contains("hi there"));
    assert!(!content.contains("hello world"));
}

#[tokio::test]
async fn test_edit_tool_not_found() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("notfound.txt", "content").unwrap();

    let tool = EditTool;
    let result = tool
        .execute(serde_json::json!({
            "path": "notfound.txt",
            "old_string": "does not exist",
            "new_string": "replacement"
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_edit_tool_ambiguous_match() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("ambiguous.txt", "foo bar foo baz foo").unwrap();

    let tool = EditTool;
    let result = tool
        .execute(serde_json::json!({
            "path": "ambiguous.txt",
            "old_string": "foo",
            "new_string": "replaced"
        }))
        .await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("3 times"),
        "expected '3 times' in error, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_ls_tool() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("file1.txt", "").unwrap();
    fs::write("file2.rs", "").unwrap();
    fs::create_dir("subdir").unwrap();

    let tool = LsTool;
    let result = tool
        .execute(serde_json::json!({"path": "."}))
        .await
        .unwrap();

    assert!(result.contains("file1.txt"));
    assert!(result.contains("file2.rs"));
    assert!(result.contains("subdir/"));
}

#[tokio::test]
async fn test_ls_tool_empty_dir() {
    let (_dir, _guard) = sandbox_dir();
    fs::create_dir_all("empty").unwrap();

    let tool = LsTool;
    let result = tool
        .execute(serde_json::json!({"path": "empty"}))
        .await
        .unwrap();

    assert_eq!(result, "(empty)");
}

#[tokio::test]
async fn test_ls_tool_nonexistent_dir() {
    let tool = LsTool;
    let result = tool
        .execute(serde_json::json!({"path": "nonexistent_dir_xyz"}))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_find_tool() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("main.rs", "fn main() {}").unwrap();
    fs::write("lib.rs", "pub fn lib() {}").unwrap();
    fs::write("readme.md", "# test").unwrap();

    let tool = FindTool;
    let result = tool
        .execute(serde_json::json!({
            "pattern": "*.rs",
            "path": "."
        }))
        .await
        .unwrap();

    assert!(result.contains("main.rs"));
    assert!(result.contains("lib.rs"));
    assert!(!result.contains("readme.md"));
}

#[tokio::test]
async fn test_find_tool_no_matches() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("test.txt", "content").unwrap();

    let tool = FindTool;
    let result = tool
        .execute(serde_json::json!({
            "pattern": "*.xyz",
            "path": "."
        }))
        .await
        .unwrap();

    assert_eq!(result, "no files found");
}

#[tokio::test]
async fn test_registry_register_and_get() {
    let registry = ToolRegistry::new();
    registry.register("read".to_string(), std::sync::Arc::new(ReadTool::new()));

    let defs = registry.definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "read");

    assert!(registry.get("read").is_some());
    assert!(registry.get("nonexistent").is_none());
}

#[tokio::test]
async fn test_registry_multiple_tools() {
    let registry = ToolRegistry::new();
    registry.register("read".to_string(), std::sync::Arc::new(ReadTool::new()));
    registry.register("write".to_string(), std::sync::Arc::new(WriteTool));
    registry.register("edit".to_string(), std::sync::Arc::new(EditTool));

    assert_eq!(registry.definitions().len(), 3);
    let names = registry.list();
    assert_eq!(names.len(), 3);
}

#[tokio::test]
async fn test_registry_overwrite() {
    let registry = ToolRegistry::new();
    registry.register("read".to_string(), std::sync::Arc::new(ReadTool::new()));
    registry.register("read".to_string(), std::sync::Arc::new(ReadTool::new()));

    assert_eq!(registry.definitions().len(), 1);
}

// ── Hashline tests ──

fn extract_hash_from_hashline(hashline_output: &str, line_idx: usize) -> String {
    let line = hashline_output.lines().nth(line_idx).unwrap();
    // Format: "     5#KJ content"
    let after_hash = line.split('#').nth(1).unwrap();
    let hash = after_hash.split(' ').next().unwrap();
    hash.to_string()
}

#[tokio::test]
async fn test_read_tool_hashline_mode() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("hash_test.txt", "line one\nline two\nline three\n").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .execute(serde_json::json!({"path": "hash_test.txt", "hashline": true}))
        .await
        .unwrap();

    assert!(result.contains("#"), "hashline output should contain #");
    assert!(result.contains("line one"));
    let first_line = result.lines().next().unwrap();
    assert!(first_line.contains("#"));
    // Verify format: "     1#XX line one"
    let parts: Vec<&str> = first_line.splitn(2, '#').collect();
    assert_eq!(parts.len(), 2);
    let after_hash: Vec<&str> = parts[1].splitn(2, ' ').collect();
    assert_eq!(after_hash[0].len(), 2, "hash should be 2 chars");
}

#[tokio::test]
async fn test_read_tool_hashline_deterministic() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("det.txt", "hello world\n").unwrap();

    let tool = ReadTool::new();
    let r1 = tool
        .execute(serde_json::json!({"path": "det.txt", "hashline": true}))
        .await
        .unwrap();
    let r2 = tool
        .execute(serde_json::json!({"path": "det.txt", "hashline": true}))
        .await
        .unwrap();
    assert_eq!(r1, r2, "hashline output should be deterministic");
}

#[tokio::test]
async fn test_edit_tool_hashline_replace() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("hl_edit.txt", "alpha\nbeta\ngamma\ndelta\n").unwrap();

    let read_tool = ReadTool::new();
    let read_result = read_tool
        .execute(serde_json::json!({"path": "hl_edit.txt", "hashline": true}))
        .await
        .unwrap();

    let hash2 = extract_hash_from_hashline(&read_result, 1);

    let edit_tool = EditTool;
    let result = edit_tool
        .execute(serde_json::json!({
            "path": "hl_edit.txt",
            "edits": [{"op": "replace", "pos": format!("2#{hash2}"), "lines": "BETA"}]
        }))
        .await
        .unwrap();

    assert!(result.contains("-beta") || result.contains("BETA"));
    let content = fs::read_to_string("hl_edit.txt").unwrap();
    assert!(content.contains("BETA"));
    assert!(!content.contains("beta"));
}

#[tokio::test]
async fn test_edit_tool_hashline_range_replace() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("range.txt", "a\nb\nc\nd\ne\n").unwrap();

    let read_tool = ReadTool::new();
    let read_result = read_tool
        .execute(serde_json::json!({"path": "range.txt", "hashline": true}))
        .await
        .unwrap();

    let hash2 = extract_hash_from_hashline(&read_result, 1);
    let hash4 = extract_hash_from_hashline(&read_result, 3);

    let edit_tool = EditTool;
    edit_tool
        .execute(serde_json::json!({
            "path": "range.txt",
            "edits": [{
                "op": "replace",
                "pos": format!("2#{hash2}"),
                "end": format!("4#{hash4}"),
                "lines": "X\nY"
            }]
        }))
        .await
        .unwrap();

    let content = fs::read_to_string("range.txt").unwrap();
    assert_eq!(content, "a\nX\nY\ne\n");
}

#[tokio::test]
async fn test_edit_tool_hashline_prepend() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("prepend.txt", "first\nsecond\n").unwrap();

    let read_tool = ReadTool::new();
    let read_result = read_tool
        .execute(serde_json::json!({"path": "prepend.txt", "hashline": true}))
        .await
        .unwrap();

    let hash1 = extract_hash_from_hashline(&read_result, 0);

    let edit_tool = EditTool;
    edit_tool
        .execute(serde_json::json!({
            "path": "prepend.txt",
            "edits": [{"op": "prepend", "pos": format!("1#{hash1}"), "lines": "zero"}]
        }))
        .await
        .unwrap();

    let content = fs::read_to_string("prepend.txt").unwrap();
    assert_eq!(content, "zero\nfirst\nsecond\n");
}

#[tokio::test]
async fn test_edit_tool_hashline_append() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("append.txt", "first\nsecond\n").unwrap();

    let read_tool = ReadTool::new();
    let read_result = read_tool
        .execute(serde_json::json!({"path": "append.txt", "hashline": true}))
        .await
        .unwrap();

    let hash1 = extract_hash_from_hashline(&read_result, 0);

    let edit_tool = EditTool;
    edit_tool
        .execute(serde_json::json!({
            "path": "append.txt",
            "edits": [{"op": "append", "pos": format!("1#{hash1}"), "lines": "inserted"}]
        }))
        .await
        .unwrap();

    let content = fs::read_to_string("append.txt").unwrap();
    assert_eq!(content, "first\ninserted\nsecond\n");
}

#[tokio::test]
async fn test_edit_tool_hashline_stale_hash() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("stale.txt", "original\n").unwrap();

    let edit_tool = EditTool;
    let result = edit_tool
        .execute(serde_json::json!({
            "path": "stale.txt",
            "edits": [{"op": "replace", "pos": "1#XX", "lines": "new"}]
        }))
        .await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("hash mismatch"),
        "expected 'hash mismatch' in error, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_edit_tool_hashline_overlapping_edits() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("overlap.txt", "a\nb\nc\nd\n").unwrap();

    let read_tool = ReadTool::new();
    let rr = read_tool
        .execute(serde_json::json!({"path": "overlap.txt", "hashline": true}))
        .await
        .unwrap();
    let h1 = extract_hash_from_hashline(&rr, 0);
    let h2 = extract_hash_from_hashline(&rr, 1);
    let h3 = extract_hash_from_hashline(&rr, 2);

    let edit_tool = EditTool;
    let result = edit_tool
        .execute(serde_json::json!({
            "path": "overlap.txt",
            "edits": [
                {"op": "replace", "pos": format!("1#{h1}"), "end": format!("2#{h2}"), "lines": "X"},
                {"op": "replace", "pos": format!("2#{h2}"), "end": format!("3#{h3}"), "lines": "Y"}
            ]
        }))
        .await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("overlapping"),
        "expected 'overlapping' in error, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_edit_tool_legacy_still_works() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("legacy.txt", "hello world\n").unwrap();

    let tool = EditTool;
    let result = tool
        .execute(serde_json::json!({
            "path": "legacy.txt",
            "old_string": "hello world",
            "new_string": "hi there"
        }))
        .await
        .unwrap();

    assert!(result.contains("-hello") || result.contains("+hi"));
    let content = fs::read_to_string("legacy.txt").unwrap();
    assert_eq!(content, "hi there\n");
}

#[tokio::test]
async fn test_edit_tool_multiple_non_overlapping_edits() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("multi.txt", "a\nb\nc\nd\ne\n").unwrap();

    let read_tool = ReadTool::new();
    let rr = read_tool
        .execute(serde_json::json!({"path": "multi.txt", "hashline": true}))
        .await
        .unwrap();
    let h1 = extract_hash_from_hashline(&rr, 0);
    let h5 = extract_hash_from_hashline(&rr, 4);

    let edit_tool = EditTool;
    edit_tool
        .execute(serde_json::json!({
            "path": "multi.txt",
            "edits": [
                {"op": "replace", "pos": format!("1#{h1}"), "lines": "A"},
                {"op": "replace", "pos": format!("5#{h5}"), "lines": "E"}
            ]
        }))
        .await
        .unwrap();

    let content = fs::read_to_string("multi.txt").unwrap();
    assert_eq!(content, "A\nb\nc\nd\nE\n");
}

#[tokio::test]
async fn test_edit_tool_returns_diff() {
    let (_dir, _guard) = sandbox_dir();
    fs::write("difftest.txt", "old line\n").unwrap();

    let tool = EditTool;
    let result = tool
        .execute(serde_json::json!({
            "path": "difftest.txt",
            "old_string": "old line",
            "new_string": "new line"
        }))
        .await
        .unwrap();

    assert!(result.contains("@@") || result.contains("---") || result.contains("+++"));
    assert!(result.contains("-old line"));
    assert!(result.contains("+new line"));
}

// ── Bash 工具测试 ──

#[tokio::test]
async fn test_bash_echo() {
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({"command": "echo hello"}))
        .await
        .unwrap();
    assert!(result.contains("hello"));
}

#[tokio::test]
async fn test_bash_missing_command() {
    let tool = BashTool::new();
    let err = tool.execute(serde_json::json!({})).await.unwrap_err();
    assert!(err.to_string().contains("command required"));
}

#[tokio::test]
async fn test_bash_workdir() {
    let (_dir, _guard) = sandbox_dir();
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({
            "command": "pwd",
            "workdir": "/tmp"
        }))
        .await
        .unwrap();
    assert!(result.contains("/tmp"));
}

#[tokio::test]
async fn test_bash_stderr_capture() {
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({
            "command": "echo error >&2"
        }))
        .await
        .unwrap();
    assert!(result.contains("stderr:"));
    assert!(result.contains("error"));
}

#[tokio::test]
async fn test_bash_exit_code_on_failure() {
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({
            "command": "exit 42"
        }))
        .await
        .unwrap();
    assert!(result.contains("exit code: 42"));
}

#[tokio::test]
async fn test_bash_bash_syntax() {
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({
            "command": "arr=(a b c); echo ${arr[1]}"
        }))
        .await
        .unwrap();
    assert!(result.contains("b"), "bash array syntax should work");
}

#[tokio::test]
async fn test_bash_timeout_triggers() {
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({
            "command": "sleep 10",
            "timeout": 1
        }))
        .await;
    assert!(result.unwrap_err().to_string().contains("timeout"));
}

#[tokio::test]
async fn test_bash_multiline_output() {
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({
            "command": "echo line1; echo line2"
        }))
        .await
        .unwrap();
    assert!(result.contains("line1"));
    assert!(result.contains("line2"));
}

#[tokio::test]
async fn test_bash_output_truncation() {
    let tool = BashTool::new();
    let line = "x".repeat(1000);
    let result = tool
        .execute(serde_json::json!({
            "command": format!("printf '%s' '{line}'")
        }))
        .await
        .unwrap();
    assert!(result.len() <= 50 * 1024 + 64, "output should be truncated");
}

#[tokio::test]
async fn test_bash_description_in_args() {
    let tool = BashTool::new();
    let result = tool
        .execute(serde_json::json!({
            "command": "echo ok",
            "description": "Print OK"
        }))
        .await
        .unwrap();
    assert!(result.contains("ok"), "description should not affect execution");
}

