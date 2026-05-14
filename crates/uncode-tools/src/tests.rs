#[cfg(test)]
mod tests {
    use std::fs;
    use uncode_core::tool::ToolExecutor;

    use crate::edit::EditTool;
    use crate::read::ReadTool;
    use crate::registry::ToolRegistry;
    use crate::write::WriteTool;

    #[tokio::test]
    async fn test_read_tool() {
        let dir = std::env::temp_dir().join(format!("uncode-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        fs::write(&path, "line 1\nline 2\nline 3\nline 4\nline 5\n").unwrap();

        let tool = ReadTool::new();
        let result = tool
            .execute(serde_json::json!({"path": path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.contains("line 1"));
        assert!(result.contains("line 5"));

        let result = tool
            .execute(serde_json::json!({"path": path.to_str().unwrap(), "offset": 2, "limit": 2}))
            .await
            .unwrap();
        assert!(result.contains("line 3"));
        assert!(!result.contains("line 1"));

        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_write_tool() {
        let dir = std::env::temp_dir().join(format!("uncode-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.txt");

        let tool = WriteTool;
        let result = tool
            .execute(serde_json::json!({"path": path.to_str().unwrap(), "content": "hello world"}))
            .await
            .unwrap();
        assert!(result.contains("wrote"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");

        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_edit_tool() {
        let dir = std::env::temp_dir().join(format!("uncode-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("edit_test.txt");
        fs::write(&path, "hello world\nfoo bar\n").unwrap();

        let tool = EditTool;
        tool.execute(serde_json::json!({
            "path": path.to_str().unwrap(),
            "old_string": "hello world",
            "new_string": "hi there"
        }))
        .await
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("hi there"));
        assert!(!content.contains("hello world"));

        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_edit_tool_not_found() {
        let dir = std::env::temp_dir().join(format!("uncode-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notfound.txt");
        fs::write(&path, "content").unwrap();

        let tool = EditTool;
        let result = tool
            .execute(serde_json::json!({
                "path": path.to_str().unwrap(),
                "old_string": "does not exist",
                "new_string": "replacement"
            }))
            .await;
        assert!(result.is_err());

        fs::remove_dir_all(dir).ok();
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
}
