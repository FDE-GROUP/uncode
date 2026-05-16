use async_trait::async_trait;
use std::fs;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

#[derive(Default)]
pub struct EditTool;

#[async_trait]
impl ToolExecutor for EditTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit".into(),
            description: "在文件中执行字符串替换".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件路径（相对或绝对）"},
                    "old_string": {"type": "string", "description": "要替换的原字符串"},
                    "new_string": {"type": "string", "description": "替换后的新字符串"}
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let raw = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;
        let old_string = arguments["old_string"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("old_string required".into()))?;
        let new_string = arguments["new_string"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("new_string required".into()))?;

        let resolved = crate::resolve_path(raw);
        let display = resolved.display().to_string();

        let content = fs::read_to_string(&resolved)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("edit {display}: {e}")))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "old_string not found in {display}"
            )));
        }
        if count > 1 {
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "old_string found {count} times in {display}, must be unique"
            )));
        }

        let new_content = content.replacen(old_string, new_string, 1);
        fs::write(&resolved, &new_content)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("edit {display}: {e}")))?;

        Ok(format!("edited {display}"))
    }
}
