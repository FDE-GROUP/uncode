use std::fs;

use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

pub struct ReadTool {
    max_size: usize,
}

impl ReadTool {
    pub fn new() -> Self {
        Self {
            max_size: 1024 * 1024,
        }
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for ReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read".into(),
            description: "读取文件内容，支持 offset/limit 控制范围".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件路径"},
                    "offset": {"type": "integer", "description": "起始行号"},
                    "limit": {"type": "integer", "description": "读取行数"}
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let path = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let content = fs::read_to_string(path)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("read {}: {e}", path)))?;

        if content.len() > self.max_size {
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "file too large ({})",
                content.len()
            )));
        }

        let offset = arguments["offset"].as_u64().unwrap_or(0) as usize;
        let limit = arguments["limit"].as_u64().map(|l| l as usize);

        let result = content
            .lines()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .enumerate()
            .fold(
                String::with_capacity(limit.unwrap_or(0).min(content.len()).saturating_mul(80)),
                |mut acc, (i, line)| {
                    use std::fmt::Write;
                    let _ = writeln!(acc, "{:>6}: {line}", offset + i + 1);
                    acc
                },
            );

        Ok(result)
    }
}
