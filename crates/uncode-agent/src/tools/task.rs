use async_trait::async_trait;
use serde_json::Value;
use uncode_core::error::{UncodeError, UncodeResult};
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

pub struct TaskTool;

impl TaskTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskTool {
    fn default() -> Self {
        Self::new()
    }
}

fn subagent_system_prompt(subagent_type: &str) -> String {
    let base = "You are a subagent. Complete the assigned task and return ONLY the result. Do not ask follow-up questions.".to_string();

    match subagent_type {
        "explore" => format!(
            "{base}\n\nYou are specialized in codebase exploration. \
            Use read, grep, glob, and ls tools to understand the codebase structure. \
            Return a concise summary of your findings."
        ),
        "general" => format!(
            "{base}\n\nYou are a general-purpose subagent. \
            Use available tools to complete the task. Return a concise summary."
        ),
        _ => format!(
            "{base}\n\nYou are a '{subagent_type}' subagent. \
            Use available tools to complete the task. Return a concise summary."
        ),
    }
}

#[async_trait]
impl ToolExecutor for TaskTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "task".to_string(),
            description: "Launch a new agent to handle complex, multistep tasks autonomously. Use this for subagent delegation when a task requires focused tool use.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "A short (3-5 words) description of the task"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The detailed task for the subagent to perform"
                    },
                    "subagent_type": {
                        "type": "string",
                        "description": "The type of subagent: 'general' (default) or 'explore'",
                        "default": "general"
                    }
                },
                "required": ["description", "prompt"]
            }),
            label: Some("delegate_task".to_string()),
            execution_mode: ExecutionMode::Sequential,
        }
    }

    async fn execute(&self, _arguments: Value) -> Result<String, UncodeError> {
        Ok("task tool requires ToolContext with subagent_runner".to_string())
    }

    async fn execute_with_context(
        &self,
        arguments: Value,
        ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let description = arguments["description"]
            .as_str()
            .unwrap_or("subagent task")
            .to_string();
        let prompt = arguments["prompt"]
            .as_str()
            .ok_or_else(|| UncodeError::Tool("prompt required".into()))?
            .to_string();
        let subagent_type = arguments["subagent_type"]
            .as_str()
            .unwrap_or("general")
            .to_string();

        let system_prompt = subagent_system_prompt(&subagent_type);

        let runner = ctx
            .subagent_runner
            .ok_or_else(|| UncodeError::Tool("subagent runner not available".into()))?;

        let model = arguments["model"]
            .as_str()
            .or(ctx.current_model.as_deref())
            .unwrap_or("default")
            .to_string();

        match runner
            .run_blocking(system_prompt.clone(), prompt.clone(), model)
            .await
        {
            Ok(output) => {
                let title = format!("{} Task: {description}", capitalize(&subagent_type));
                Ok(ToolResult::ok(output).with_details(serde_json::json!({
                    "title": title,
                    "subagent_type": subagent_type,
                    "description": description,
                })))
            }
            Err(e) => Ok(ToolResult::err(format!("task failed: {e}"))),
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
