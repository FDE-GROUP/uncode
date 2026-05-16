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
