#[cfg(test)]
mod tests {
    use std::fs;
    use uncode_core::tool::ToolExecutor;

    use crate::edit::EditTool;
    use crate::find::FindTool;
    use crate::ls::LsTool;
    use crate::read::ReadTool;
    use crate::registry::ToolRegistry;
    use crate::write::WriteTool;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("uncode-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Create temp dir and set it as cwd so sandbox checks pass.
    /// Tests must use relative paths to stay within sandbox.
    fn sandbox_dir() -> std::path::PathBuf {
        let dir = temp_dir();
        std::env::set_current_dir(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_read_tool() {
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();

        let tool = WriteTool;
        let result = tool
            .execute(serde_json::json!({"path": "output.txt", "content": "hello world"}))
            .await
            .unwrap();
        assert!(result.contains("wrote"));
        assert_eq!(fs::read_to_string("output.txt").unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_write_tool_creates_parent_dirs() {
        let _dir = sandbox_dir();

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
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();
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
        let _dir = sandbox_dir();
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
}
