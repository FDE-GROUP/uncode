//! uncode-llm — LLM 供应商抽象层
//!
//! 提供统一的 `LlmDriver` trait 和 7 个供应商的具体实现。
//! 通过 `ProviderRegistry` 实现运行时多供应商注册和切换。

// ── 新 API-first 架构（Stage 2+） ──
pub mod api;
pub mod api_registry;
pub mod model_registry;

// ── 旧 Driver-first 架构（Stage 7 清理） ──
pub mod builder;
pub mod driver;
pub mod providers;
pub mod registry;

pub use api::Api;
pub use api_registry::ApiRegistry;
pub use builder::CompletionRequestBuilder;
pub use driver::{CompletionRequest, LlmDriver, StreamEvent, UsageInfo};
pub use model_registry::ModelRegistry;
pub use providers::anthropic::AnthropicDriver;
pub use providers::anthropic_messages::AnthropicMessagesApi;
pub use providers::deepseek::DeepSeekDriver;
pub use providers::gemini::GeminiDriver;
pub use providers::gemini_generative::GeminiGenerativeAiApi;
pub use providers::glm::GlmDriver;
pub use providers::ollama::OllamaDriver;
pub use providers::ollama_native::OllamaNativeApi;
pub use providers::openai::OpenAiDriver;
pub use providers::openai_completions::OpenAiCompletionsApi;
pub use providers::openrouter::OpenRouterDriver;
pub use registry::ProviderRegistry;

// ── 顶层入口函数（Stage 5） ──

use futures::stream::BoxStream;
use uncode_core::error::UncodeError;
use uncode_core::message::Message as CoreMessage;

/// 通过 ApiRegistry 路由到对应 API 实现的流式补全
pub async fn stream(
    model: &uncode_core::model::Model,
    context: &uncode_core::api_types::Context,
    options: &uncode_core::api_types::StreamOptions,
    api_registry: &ApiRegistry,
) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
    let api = api_registry
        .get(&model.api)
        .ok_or_else(|| UncodeError::Config(format!("no API registered for '{}'", model.api)))?;
    api.stream(model, context, options).await
}

/// 通过 ApiRegistry 路由到对应 API 实现的非流式补全
pub async fn complete(
    model: &uncode_core::model::Model,
    context: &uncode_core::api_types::Context,
    options: &uncode_core::api_types::StreamOptions,
    api_registry: &ApiRegistry,
) -> Result<CoreMessage, UncodeError> {
    let api = api_registry
        .get(&model.api)
        .ok_or_else(|| UncodeError::Config(format!("no API registered for '{}'", model.api)))?;
    api.complete(model, context, options).await
}

#[cfg(test)]
mod tests;
