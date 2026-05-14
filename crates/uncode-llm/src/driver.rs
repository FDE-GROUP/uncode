use async_trait::async_trait;
use futures::stream::BoxStream;
use uncode_core::message::Message;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub tools: Vec<uncode_core::tool::ToolDefinition>,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
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
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[async_trait]
pub trait LlmDriver: Send + Sync {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, uncode_core::error::UncodeError>;

    fn provider_name(&self) -> &'static str;
}
