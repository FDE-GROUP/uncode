use async_trait::async_trait;
use std::fs;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

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
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let raw = arguments["path"].as_str().unwrap_or(".");
        let resolved = crate::resolve_path(raw)
            .map_err(uncode_core::error::UncodeError::Tool)?;
        let display = resolved.display().to_string();

        let entries = fs::read_dir(&resolved)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("ls {display}: {e}")))?;

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
}
