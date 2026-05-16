use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

#[derive(Default)]
pub struct FindTool;

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
            label: Some("Find Files".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let pattern = arguments["pattern"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("pattern required".into()))?;
        let root_raw = arguments["path"].as_str().unwrap_or(".");
        let root = crate::resolve_path(root_raw).map_err(uncode_core::error::UncodeError::Tool)?;

        let glob_pattern = format!("{}/{}", root.display(), pattern);
        let mut results: Vec<String> = glob::glob(&glob_pattern)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("glob: {e}")))?
            .flatten()
            .take(200)
            .map(|e| e.display().to_string())
            .collect();

        if results.is_empty() {
            return Ok("no files found".into());
        }
        if results.len() >= 200 {
            results.push("... (truncated)".into());
        }
        Ok(results.join("\n"))
    }
}
