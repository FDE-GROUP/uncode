use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::api_types::{
    CompatConfig, InputModality, MaxTokensField, ThinkingFormat, ThinkingLevel,
};
use crate::config::UserModelConfig;

// ── 旧类型（Stage 7 清理前保留） ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub max_tokens: u32,
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub pricing: Option<ModelPricing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_1k: f64,
    pub output_per_1k: f64,
    pub cache_read_per_1k: Option<f64>,
}

// ── 新类型：API-first Model ──

/// 模型元数据（纯数据，不存密钥）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    pub base_url: String,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub thinking_format: Option<ThinkingFormat>,
    #[serde(default)]
    pub input_modalities: Vec<InputModality>,
    #[serde(default)]
    pub pricing: ModelPricingPerMillion,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub compat: CompatConfig,
    #[serde(default)]
    pub thinking_level_map: HashMap<ThinkingLevel, Option<String>>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            api: "openai-completions".into(),
            provider: String::new(),
            base_url: String::new(),
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: false,
            thinking_format: None,
            input_modalities: vec![InputModality::Text],
            pricing: ModelPricingPerMillion::default(),
            headers: HashMap::new(),
            compat: CompatConfig::default(),
            thinking_level_map: HashMap::new(),
        }
    }
}

impl Model {
    pub fn from_user_config(uc: &UserModelConfig) -> Self {
        let mut compat = CompatConfig::default();
        if let Some(ref uc_compat) = uc.compat {
            if let Some(v) = uc_compat.supports_developer_role {
                compat.supports_developer_role = v;
            }
            if let Some(v) = uc_compat.supports_usage_in_streaming {
                compat.supports_usage_in_streaming = v;
            }
            if let Some(v) = uc_compat.done_breaks_stream {
                compat.done_breaks_stream = v;
            }
            if let Some(ref tf) = uc_compat.thinking_format {
                compat.thinking_format = match tf.as_str() {
                    "deepseek" => Some(ThinkingFormat::DeepSeek),
                    "anthropic" => Some(ThinkingFormat::Anthropic),
                    "openai" => Some(ThinkingFormat::OpenAi),
                    "gemini" => Some(ThinkingFormat::Gemini),
                    "openrouter" => Some(ThinkingFormat::OpenRouter),
                    "together" => Some(ThinkingFormat::Together),
                    "zai" => Some(ThinkingFormat::ZAi),
                    "qwen" => Some(ThinkingFormat::Qwen),
                    "qwen_chat_template" => Some(ThinkingFormat::QwenChatTemplate),
                    _ => None,
                };
            }
            if let Some(ref mf) = uc_compat.max_tokens_field {
                compat.max_tokens_field = if mf == "max_completion_tokens" {
                    MaxTokensField::MaxCompletionTokens
                } else {
                    MaxTokensField::MaxTokens
                };
            }
            if let Some(v) = uc_compat.send_session_affinity_headers {
                compat.send_session_affinity_headers = v;
            }
            if let Some(v) = uc_compat.supports_long_cache_retention {
                compat.supports_long_cache_retention = v;
            }
            if let Some(v) = uc_compat.supports_store {
                compat.supports_store = v;
            }
            if let Some(v) = uc_compat.requires_reasoning_content_on_assistant_messages {
                compat.requires_reasoning_content_on_assistant_messages = v;
            }
            if let Some(v) = uc_compat.supports_eager_tool_input_streaming {
                compat.supports_eager_tool_input_streaming = v;
            }
            if let Some(v) = uc_compat.supports_cache_control_on_tools {
                compat.supports_cache_control_on_tools = v;
            }
        }
        Self {
            id: uc.id.clone(),
            name: uc.id.clone(),
            api: uc.api.clone(),
            provider: uc.provider.clone(),
            base_url: uc.base_url.clone().unwrap_or_default(),
            context_window: uc.context_window.unwrap_or(128_000),
            max_output_tokens: uc.max_output_tokens.unwrap_or(8192),
            compat,
            ..Model::default()
        }
    }
}

/// 每百万 token 定价（USD）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelPricingPerMillion {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
}

/// 将请求的 thinking level 降级到模型实际支持的最近级别。
///
/// 逻辑：查 `model.thinking_level_map`，如果请求的级别存在则直接返回；
/// 否则按 Off < Minimal < Low < Medium < High < XHigh 顺序向下寻找最近的可用级别。
/// 空 map 意味着模型不支持 thinking，返回 Off。
pub fn clamp_thinking_level(requested: ThinkingLevel, model: &Model) -> ThinkingLevel {
    const LEVELS: [ThinkingLevel; 6] = [
        ThinkingLevel::Off,
        ThinkingLevel::Minimal,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
        ThinkingLevel::XHigh,
    ];

    // Off 始终可用
    if requested == ThinkingLevel::Off {
        return ThinkingLevel::Off;
    }

    // 空 map → 不支持 thinking
    if model.thinking_level_map.is_empty() {
        return ThinkingLevel::Off;
    }

    // 精确匹配
    if model.thinking_level_map.contains_key(&requested) {
        return requested;
    }

    // 向下降级：找到请求级别在 LEVELS 中的位置，向下搜索
    let req_idx = LEVELS.iter().position(|&l| l == requested).unwrap_or(0);
    for &level in LEVELS.iter().rev().skip(LEVELS.len() - req_idx) {
        if model.thinking_level_map.contains_key(&level) {
            return level;
        }
    }

    ThinkingLevel::Off
}

fn default_context_window() -> u32 {
    128_000
}

fn default_max_output_tokens() -> u32 {
    8192
}

/// 内置模型数据集
pub fn builtin_models() -> Vec<Model> {
    vec![
        // ── DeepSeek ──
        Model {
            id: "deepseek-chat".into(),
            name: "DeepSeek V3".into(),
            api: "openai-completions".into(),
            provider: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: true,
            thinking_format: Some(ThinkingFormat::DeepSeek),
            input_modalities: vec![InputModality::Text],
            pricing: ModelPricingPerMillion {
                input: 0.27,
                output: 1.10,
                cache_read: 0.07,
                cache_write: 0.27,
            },
            compat: CompatConfig {
                supports_developer_role: false,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::from([
                (ThinkingLevel::Minimal, None),
                (ThinkingLevel::Low, None),
                (ThinkingLevel::Medium, None),
                (ThinkingLevel::High, Some("high".into())),
                (ThinkingLevel::XHigh, Some("max".into())),
            ]),
            ..Model::default()
        },
        Model {
            id: "deepseek-reasoner".into(),
            name: "DeepSeek R1".into(),
            api: "openai-completions".into(),
            provider: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: true,
            thinking_format: Some(ThinkingFormat::DeepSeek),
            input_modalities: vec![InputModality::Text],
            pricing: ModelPricingPerMillion {
                input: 0.55,
                output: 2.19,
                cache_read: 0.14,
                cache_write: 0.55,
            },
            compat: CompatConfig {
                supports_developer_role: false,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::from([
                (ThinkingLevel::Minimal, None),
                (ThinkingLevel::Low, None),
                (ThinkingLevel::Medium, None),
                (ThinkingLevel::High, Some("high".into())),
                (ThinkingLevel::XHigh, Some("max".into())),
            ]),
            ..Model::default()
        },
        // ── GLM (智谱) ──
        Model {
            id: "glm-4-flash".into(),
            name: "GLM-4 Flash".into(),
            api: "openai-completions".into(),
            provider: "glm".into(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            context_window: 128_000,
            max_output_tokens: 4096,
            input_modalities: vec![InputModality::Text],
            compat: CompatConfig {
                done_breaks_stream: true,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
        // ── OpenAI ──
        Model {
            id: "gpt-4o-mini".into(),
            name: "GPT-4o Mini".into(),
            api: "openai-completions".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            context_window: 128_000,
            max_output_tokens: 16_384,
            input_modalities: vec![InputModality::Text, InputModality::Image],
            pricing: ModelPricingPerMillion {
                input: 0.15,
                output: 0.60,
                ..Default::default()
            },
            compat: CompatConfig {
                send_session_affinity_headers: true,
                supports_long_cache_retention: true,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
        Model {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            api: "openai-completions".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            context_window: 128_000,
            max_output_tokens: 16_384,
            input_modalities: vec![InputModality::Text, InputModality::Image],
            pricing: ModelPricingPerMillion {
                input: 2.50,
                output: 10.00,
                ..Default::default()
            },
            compat: CompatConfig {
                send_session_affinity_headers: true,
                supports_long_cache_retention: true,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
        // ── Anthropic ──
        Model {
            id: "claude-sonnet-4-6".into(),
            name: "Claude Sonnet 4.6".into(),
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            context_window: 200_000,
            max_output_tokens: 16_384,
            reasoning: true,
            thinking_format: Some(ThinkingFormat::Anthropic),
            input_modalities: vec![InputModality::Text, InputModality::Image],
            pricing: ModelPricingPerMillion {
                input: 3.00,
                output: 15.00,
                cache_read: 0.30,
                cache_write: 3.75,
            },
            compat: CompatConfig {
                send_session_affinity_headers: true,
                supports_long_cache_retention: true,
                supports_cache_control_on_tools: true,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
        // ── Gemini ──
        Model {
            id: "gemini-2.0-flash".into(),
            name: "Gemini 2.0 Flash".into(),
            api: "google-generative-ai".into(),
            provider: "gemini".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            context_window: 1_048_576,
            max_output_tokens: 8192,
            input_modalities: vec![InputModality::Text, InputModality::Image],
            pricing: ModelPricingPerMillion::default(),
            ..Model::default()
        },
        // ── OpenRouter ──
        Model {
            id: "openrouter-auto".into(),
            name: "OpenRouter Auto".into(),
            api: "openai-completions".into(),
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            headers: HashMap::from([(
                "HTTP-Referer".into(),
                "https://github.com/FDE-GROUP/uncode".into(),
            )]),
            ..Model::default()
        },
        // ── Ollama (native) ──
        Model {
            id: "ollama".into(),
            name: "Ollama (local)".into(),
            api: "ollama-native".into(),
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            ..Model::default()
        },
        // ── Groq ──
        Model {
            id: "llama-3.3-70b-versatile".into(),
            name: "Llama 3.3 70B (Groq)".into(),
            api: "openai-completions".into(),
            provider: "groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            compat: CompatConfig {
                max_tokens_field: crate::api_types::MaxTokensField::MaxCompletionTokens,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
        // ── Cerebras ──
        Model {
            id: "llama-3.3-70b".into(),
            name: "Llama 3.3 70B (Cerebras)".into(),
            api: "openai-completions".into(),
            provider: "cerebras".into(),
            base_url: "https://api.cerebras.ai/v1".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            compat: CompatConfig {
                max_tokens_field: crate::api_types::MaxTokensField::MaxCompletionTokens,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
        // ── Mistral ──
        Model {
            id: "mistral-large-latest".into(),
            name: "Mistral Large".into(),
            api: "openai-completions".into(),
            provider: "mistral".into(),
            base_url: "https://api.mistral.ai/v1".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            compat: CompatConfig {
                supports_developer_role: false,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
        // ── xAI ──
        Model {
            id: "grok-3-mini".into(),
            name: "Grok 3 Mini".into(),
            api: "openai-completions".into(),
            provider: "xai".into(),
            base_url: "https://api.x.ai/v1".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: true,
            thinking_format: Some(ThinkingFormat::OpenAi),
            compat: CompatConfig {
                max_tokens_field: crate::api_types::MaxTokensField::MaxCompletionTokens,
                ..CompatConfig::default()
            },
            ..Model::default()
        },
    ]
}
