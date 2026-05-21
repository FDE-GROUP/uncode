use std::path::PathBuf;

use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

use super::diff::unified_diff;

#[derive(Default)]
pub struct WriteTool;

struct WriteJob {
    path: PathBuf,
    content: String,
    display: String,
}

fn write_blocking(job: WriteJob) -> Result<String, String> {
    let old_content = std::fs::read_to_string(&job.path).unwrap_or_default();
    super::atomic_write(&job.path, &job.content)?;
    Ok(unified_diff(&old_content, &job.content, &job.display))
}

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
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("content required".into()))?
            .to_string();

        let resolved = super::resolve_path(raw).map_err(uncode_core::error::UncodeError::Tool)?;
        let job = WriteJob {
            display: resolved.display().to_string(),
            path: resolved,
            content,
        };

        tokio::task::spawn_blocking(move || write_blocking(job))
            .await
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("write task failed: {e}")))?
            .map_err(uncode_core::error::UncodeError::Tool)
    }
}
