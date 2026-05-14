use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub providers: ProviderConfigs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            providers: ProviderConfigs {
                glm: None,
                deepseek: None,
                ollama: None,
                openrouter: None,
                openai: None,
                anthropic: None,
                gemini: None,
            },
        }
    }
}

fn default_base_url() -> Option<String> {
    None
}

fn default_ollama_url() -> String {
    "http://localhost:11434".into()
}
