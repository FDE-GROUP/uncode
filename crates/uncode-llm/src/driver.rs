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

#[async_trait]
pub trait LlmDriver: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, uncode_core::error::UncodeError>;
}

pub struct ChatMessage {
    pub role: String,
    pub content: String,
}
