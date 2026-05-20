use async_trait::async_trait;
use std::fs;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

use super::diff::unified_diff;

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
                "additionalProperties": false,
                "properties": {
                    "path": {"type": "string", "description": "文件路径（相对或绝对）"},
                    "content": {"type": "string", "description": "写入的内容"}
                },
                "required": ["path", "content"]
            }),
            label: Some("Write File".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let raw = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let content = arguments["content"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("content required".into()))?;

        let resolved = super::resolve_path(raw).map_err(uncode_core::error::UncodeError::Tool)?;
        let display = resolved.display().to_string();

        let old_content = fs::read_to_string(&resolved).unwrap_or_default();

        super::atomic_write(&resolved, content).map_err(uncode_core::error::UncodeError::Tool)?;

        Ok(unified_diff(&old_content, content, &display))
    }
}
