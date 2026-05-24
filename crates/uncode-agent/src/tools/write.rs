use std::path::PathBuf;

use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

use super::diff::unified_diff;

const MAX_WRITE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Default)]
pub struct WriteTool;

struct WriteJob {
    path: PathBuf,
    content: String,
    display: String,
    old_content: String,
}

fn write_blocking(job: WriteJob) -> Result<String, String> {
    super::atomic_write(&job.path, &job.content)?;
    Ok(unified_diff(&job.old_content, &job.content, &job.display))
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

    fn prepare_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, uncode_core::error::UncodeError> {
        super::prepare_arguments_path(arguments, "path", None, &[])
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let tr = self
            .execute_with_context(
                arguments,
                ToolContext {
                    cancel_token: tokio_util::sync::CancellationToken::new(),
                    on_progress: None,
                    tool_call_id: String::new(),
                    execution_env: None,
                    allowed_paths: Vec::new(),
                },
            )
            .await?;
        Ok(tr.text_content())
    }

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let raw = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let content = arguments["content"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("content required".into()))?
            .to_string();

        if content.len() > MAX_WRITE_BYTES {
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "content exceeds maximum write size ({} MB)",
                MAX_WRITE_BYTES / (1024 * 1024)
            )));
        }

        let bytes_written = content.len();

        let resolved = super::resolve_path(raw, &ctx.allowed_paths)
            .map_err(uncode_core::error::UncodeError::Tool)?;
        let display = resolved.display().to_string();

        let env = super::ctx_execution_env(&ctx);
        let old_content = env.fs().read_text_file(&resolved).await.unwrap_or_default();

        let job = WriteJob {
            display,
            path: resolved,
            content,
            old_content,
        };

        let output = tokio::task::spawn_blocking(move || write_blocking(job))
            .await
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("write task failed: {e}")))?
            .map_err(uncode_core::error::UncodeError::Tool)?;

        Ok(ToolResult::ok(output)
            .with_details(serde_json::json!({ "bytes_written": bytes_written })))
    }
}
