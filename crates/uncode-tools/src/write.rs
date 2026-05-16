use async_trait::async_trait;
use std::fs;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

#[derive(Default)]
pub struct WriteTool;

#[async_trait]
impl ToolExecutor for WriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write".into(),
            description: "向文件写入内容，覆盖已有文件".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件路径（相对或绝对）"},
                    "content": {"type": "string", "description": "写入的内容"}
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let raw = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let content = arguments["content"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("content required".into()))?;

        let resolved = crate::resolve_path(raw).map_err(uncode_core::error::UncodeError::Tool)?;

        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("write {}: {e}", resolved.display()))
            })?;
        }

        fs::write(&resolved, content).map_err(|e| {
            uncode_core::error::UncodeError::Tool(format!("write {}: {e}", resolved.display()))
        })?;

        Ok(format!(
            "wrote {} bytes to {}",
            content.len(),
            resolved.display()
        ))
    }
}
