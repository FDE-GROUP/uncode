//! uncode-llm — LLM 供应商抽象层
//!
//! 提供统一的 `LlmDriver` trait 和 7 个供应商的具体实现。
//! 通过 `ProviderRegistry` 实现运行时多供应商注册和切换。

pub mod driver;
pub mod providers;
pub mod registry;

pub use driver::{CompletionRequest, LlmDriver, StreamEvent, UsageInfo};
pub use providers::anthropic::AnthropicDriver;
pub use providers::deepseek::DeepSeekDriver;
pub use providers::gemini::GeminiDriver;
pub use providers::glm::GlmDriver;
pub use providers::ollama::OllamaDriver;
pub use providers::openai::OpenAiDriver;
pub use providers::openrouter::OpenRouterDriver;
pub use registry::ProviderRegistry;
