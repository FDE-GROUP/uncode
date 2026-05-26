use async_trait::async_trait;
use uncode_core::error::{UncodeError, UncodeResult};
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

const MAX_DIR_ENTRIES: usize = 500;

#[derive(Default)]
pub struct LsTool;

#[async_trait]
impl ToolExecutor for LsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ls".into(),
            description: "列出目录内容".into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "path": {"type": "string", "description": "目录路径（相对或绝对），默认当前目录"}
                }
            }),
            label: Some("List Directory".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    fn prepare_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, uncode_core::error::UncodeError> {
        super::prepare_arguments_path(arguments, "path", Some("."), &[])
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
                    subagent_runner: None,
                    current_model: None,
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
        let raw = arguments["path"].as_str().unwrap_or(".").to_string();
        let resolved =
            super::resolve_path(&raw, &ctx.allowed_paths).map_err(|e| UncodeError::Tool(e))?;
        let display = resolved.display().to_string();

        let env = super::ctx_execution_env(&ctx);
        let info = env
            .fs()
            .file_info(&resolved)
            .await
            .map_err(|e| UncodeError::Tool(format!("ls {display}: {e}")))?;

        if !info.is_dir {
            return Ok(ToolResult::err(format!("{display} is not a directory")));
        }

        let entries = env
            .fs()
            .list_dir(&resolved)
            .await
            .map_err(|e| UncodeError::Tool(format!("ls {display}: {e}")))?;

        let mut results: Vec<String> = entries
            .into_iter()
            .take(MAX_DIR_ENTRIES)
            .map(|e| {
                if e.is_dir {
                    format!("{}/", e.name)
                } else {
                    e.name
                }
            })
            .collect();

        if results.is_empty() {
            Ok(ToolResult::ok("(empty)"))
        } else {
            results.sort_unstable();
            Ok(ToolResult::ok(results.join("\n")))
        }
    }
}
