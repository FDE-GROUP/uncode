use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub providers: ProviderConfigs,
    #[serde(default = "default_models")]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub user_models: Vec<UserModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    #[serde(default = "default_model_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default)]
    pub supports_tools: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigs {
    pub glm: Option<ProviderConfig>,
    pub deepseek: Option<ProviderConfig>,
    pub ollama: Option<OllamaConfig>,
    pub openrouter: Option<ProviderConfig>,
    pub openai: Option<ProviderConfig>,
    pub anthropic: Option<ProviderConfig>,
    pub gemini: Option<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    #[serde(default = "default_base_url")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub host: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model: "deepseek-v3".into(),
            max_tokens: 8192,
            temperature: 0.7,
            providers: ProviderConfigs::default(),
            models: default_models(),
            user_models: vec![],
        }
    }
}

fn default_max_tokens() -> u32 {
    8192
}

fn default_temperature() -> f32 {
    0.7
}

fn default_base_url() -> Option<String> {
    None
}

fn default_ollama_url() -> String {
    "http://localhost:11434".into()
}

fn default_models() -> Vec<ModelConfig> {
    vec![
        ModelConfig {
            id: "deepseek-v3".into(),
            provider: "deepseek".into(),
            display_name: "DeepSeek V3".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            id: "deepseek-v4-pro".into(),
            provider: "deepseek".into(),
            display_name: "DeepSeek V4 Pro".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            id: "glm-5.1".into(),
            provider: "glm".into(),
            display_name: "GLM 5.1".into(),
            max_tokens: 128_000,
            supports_vision: true,
            supports_tools: true,
        },
        ModelConfig {
            id: "ollama".into(),
            provider: "ollama".into(),
            display_name: "Ollama (local)".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
        },
    ]
}

fn default_model_max_tokens() -> u32 {
    128_000
}

// ── 用户自定义模型配置（Stage 6） ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModelConfig {
    pub id: String,
    #[serde(default = "default_user_api")]
    pub api: String,
    pub provider: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub compat: Option<UserCompatConfig>,
}

fn default_user_api() -> String {
    "openai-completions".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserCompatConfig {
    #[serde(default)]
    pub supports_developer_role: Option<bool>,
    #[serde(default)]
    pub supports_usage_in_streaming: Option<bool>,
    #[serde(default)]
    pub done_breaks_stream: Option<bool>,
    #[serde(default)]
    pub thinking_format: Option<String>,
    #[serde(default)]
    pub max_tokens_field: Option<String>,
}
