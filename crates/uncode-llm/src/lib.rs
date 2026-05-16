//! uncode-llm — LLM 供应商抽象层（API-first 架构）
//!
//! 提供 4 个 API 协议实现，通过 `ApiRegistry` + `ModelRegistry` 实现运行时路由。
//! 新增供应商只需声明 `Model` 数据 + `CompatConfig`，无需写驱动代码。

pub mod api;
pub mod api_registry;
pub mod model_registry;
pub mod providers;

pub use api::Api;
pub use api_registry::ApiRegistry;
pub use model_registry::ModelRegistry;
pub use providers::anthropic_messages::AnthropicMessagesApi;
pub use providers::gemini_generative::GeminiGenerativeAiApi;
pub use providers::ollama_native::OllamaNativeApi;
pub use providers::openai_completions::OpenAiCompletionsApi;

pub use api::{StreamEvent, UsageInfo};

// ── 顶层入口函数 ──

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
