use async_trait::async_trait;
use futures::stream::BoxStream;
use uncode_core::message::Message;
use uncode_core::tool::ToolDefinition;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallDelta {
        id: String,
        arguments: String,
    },
    ToolCallEnd {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    Usage(UsageInfo),
    Error(String),
    Done,
}

#[derive(Debug, Clone)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// LLM 驱动 trait，所有供应商必须实现
///
/// 封装不同 LLM API 的差异，上层通过此 trait 统一调用
#[async_trait]
pub trait LlmDriver: Send + Sync {
    /// 供应商名称标识
    fn provider_name(&self) -> &'static str;

    /// 发送完成请求，返回流式事件流
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, uncode_core::error::UncodeError>;
}
