use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::api_types::{
    CompatConfig, InputModality, MaxTokensField, ThinkingFormat, ThinkingLevel,
};
use uncode_shared::config::ModelConfig;
use uncode_shared::config::UserModelConfig;

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

/// 模型元数据（纯数据，不存密钥）。
///
/// **Pi:** 对应 `Model` 声明（api / provider / context_window / compat）；通过 `ModelRegistry` 接入供应商。
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
    /// 厂商 preset + 模型级 compat 覆盖后的有效配置。
    pub fn effective_compat(&self) -> CompatConfig {
        match crate::provider_preset::provider_preset(&self.provider) {
            Some(p) => CompatConfig::merge_with_overlay(&p.compat, &self.compat),
            None => self.compat.clone(),
        }
    }

    /// Request/SSE 使用有效 compat；内置模型可在 `thinking_format` 字段再声明一层。
    pub fn effective_thinking_format(&self) -> Option<ThinkingFormat> {
        self.effective_compat()
            .thinking_format
            .or(self.thinking_format)
    }

    /// 供 `stream_simple` 使用：钳制 thinking 并写入有效 compat。
    pub fn prepared_for_stream(&self, options: &mut crate::api_types::StreamOptions) -> Model {
        let mut model = self.clone();
        model.compat = self.effective_compat();
        model.thinking_format = model.effective_thinking_format();

        if let Some(level) = options.thinking_level {
            options.thinking_level = Some(clamp_thinking_level(level, &model));
        }

        model
    }

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
        let model = Self {
            id: uc.id.clone(),
            name: uc.id.clone(),
            api: uc.api.clone(),
            provider: uc.provider.clone(),
            base_url: uc.base_url.clone().unwrap_or_default(),
            context_window: uc.context_window.unwrap_or(128_000),
            max_output_tokens: uc.max_output_tokens.unwrap_or(8192),
            compat,
            ..Model::default()
        };
        crate::provider_preset::apply_provider_preset(model)
    }

    pub fn from_model_config(mc: &ModelConfig) -> Self {
        // If a builtin model with the same id exists, use it as base
        let builtin = builtin_models().into_iter().find(|m| m.id == mc.id);

        let model = if let Some(base) = builtin {
            // Use builtin model's full config (api, compat, pricing, etc.)
            let mut input_modalities = base.input_modalities.clone();
            if mc.supports_vision && !input_modalities.contains(&InputModality::Image) {
                input_modalities.push(InputModality::Image);
            }
            Self {
                name: mc.display_name.clone(),
                context_window: mc.max_tokens,
                input_modalities,
                ..base
            }
        } else {
            let (api, base_url) = provider_defaults(&mc.provider);
            let mut input_modalities = vec![InputModality::Text];
            if mc.supports_vision {
                input_modalities.push(InputModality::Image);
            }
            Self {
                id: mc.id.clone(),
                name: mc.display_name.clone(),
                api: api.to_string(),
                provider: mc.provider.clone(),
                base_url: base_url.to_string(),
                context_window: mc.max_tokens,
                input_modalities,
                ..Model::default()
            }
        };
        crate::provider_preset::apply_provider_preset(model)
    }
}

fn provider_defaults(provider: &str) -> (&'static str, &'static str) {
    match provider {
        "deepseek" => ("openai-completions", "https://api.deepseek.com/v1"),
        "openai" => ("openai-completions", "https://api.openai.com/v1"),
        "glm" => ("openai-completions", "https://open.bigmodel.cn/api/paas/v4"),
        "anthropic" => ("anthropic-messages", "https://api.anthropic.com/v1"),
        "gemini" => (
            "google-generative-ai",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        "ollama" => ("ollama-native", "http://localhost:11434"),
        "openrouter" => ("openai-completions", "https://openrouter.ai/api/v1"),
        "groq" => ("openai-completions", "https://api.groq.com/openai/v1"),
        "cerebras" => ("openai-completions", "https://api.cerebras.ai/v1"),
        "mistral" => ("openai-completions", "https://api.mistral.ai/v1"),
        "xai" => ("openai-completions", "https://api.x.ai/v1"),
        _ => ("openai-completions", ""),
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
pub fn clamp_thinking_level(requested: ThinkingLevel, model: &Model) -> ThinkingLevel {
    const LEVELS: [ThinkingLevel; 6] = [
        ThinkingLevel::Off,
        ThinkingLevel::Minimal,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
        ThinkingLevel::XHigh,
    ];

    if requested == ThinkingLevel::Off {
        return ThinkingLevel::Off;
    }

    if model.thinking_level_map.is_empty() {
        return ThinkingLevel::Off;
    }

    if model.thinking_level_map.contains_key(&requested) {
        return requested;
    }

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

/// 内置模型声明：仅写模型级 delta；`api`/`base_url`/Compat/thinking 映射由 [`crate::provider_preset::apply_provider_preset`] 合并。
fn builtin_model(
    id: &'static str,
    name: &'static str,
    provider: &'static str,
    context_window: u32,
    max_output_tokens: u32,
    reasoning: bool,
    pricing: ModelPricingPerMillion,
    input_modalities: Vec<InputModality>,
    headers: HashMap<String, String>,
) -> Model {
    Model {
        id: id.into(),
        name: name.into(),
        provider: provider.into(),
        context_window,
        max_output_tokens,
        reasoning,
        pricing,
        input_modalities,
        headers,
        ..Model::default()
    }
}

/// 内置模型数据集（注册时经 `ModelRegistry::from_builtin` 套用厂商 preset）。
pub fn builtin_models() -> Vec<Model> {
    vec![
        builtin_model(
            "deepseek-chat",
            "DeepSeek V3",
            "deepseek",
            128_000,
            8192,
            true,
            ModelPricingPerMillion {
                input: 0.27,
                output: 1.10,
                cache_read: 0.07,
                cache_write: 0.27,
            },
            vec![InputModality::Text],
            HashMap::new(),
        ),
        builtin_model(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            "deepseek",
            128_000,
            8192,
            true,
            ModelPricingPerMillion {
                input: 0.27,
                output: 1.10,
                cache_read: 0.07,
                cache_write: 0.27,
            },
            vec![InputModality::Text],
            HashMap::new(),
        ),
        builtin_model(
            "deepseek-reasoner",
            "DeepSeek R1",
            "deepseek",
            128_000,
            8192,
            true,
            ModelPricingPerMillion {
                input: 0.55,
                output: 2.19,
                cache_read: 0.14,
                cache_write: 0.55,
            },
            vec![InputModality::Text],
            HashMap::new(),
        ),
        builtin_model(
            "glm-4-flash",
            "GLM-4 Flash",
            "glm",
            128_000,
            4096,
            false,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text],
            HashMap::new(),
        ),
        builtin_model(
            "glm-5.1",
            "GLM 5.1",
            "glm",
            200_000,
            8192,
            true,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text, InputModality::Image],
            HashMap::new(),
        ),
        builtin_model(
            "gpt-4o-mini",
            "GPT-4o Mini",
            "openai",
            128_000,
            16_384,
            false,
            ModelPricingPerMillion {
                input: 0.15,
                output: 0.60,
                ..Default::default()
            },
            vec![InputModality::Text, InputModality::Image],
            HashMap::new(),
        ),
        builtin_model(
            "gpt-4o",
            "GPT-4o",
            "openai",
            128_000,
            16_384,
            false,
            ModelPricingPerMillion {
                input: 2.50,
                output: 10.00,
                ..Default::default()
            },
            vec![InputModality::Text, InputModality::Image],
            HashMap::new(),
        ),
        builtin_model(
            "claude-sonnet-4-6",
            "Claude Sonnet 4.6",
            "anthropic",
            200_000,
            16_384,
            true,
            ModelPricingPerMillion {
                input: 3.00,
                output: 15.00,
                cache_read: 0.30,
                cache_write: 3.75,
            },
            vec![InputModality::Text, InputModality::Image],
            HashMap::new(),
        ),
        builtin_model(
            "gemini-2.0-flash",
            "Gemini 2.0 Flash",
            "gemini",
            1_048_576,
            8192,
            false,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text, InputModality::Image],
            HashMap::new(),
        ),
        builtin_model(
            "openrouter-auto",
            "OpenRouter Auto",
            "openrouter",
            128_000,
            8192,
            false,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text],
            HashMap::from([(
                "HTTP-Referer".into(),
                "https://github.com/FDE-GROUP/uncode".into(),
            )]),
        ),
        builtin_model(
            "llama-3.3-70b-versatile",
            "Llama 3.3 70B (Groq)",
            "groq",
            128_000,
            8192,
            false,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text],
            HashMap::new(),
        ),
        builtin_model(
            "llama-3.3-70b",
            "Llama 3.3 70B (Cerebras)",
            "cerebras",
            128_000,
            8192,
            false,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text],
            HashMap::new(),
        ),
        builtin_model(
            "mistral-large-latest",
            "Mistral Large",
            "mistral",
            128_000,
            8192,
            false,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text],
            HashMap::new(),
        ),
        builtin_model(
            "grok-3-mini",
            "Grok 3 Mini",
            "xai",
            128_000,
            8192,
            true,
            ModelPricingPerMillion::default(),
            vec![InputModality::Text],
            HashMap::new(),
        ),
    ]
}
