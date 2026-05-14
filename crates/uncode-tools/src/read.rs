use std::fs;
use std::path::PathBuf;

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
        let lines: Vec<&str> = content.lines().collect();

        if offset >= lines.len() {
            return Ok(String::new());
        }

        let limit = arguments["limit"]
            .as_u64()
            .map(|l| l as usize)
            .unwrap_or(lines.len() - offset);

        let end = (offset + limit).min(lines.len());
        let selected = &lines[offset..end];

        let mut result = String::new();
        for (i, line) in selected.iter().enumerate() {
            result.push_str(&format!("{:>6}: {line}\n", offset + i + 1));
        }

        Ok(result)
    }
}

pub fn read_tool_path(path: &str) -> PathBuf {
    PathBuf::from(path)
}
