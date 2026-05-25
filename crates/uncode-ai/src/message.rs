use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 会话中的一条消息，由 role 和 content blocks 组成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<crate::api_types::StopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

/// 消息发送者角色
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => f.write_str("system"),
            Self::User => f.write_str("user"),
            Self::Assistant => f.write_str("assistant"),
            Self::Tool => f.write_str("tool"),
        }
    }
}

/// 消息内容块，一条消息可以包含多种类型的块
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlock {
    /// 纯文本内容
    Text { text: String },
    /// LLM 的思考/推理过程
    Thinking { text: String },
    /// LLM 请求调用工具
    ToolCall(Box<ToolCall>),
    /// 工具执行结果
    ToolResult(Box<ToolResult>),
    /// 图片内容（base64 编码）
    Image { mime_type: String, data: String },
    /// Bash 执行结果（exclude_from_context=true 时从 LLM context 丢弃）
    BashExecution {
        command: String,
        output: String,
        exit_code: i32,
        cancelled: bool,
        #[serde(default)]
        exclude_from_context: bool,
    },
    /// 分支切换时生成的摘要
    BranchSummary { summary: String, from_id: String },
    /// 上下文压缩摘要
    CompactionSummary {
        summary: String,
        #[serde(default)]
        tokens_before: u64,
    },
}

/// LLM 请求的工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// 工具调用的执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// Token 用量统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostInfo>,
}

/// 费用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostInfo {
    pub input_cost: f64,
    pub output_cost: f64,
}

impl Message {
    pub fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            content,
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, vec![ContentBlock::Text { text: text.into() }])
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(
            Role::Assistant,
            vec![ContentBlock::Text { text: text.into() }],
        )
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, vec![ContentBlock::Text { text: text.into() }])
    }

    pub fn with_usage(mut self, input_tokens: u64, output_tokens: u64) -> Self {
        self.usage = Some(UsageInfo {
            input_tokens,
            output_tokens,
            cost: None,
        });
        self
    }
}

/// Pi 风格消息桥接：过滤非 LLM 消息，转换自定义类型为标准 LLM 消息。
///
/// - bashExecution + exclude_from_context → 丢弃
/// - bashExecution（可见）→ 包裹为 user message
/// - branchSummary → 包裹前缀后转为 user message
/// - compactionSummary → 包裹前缀后转为 user message
/// - user/assistant/toolResult → 透传
pub fn convert_to_llm(messages: Vec<Message>) -> Vec<Message> {
    let mut result = Vec::with_capacity(messages.len());
    for msg in messages {
        let converted_blocks: Vec<ContentBlock> = msg
            .content
            .into_iter()
            .filter_map(|block| match block {
                b @ (ContentBlock::Text { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::ToolCall(_)
                | ContentBlock::ToolResult(_)
                | ContentBlock::Image { .. }) => Some(b),

                ContentBlock::BashExecution {
                    exclude_from_context,
                    ..
                } if exclude_from_context => None,

                ContentBlock::BashExecution {
                    command,
                    output,
                    exit_code,
                    cancelled,
                    ..
                } => Some(ContentBlock::Text {
                    text: format!(
                        "[bash] {command}\n{output}\nexit: {exit_code}{}",
                        if cancelled { " (cancelled)" } else { "" }
                    ),
                }),

                ContentBlock::BranchSummary { summary, from_id } => {
                    Some(ContentBlock::Text {
                        text: format!(
                            "[branch summary from {from_id}]\n{summary}\n[/branch summary]"
                        ),
                    })
                }

                ContentBlock::CompactionSummary {
                    summary,
                    tokens_before,
                } => Some(ContentBlock::Text {
                    text: format!(
                        "[compaction summary (tokens before: {tokens_before})]\n{summary}\n[/compaction summary]"
                    ),
                }),
            })
            .collect();

        if converted_blocks.is_empty() {
            continue;
        }

        let role = match msg.role {
            Role::User | Role::Assistant | Role::System | Role::Tool => msg.role,
        };

        result.push(Message {
            id: msg.id,
            role,
            content: converted_blocks,
            usage: msg.usage,
            stop_reason: msg.stop_reason,
            error_message: msg.error_message,
            timestamp: msg.timestamp,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_message_new_with_different_roles() {
        let user_msg = Message::new(Role::User, vec![]);
        assert_eq!(user_msg.role, Role::User);

        let asst_msg = Message::new(Role::Assistant, vec![]);
        assert_eq!(asst_msg.role, Role::Assistant);

        let sys_msg = Message::new(Role::System, vec![]);
        assert_eq!(sys_msg.role, Role::System);

        let tool_msg = Message::new(Role::Tool, vec![]);
        assert_eq!(tool_msg.role, Role::Tool);
    }

    #[test]
    fn test_convenience_constructors() {
        let u = Message::user("hello");
        assert_eq!(u.role, Role::User);
        assert!(matches!(&u.content[0], ContentBlock::Text { text } if text == "hello"));

        let a = Message::assistant("hi");
        assert_eq!(a.role, Role::Assistant);
        assert!(matches!(&a.content[0], ContentBlock::Text { text } if text == "hi"));

        let s = Message::system("beep");
        assert_eq!(s.role, Role::System);
        assert!(matches!(&s.content[0], ContentBlock::Text { text } if text == "beep"));
    }

    #[test]
    fn test_with_usage_builder() {
        let msg = Message::user("test").with_usage(10, 20);
        let usage = msg.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
        assert!(usage.cost.is_none());
    }

    #[test]
    fn test_message_serde_roundtrip() {
        let msg = Message::user("hello").with_usage(5, 10);
        let json = serde_json::to_string(&msg).unwrap();
        let de: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(de.role, Role::User);
        assert!(matches!(&de.content[0], ContentBlock::Text { text } if text == "hello"));
        assert_eq!(de.usage.unwrap().input_tokens, 5);
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::System.to_string(), "system");
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Assistant.to_string(), "assistant");
        assert_eq!(Role::Tool.to_string(), "tool");
    }

    #[test]
    fn test_role_serde_roundtrip() {
        for role in &[Role::System, Role::User, Role::Assistant, Role::Tool] {
            let json = serde_json::to_string(role).unwrap();
            let de: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(de, *role);
        }
    }

    #[test]
    fn test_content_block_text_serde() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(matches!(de, ContentBlock::Text { text } if text == "hello"));
    }

    #[test]
    fn test_content_block_thinking_serde() {
        let block = ContentBlock::Thinking { text: "hmm".into() };
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(matches!(de, ContentBlock::Thinking { text } if text == "hmm"));
    }

    #[test]
    fn test_content_block_tool_call_serde() {
        let tc = ToolCall {
            id: "call_1".into(),
            name: "get_weather".into(),
            arguments: serde_json::json!({"city": "Beijing"}),
        };
        let block = ContentBlock::ToolCall(Box::new(tc));
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        match de {
            ContentBlock::ToolCall(tc) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "get_weather");
                assert_eq!(tc.arguments["city"], "Beijing");
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn test_content_block_tool_result_serde() {
        let tr = ToolResult {
            tool_call_id: "call_1".into(),
            content: "sunny".into(),
            is_error: false,
        };
        let block = ContentBlock::ToolResult(Box::new(tr));
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        match de {
            ContentBlock::ToolResult(tr) => {
                assert_eq!(tr.tool_call_id, "call_1");
                assert_eq!(tr.content, "sunny");
                assert!(!tr.is_error);
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn test_content_block_image_serde() {
        let block = ContentBlock::Image {
            mime_type: "image/png".into(),
            data: "base64data".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        match de {
            ContentBlock::Image { mime_type, data } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(data, "base64data");
            }
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn test_content_block_bash_execution_serde() {
        let block = ContentBlock::BashExecution {
            command: "ls".into(),
            output: "file.txt".into(),
            exit_code: 0,
            cancelled: false,
            exclude_from_context: false,
        };
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        match de {
            ContentBlock::BashExecution {
                command,
                output: _,
                exit_code,
                cancelled,
                exclude_from_context,
            } => {
                assert_eq!(command, "ls");
                assert_eq!(exit_code, 0);
                assert!(!cancelled);
                assert!(!exclude_from_context);
            }
            _ => panic!("expected BashExecution"),
        }
    }

    #[test]
    fn test_content_block_branch_summary_serde() {
        let block = ContentBlock::BranchSummary {
            summary: "worked".into(),
            from_id: "branch_1".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        match de {
            ContentBlock::BranchSummary { summary, from_id } => {
                assert_eq!(summary, "worked");
                assert_eq!(from_id, "branch_1");
            }
            _ => panic!("expected BranchSummary"),
        }
    }

    #[test]
    fn test_content_block_compaction_summary_serde() {
        let block = ContentBlock::CompactionSummary {
            summary: "compressed".into(),
            tokens_before: 5000,
        };
        let json = serde_json::to_string(&block).unwrap();
        let de: ContentBlock = serde_json::from_str(&json).unwrap();
        match de {
            ContentBlock::CompactionSummary {
                summary,
                tokens_before,
            } => {
                assert_eq!(summary, "compressed");
                assert_eq!(tokens_before, 5000);
            }
            _ => panic!("expected CompactionSummary"),
        }
    }

    #[test]
    fn test_tool_call_construction() {
        let tc = ToolCall {
            id: "call_abc".into(),
            name: "search".into(),
            arguments: serde_json::json!({"q": "rust"}),
        };
        assert_eq!(tc.id, "call_abc");
        assert_eq!(tc.name, "search");
        assert_eq!(tc.arguments["q"], "rust");
    }

    #[test]
    fn test_tool_result_construction() {
        let tr = ToolResult {
            tool_call_id: "call_abc".into(),
            content: "results".into(),
            is_error: false,
        };
        assert_eq!(tr.tool_call_id, "call_abc");
        assert_eq!(tr.content, "results");
        assert!(!tr.is_error);
    }

    #[test]
    fn test_usage_info_and_cost_serde() {
        let usage = UsageInfo {
            input_tokens: 100,
            output_tokens: 200,
            cost: Some(CostInfo {
                input_cost: 0.001,
                output_cost: 0.002,
            }),
        };
        let json = serde_json::to_string(&usage).unwrap();
        let de: UsageInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(de.input_tokens, 100);
        assert_eq!(de.output_tokens, 200);
        let cost = de.cost.unwrap();
        assert!((cost.input_cost - 0.001).abs() < 1e-9);
        assert!((cost.output_cost - 0.002).abs() < 1e-9);
    }

    // ── convert_to_llm tests ──

    #[test]
    fn test_convert_passthrough_text() {
        let msg = Message::user("hello");
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        assert!(matches!(&result[0].content[0], ContentBlock::Text { text } if text == "hello"));
    }

    #[test]
    fn test_convert_passthrough_thinking() {
        let msg = Message {
            id: "id".into(),
            role: Role::Assistant,
            content: vec![ContentBlock::Thinking {
                text: "reasoning".into(),
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].content[0], ContentBlock::Thinking { text } if text == "reasoning")
        );
    }

    #[test]
    fn test_convert_passthrough_tool_call() {
        let tc = ToolCall {
            id: "c1".into(),
            name: "f".into(),
            arguments: serde_json::json!({}),
        };
        let msg = Message {
            id: "id".into(),
            role: Role::Assistant,
            content: vec![ContentBlock::ToolCall(Box::new(tc))],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        assert!(matches!(&result[0].content[0], ContentBlock::ToolCall(_)));
    }

    #[test]
    fn test_convert_passthrough_tool_result() {
        let tr = ToolResult {
            tool_call_id: "c1".into(),
            content: "ok".into(),
            is_error: false,
        };
        let msg = Message {
            id: "id".into(),
            role: Role::Tool,
            content: vec![ContentBlock::ToolResult(Box::new(tr))],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        assert!(matches!(&result[0].content[0], ContentBlock::ToolResult(_)));
    }

    #[test]
    fn test_convert_passthrough_image() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![ContentBlock::Image {
                mime_type: "image/png".into(),
                data: "abc".into(),
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        assert!(matches!(&result[0].content[0], ContentBlock::Image { .. }));
    }

    #[test]
    fn test_convert_bash_exclude_from_context_drops() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![ContentBlock::BashExecution {
                command: "ls".into(),
                output: "x".into(),
                exit_code: 0,
                cancelled: false,
                exclude_from_context: true,
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_convert_bash_visible_wraps_as_text() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![ContentBlock::BashExecution {
                command: "echo hi".into(),
                output: "hi".into(),
                exit_code: 0,
                cancelled: false,
                exclude_from_context: false,
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        match &result[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.starts_with("[bash]"));
                assert!(text.contains("echo hi"));
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_convert_bash_cancelled_suffix() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![ContentBlock::BashExecution {
                command: "sleep 10".into(),
                output: "".into(),
                exit_code: -1,
                cancelled: true,
                exclude_from_context: false,
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        match &result[0].content[0] {
            ContentBlock::Text { text } => assert!(text.contains("(cancelled)")),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_convert_branch_summary_wraps_as_text() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![ContentBlock::BranchSummary {
                summary: "did stuff".into(),
                from_id: "b1".into(),
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        match &result[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.starts_with("[branch summary from b1]"));
                assert!(text.contains("did stuff"));
                assert!(text.ends_with("[/branch summary]"));
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_convert_compaction_summary_wraps_as_text() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![ContentBlock::CompactionSummary {
                summary: "compressed".into(),
                tokens_before: 8000,
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        match &result[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.starts_with("[compaction summary (tokens before: 8000)]"));
                assert!(text.contains("compressed"));
                assert!(text.ends_with("[/compaction summary]"));
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_convert_empty_blocks_skips_message() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![ContentBlock::BashExecution {
                command: "ls".into(),
                output: "x".into(),
                exit_code: 0,
                cancelled: false,
                exclude_from_context: true,
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_convert_multiple_messages() {
        let msgs = vec![Message::user("hello"), Message::assistant("world")];
        let result = convert_to_llm(msgs);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, Role::User);
        assert_eq!(result[1].role, Role::Assistant);
    }

    #[test]
    fn test_convert_mixed_content_blocks() {
        let msg = Message {
            id: "id".into(),
            role: Role::User,
            content: vec![
                ContentBlock::Text { text: "hi".into() },
                ContentBlock::BashExecution {
                    command: "ls".into(),
                    output: "f1".into(),
                    exit_code: 0,
                    cancelled: false,
                    exclude_from_context: true,
                },
                ContentBlock::BashExecution {
                    command: "pwd".into(),
                    output: "/home".into(),
                    exit_code: 0,
                    cancelled: false,
                    exclude_from_context: false,
                },
            ],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content.len(), 2);
        assert!(matches!(&result[0].content[0], ContentBlock::Text { text } if text == "hi"));
        match &result[0].content[1] {
            ContentBlock::Text { text } => assert!(text.starts_with("[bash]")),
            _ => panic!("expected Text"),
        }
    }
}
