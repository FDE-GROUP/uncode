//! uncode-ai — LLM 抽象层
//!
//! 对应 Pi 的 `pi-ai` 包。包含消息类型、模型定义、API 协议抽象和 4 个 provider 实现。

pub mod api;
pub mod api_registry;
pub mod api_types;
pub mod message;
pub mod model;
pub mod model_registry;
pub mod providers;
pub mod tool_def;

pub use api::{Api, StreamEvent, ToolCallEndData, UsageInfo as LlmUsageInfo};
pub use api_registry::ApiRegistry;
pub use model_registry::ModelRegistry;
pub use providers::anthropic_messages::AnthropicMessagesApi;
pub use providers::gemini_generative::GeminiGenerativeAiApi;
pub use providers::ollama_native::OllamaNativeApi;
pub use providers::openai_completions::OpenAiCompletionsApi;

use futures::stream::BoxStream;
use uncode_shared::error::UncodeError;

/// 通过 ApiRegistry 路由到对应 API 实现的流式补全
pub async fn stream(
    model: &model::Model,
    context: &api_types::Context,
    options: &api_types::StreamOptions,
    api_registry: &ApiRegistry,
) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
    let api = api_registry
        .get(&model.api)
        .ok_or_else(|| UncodeError::Config(format!("no API registered for '{}'", model.api)))?;
    api.stream(model, context, options).await
}

/// 通过 ApiRegistry 路由到对应 API 实现的非流式补全
pub async fn complete(
    model: &model::Model,
    context: &api_types::Context,
    options: &api_types::StreamOptions,
    api_registry: &ApiRegistry,
) -> Result<message::Message, UncodeError> {
    let api = api_registry
        .get(&model.api)
        .ok_or_else(|| UncodeError::Config(format!("no API registered for '{}'", model.api)))?;
    api.complete(model, context, options).await
}

#[cfg(test)]
mod tests;
