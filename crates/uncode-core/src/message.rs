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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    ToolCall(ToolCall),
    /// 工具执行结果
    ToolResult(ToolResult),
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

        // 自定义消息类型转为 user message
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
