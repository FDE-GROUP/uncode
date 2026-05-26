use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::oneshot;
use uncode_core::error::{UncodeError, UncodeResult};
use uncode_core::event::{AgentEvent, QuestionItem, QuestionOption, QuestionRequestData};
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

pub struct QuestionTool;

impl QuestionTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for QuestionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for QuestionTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "question".to_string(),
            description: "Ask the user one or more questions to gather information needed to continue. Use this when you need clarification, choices, or user input before proceeding.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "question": { "type": "string", "description": "The question to ask the user" },
                                "header": { "type": "string", "description": "Short label for this question (max 30 chars)" },
                                "options": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "label": { "type": "string", "description": "Display text (1-5 words)" },
                                            "description": { "type": "string", "description": "Explanation of this choice" }
                                        },
                                        "required": ["label", "description"]
                                    }
                                },
                                "multiple": { "type": "boolean", "description": "Allow selecting multiple choices" }
                            },
                            "required": ["question", "header", "options"]
                        }
                    }
                },
                "required": ["questions"]
            }),
            label: Some("ask_user".to_string()),
            execution_mode: ExecutionMode::Sequential,
        }
    }

    async fn execute(&self, _arguments: Value) -> Result<String, UncodeError> {
        Ok("question tool requires interactive context".to_string())
    }

    async fn execute_with_context(
        &self,
        arguments: Value,
        ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let questions = parse_questions(&arguments)?;

        if questions.is_empty() {
            return Ok(ToolResult::err("no questions provided"));
        }

        // Set up response channel
        let (tx, rx) = oneshot::channel();

        // Emit question request to TUI
        {
            let event = AgentEvent::QuestionRequest {
                data: Box::new(QuestionRequestData {
                    tool_call_id: ctx.tool_call_id.clone(),
                    title: "Agent Question".to_string(),
                    questions,
                }),
            };
            crate::tools::question_registry::send_event(event);
        }

        // Store sender in registry so TUI can respond
        let _ = crate::tools::question_registry::register(&ctx.tool_call_id, tx);

        // Wait for user response
        match rx.await {
            Ok(answers) => {
                let formatted = format_answers(&answers);
                Ok(ToolResult::ok(formatted).with_details(serde_json::json!({
                    "title": "Answered questions",
                    "answers": answers
                })))
            }
            Err(_) => Ok(ToolResult::err("question was dismissed or timed out")),
        }
    }
}

fn parse_questions(args: &Value) -> Result<Vec<QuestionItem>, UncodeError> {
    let arr = args["questions"]
        .as_array()
        .ok_or_else(|| UncodeError::Tool("questions must be an array".into()))?;

    let mut result = Vec::new();
    for item in arr {
        let question = item["question"].as_str().unwrap_or("").to_string();
        let header = item["header"].as_str().unwrap_or("").to_string();
        let multiple = item["multiple"].as_bool().unwrap_or(false);
        let mut options = Vec::new();
        if let Some(opts) = item["options"].as_array() {
            for opt in opts {
                options.push(QuestionOption {
                    label: opt["label"].as_str().unwrap_or("").to_string(),
                    description: opt["description"].as_str().unwrap_or("").to_string(),
                });
            }
        }
        result.push(QuestionItem {
            question,
            header,
            options,
            multiple,
        });
    }
    Ok(result)
}

fn format_answers(answers: &[Vec<String>]) -> String {
    answers
        .iter()
        .enumerate()
        .map(|(i, a)| format!("Q{}: {}", i + 1, a.join(", ")))
        .collect::<Vec<_>>()
        .join("\n")
}
