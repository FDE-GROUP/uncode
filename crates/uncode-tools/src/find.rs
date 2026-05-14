use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

pub struct FindTool;

impl Default for FindTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl ToolExecutor for FindTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "find".into(),
            description: "按文件名模式查找文件".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "glob 模式，如 **/*.rs"},
                    "path": {"type": "string", "description": "搜索根目录，默认当前目录"}
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let pattern = arguments["pattern"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("pattern required".into()))?;
        let root = arguments["path"].as_str().unwrap_or(".");

        let mut results = Vec::new();
        for entry in glob::glob(&format!("{root}/{pattern}"))
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("glob: {e}")))?
            .flatten()
            .take(200)
        {
            results.push(entry.display().to_string());
        }

        if results.is_empty() {
            Ok("no files found".into())
        } else if results.len() >= 200 {
            results.push("... (truncated)".into());
            Ok(results.join("\n"))
        } else {
            Ok(results.join("\n"))
        }
    }
}
