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

#[cfg(test)]
mod tests;
