use async_trait::async_trait;
use uncode_core::error::{UncodeError, UncodeResult};
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

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
                "properties": {
                    "path": {"type": "string", "description": "目录路径（相对或绝对），默认当前目录"}
                }
            }),
            label: Some("List Directory".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let raw = arguments["path"].as_str().unwrap_or(".").to_string();
        let resolved = super::resolve_path(&raw).map_err(UncodeError::Tool)?;
        let display = resolved.display().to_string();

        tokio::task::spawn_blocking(move || list_dir(&display))
            .await
            .map_err(|e| UncodeError::Tool(format!("ls task failed: {e}")))?
    }
}

fn list_dir(path: &str) -> UncodeResult<String> {
    let entries =
        std::fs::read_dir(path).map_err(|e| UncodeError::Tool(format!("ls {path}: {e}")))?;

    let mut results: Vec<String> = entries
        .flatten()
        .take(500)
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                format!("{name}/")
            } else {
                name
            }
        })
        .collect();

    if results.is_empty() {
        Ok("(empty)".into())
    } else {
        results.sort();
        Ok(results.join("\n"))
    }
}
