use async_trait::async_trait;
use std::fs;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

pub struct WriteTool;

impl Default for WriteTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl ToolExecutor for WriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write".into(),
            description: "向文件写入内容，覆盖已有文件".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件路径"},
                    "content": {"type": "string", "description": "写入的内容"}
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let path = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let content = arguments["content"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("content required".into()))?;

        if let Some(parent) = std::path::Path::new(path).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| uncode_core::error::UncodeError::Tool(format!("write {path}: {e}")))?;
        }

        fs::write(path, content)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("write {path}: {e}")))?;

        Ok(format!("wrote {} bytes to {path}", content.len()))
    }
}
