use async_trait::async_trait;
use std::fs;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

pub struct LsTool;

impl Default for LsTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl ToolExecutor for LsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ls".into(),
            description: "列出目录内容".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "目录路径，默认当前目录"}
                }
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let path = arguments["path"].as_str().unwrap_or(".");
        let entries = fs::read_dir(path)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("ls {path}: {e}")))?;

        let mut results = Vec::new();
        for entry in entries.flatten().take(500) {
            let file_type = entry.file_type().ok();
            let is_dir = file_type.map(|t| t.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().to_string();
            if is_dir {
                results.push(format!("{name}/"));
            } else {
                results.push(name);
            }
        }

        if results.is_empty() {
            Ok("(empty)".into())
        } else {
            results.sort();
            Ok(results.join("\n"))
        }
    }
}
