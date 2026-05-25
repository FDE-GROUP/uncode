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
}

impl<'a> From<&'a UserModelConfig> for Model {
    fn from(uc: &'a UserModelConfig) -> Self {
        Self::from_user_config(uc)
    }
}

impl Model {
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

impl<'a> From<&'a ModelConfig> for Model {
    fn from(mc: &'a ModelConfig) -> Self {
        Self::from_model_config(mc)
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
    LEVELS
        .iter()
        .rev()
        .skip(LEVELS.len().saturating_sub(req_idx))
        .copied()
        .find(|level| model.thinking_level_map.contains_key(level))
        .unwrap_or(ThinkingLevel::Off)
}

fn default_context_window() -> u32 {
    128_000
}

fn default_max_output_tokens() -> u32 {
    8192
}

/// 内置模型表的一行：字段分组，避免 `builtin_model` 参数过长（Clippy / 可读性）。
struct BuiltinModelSpec {
    id: &'static str,
    name: &'static str,
    provider: &'static str,
    context_window: u32,
    max_output_tokens: u32,
    reasoning: bool,
    pricing: ModelPricingPerMillion,
    input_modalities: Vec<InputModality>,
    headers: HashMap<String, String>,
}

/// 内置模型声明：仅写模型级 delta；`api`/`base_url`/Compat/thinking 映射由 [`crate::provider_preset::apply_provider_preset`] 合并。
fn builtin_model(spec: BuiltinModelSpec) -> Model {
    Model {
        id: spec.id.into(),
        name: spec.name.into(),
        provider: spec.provider.into(),
        context_window: spec.context_window,
        max_output_tokens: spec.max_output_tokens,
        reasoning: spec.reasoning,
        pricing: spec.pricing,
        input_modalities: spec.input_modalities,
        headers: spec.headers,
        ..Model::default()
    }
}

/// 内置模型数据集（注册时经 `ModelRegistry::from_builtin` 套用厂商 preset）。
pub fn builtin_models() -> Vec<Model> {
    vec![
        builtin_model(BuiltinModelSpec {
            id: "deepseek-chat",
            name: "DeepSeek V3",
            provider: "deepseek",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: true,
            pricing: ModelPricingPerMillion {
                input: 0.27,
                output: 1.10,
                cache_read: 0.07,
                cache_write: 0.27,
            },
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "deepseek-v4-pro",
            name: "DeepSeek V4 Pro",
            provider: "deepseek",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: true,
            pricing: ModelPricingPerMillion {
                input: 0.27,
                output: 1.10,
                cache_read: 0.07,
                cache_write: 0.27,
            },
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "deepseek-reasoner",
            name: "DeepSeek R1",
            provider: "deepseek",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: true,
            pricing: ModelPricingPerMillion {
                input: 0.55,
                output: 2.19,
                cache_read: 0.14,
                cache_write: 0.55,
            },
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "glm-4-flash",
            name: "GLM-4 Flash",
            provider: "glm",
            context_window: 128_000,
            max_output_tokens: 4096,
            reasoning: false,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "glm-5.1",
            name: "GLM 5.1",
            provider: "glm",
            context_window: 200_000,
            max_output_tokens: 8192,
            reasoning: true,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text, InputModality::Image],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "gpt-4o-mini",
            name: "GPT-4o Mini",
            provider: "openai",
            context_window: 128_000,
            max_output_tokens: 16_384,
            reasoning: false,
            pricing: ModelPricingPerMillion {
                input: 0.15,
                output: 0.60,
                ..Default::default()
            },
            input_modalities: vec![InputModality::Text, InputModality::Image],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "gpt-4o",
            name: "GPT-4o",
            provider: "openai",
            context_window: 128_000,
            max_output_tokens: 16_384,
            reasoning: false,
            pricing: ModelPricingPerMillion {
                input: 2.50,
                output: 10.00,
                ..Default::default()
            },
            input_modalities: vec![InputModality::Text, InputModality::Image],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "claude-sonnet-4-6",
            name: "Claude Sonnet 4.6",
            provider: "anthropic",
            context_window: 200_000,
            max_output_tokens: 16_384,
            reasoning: true,
            pricing: ModelPricingPerMillion {
                input: 3.00,
                output: 15.00,
                cache_read: 0.30,
                cache_write: 3.75,
            },
            input_modalities: vec![InputModality::Text, InputModality::Image],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "gemini-2.0-flash",
            name: "Gemini 2.0 Flash",
            provider: "gemini",
            context_window: 1_048_576,
            max_output_tokens: 8192,
            reasoning: false,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text, InputModality::Image],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "openrouter-auto",
            name: "OpenRouter Auto",
            provider: "openrouter",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: false,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text],
            headers: HashMap::from([(
                "HTTP-Referer".into(),
                "https://github.com/FDE-GROUP/uncode".into(),
            )]),
        }),
        builtin_model(BuiltinModelSpec {
            id: "llama-3.3-70b-versatile",
            name: "Llama 3.3 70B (Groq)",
            provider: "groq",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: false,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "llama-3.3-70b",
            name: "Llama 3.3 70B (Cerebras)",
            provider: "cerebras",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: false,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "mistral-large-latest",
            name: "Mistral Large",
            provider: "mistral",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: false,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
        builtin_model(BuiltinModelSpec {
            id: "grok-3-mini",
            name: "Grok 3 Mini",
            provider: "xai",
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning: true,
            pricing: ModelPricingPerMillion::default(),
            input_modalities: vec![InputModality::Text],
            headers: HashMap::new(),
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::ThinkingFormat;
    use uncode_shared::config::UserCompatConfig;

    // ── Model ──
    #[test]
    fn model_default() {
        let m = Model::default();
        assert_eq!(m.id, "");
        assert_eq!(m.name, "");
        assert_eq!(m.api, "openai-completions");
        assert_eq!(m.provider, "");
        assert_eq!(m.base_url, "");
        assert_eq!(m.context_window, 128_000);
        assert_eq!(m.max_output_tokens, 8192);
        assert!(!m.reasoning);
        assert!(m.thinking_format.is_none());
        assert_eq!(m.input_modalities, vec![InputModality::Text]);
        assert_eq!(m.pricing.input, 0.0);
        assert_eq!(m.pricing.output, 0.0);
        assert_eq!(m.pricing.cache_read, 0.0);
        assert_eq!(m.pricing.cache_write, 0.0);
        assert!(m.headers.is_empty());
        assert!(m.compat.supports_usage_in_streaming);
        assert_eq!(m.compat.max_tokens_field, MaxTokensField::MaxTokens);
        assert!(m.thinking_level_map.is_empty());
    }

    #[test]
    fn model_construction() {
        let m = Model {
            id: "test-model".into(),
            name: "Test Model".into(),
            api: "test-api".into(),
            provider: "test-provider".into(),
            base_url: "https://test.com/v1".into(),
            context_window: 64000,
            max_output_tokens: 4096,
            reasoning: true,
            thinking_format: Some(ThinkingFormat::DeepSeek),
            input_modalities: vec![InputModality::Text, InputModality::Image],
            pricing: ModelPricingPerMillion {
                input: 1.0,
                output: 2.0,
                cache_read: 0.5,
                cache_write: 0.25,
            },
            headers: HashMap::from([("X-Custom".into(), "val".into())]),
            compat: CompatConfig {
                supports_developer_role: true,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::from([(ThinkingLevel::Medium, None)]),
        };
        assert_eq!(m.id, "test-model");
        assert!(m.reasoning);
        assert_eq!(m.thinking_format, Some(ThinkingFormat::DeepSeek));
        assert_eq!(m.pricing.input, 1.0);
        assert_eq!(m.pricing.cache_write, 0.25);
    }

    #[test]
    fn model_serde_roundtrip() {
        let m = Model {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            api: "openai-completions".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            context_window: 128000,
            max_output_tokens: 16384,
            reasoning: true,
            thinking_format: Some(ThinkingFormat::OpenAi),
            input_modalities: vec![InputModality::Text, InputModality::Image],
            pricing: ModelPricingPerMillion {
                input: 2.5,
                output: 10.0,
                ..Default::default()
            },
            headers: HashMap::new(),
            compat: CompatConfig::default(),
            thinking_level_map: HashMap::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let decoded: Model = serde_json::from_str(&json).unwrap();
        assert_eq!(m.id, decoded.id);
        assert_eq!(m.api, decoded.api);
        assert_eq!(m.context_window, decoded.context_window);
        assert_eq!(m.thinking_format, decoded.thinking_format);
        assert_eq!(m.input_modalities.len(), decoded.input_modalities.len());
    }

    // ── effective_compat ──
    #[test]
    fn effective_compat_with_known_provider() {
        let mut m = Model::default();
        m.provider = "deepseek".into();
        m.compat.supports_strict_mode = true;
        let compat = m.effective_compat();
        assert!(compat.supports_strict_mode);
        assert_eq!(compat.thinking_format, Some(ThinkingFormat::DeepSeek));
        assert!(!compat.supports_developer_role);
    }

    #[test]
    fn effective_compat_with_unknown_provider() {
        let mut m = Model::default();
        m.provider = "nonexistent".into();
        m.compat.supports_developer_role = true;
        let compat = m.effective_compat();
        assert!(compat.supports_developer_role);
    }

    // ── effective_thinking_format ──
    #[test]
    fn effective_thinking_format_from_compat() {
        let mut m = Model::default();
        m.provider = "deepseek".into();
        assert_eq!(
            m.effective_thinking_format(),
            Some(ThinkingFormat::DeepSeek)
        );
    }

    #[test]
    fn effective_thinking_format_from_model_when_compat_none() {
        let mut m = Model::default();
        m.provider = "openai".into();
        m.thinking_format = Some(ThinkingFormat::OpenAi);
        assert_eq!(m.effective_thinking_format(), Some(ThinkingFormat::OpenAi));
    }

    // ── clamp_thinking_level ──
    #[test]
    fn clamp_off_returns_off() {
        let model = Model::default();
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Off, &model),
            ThinkingLevel::Off
        );
    }

    #[test]
    fn clamp_empty_map_returns_off() {
        let model = Model::default();
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Medium, &model),
            ThinkingLevel::Off
        );
    }

    #[test]
    fn clamp_level_in_map_returns_same() {
        let mut model = Model::default();
        model.thinking_level_map.insert(ThinkingLevel::Medium, None);
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Medium, &model),
            ThinkingLevel::Medium
        );
    }

    #[test]
    fn clamp_level_above_max_clamps_down() {
        let mut model = Model::default();
        model.thinking_level_map.insert(ThinkingLevel::Low, None);
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::XHigh, &model),
            ThinkingLevel::Low
        );
    }

    #[test]
    fn clamp_level_between_available_clamps_down() {
        let mut model = Model::default();
        model
            .thinking_level_map
            .insert(ThinkingLevel::Minimal, None);
        model.thinking_level_map.insert(ThinkingLevel::High, None);
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Medium, &model),
            ThinkingLevel::Minimal
        );
    }

    #[test]
    fn clamp_map_only_off_returns_off_for_non_off() {
        let mut model = Model::default();
        model
            .thinking_level_map
            .insert(ThinkingLevel::Off, Some("off".into()));
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Medium, &model),
            ThinkingLevel::Off
        );
    }

    #[test]
    fn clamp_map_with_high_levels_medium_available() {
        let mut model = Model::default();
        model.thinking_level_map.insert(ThinkingLevel::Medium, None);
        model.thinking_level_map.insert(ThinkingLevel::High, None);
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Medium, &model),
            ThinkingLevel::Medium
        );
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::High, &model),
            ThinkingLevel::High
        );
    }

    // ── ModelPricingPerMillion ──
    #[test]
    fn model_pricing_per_million_default() {
        let p = ModelPricingPerMillion::default();
        assert_eq!(p.input, 0.0);
        assert_eq!(p.output, 0.0);
        assert_eq!(p.cache_read, 0.0);
        assert_eq!(p.cache_write, 0.0);
    }

    #[test]
    fn model_pricing_per_million_construction() {
        let p = ModelPricingPerMillion {
            input: 2.5,
            output: 10.0,
            cache_read: 0.3,
            cache_write: 3.75,
        };
        assert!((p.input - 2.5).abs() < f64::EPSILON);
        assert!((p.cache_write - 3.75).abs() < f64::EPSILON);
    }

    // ── ModelInfo + ModelPricing ──
    #[test]
    fn model_info_serde_roundtrip() {
        let info = ModelInfo {
            id: "test".into(),
            provider: "openai".into(),
            display_name: "Test".into(),
            max_tokens: 4096,
            supports_vision: true,
            supports_tools: true,
            pricing: Some(ModelPricing {
                input_per_1k: 0.01,
                output_per_1k: 0.03,
                cache_read_per_1k: Some(0.005),
            }),
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.id, decoded.id);
        assert_eq!(info.supports_vision, decoded.supports_vision);
        assert_eq!(
            decoded.pricing.as_ref().unwrap().cache_read_per_1k,
            Some(0.005)
        );
    }

    #[test]
    fn model_pricing_construction() {
        let p = ModelPricing {
            input_per_1k: 0.01,
            output_per_1k: 0.03,
            cache_read_per_1k: None,
        };
        assert!((p.input_per_1k - 0.01).abs() < f64::EPSILON);
        assert!(p.cache_read_per_1k.is_none());
    }

    // ── Model::from_user_config ──
    #[test]
    fn from_user_config_applies_compat_fields() {
        let uc = UserModelConfig {
            id: "my-model".into(),
            api: "openai-completions".into(),
            provider: "custom".into(),
            base_url: Some("https://custom.api/v1".into()),
            context_window: Some(64000),
            max_output_tokens: Some(4096),
            api_key: None,
            compat: Some(UserCompatConfig {
                supports_developer_role: Some(true),
                supports_usage_in_streaming: Some(false),
                ..Default::default()
            }),
        };
        let m = Model::from_user_config(&uc);
        assert_eq!(m.id, "my-model");
        assert_eq!(m.base_url, "https://custom.api/v1");
        assert_eq!(m.context_window, 64000);
        assert_eq!(m.max_output_tokens, 4096);
        assert!(m.compat.supports_developer_role);
        assert!(!m.compat.supports_usage_in_streaming);
    }

    #[test]
    fn from_user_config_thinking_format_deepseek() {
        let uc = UserModelConfig {
            id: "ds".into(),
            api: "openai-completions".into(),
            provider: "deepseek".into(),
            base_url: None,
            context_window: None,
            max_output_tokens: None,
            api_key: None,
            compat: Some(UserCompatConfig {
                thinking_format: Some("deepseek".into()),
                ..Default::default()
            }),
        };
        let m = Model::from_user_config(&uc);
        assert_eq!(m.compat.thinking_format, Some(ThinkingFormat::DeepSeek));
    }

    #[test]
    fn from_user_config_thinking_format_anthropic() {
        let uc = UserModelConfig {
            id: "cl".into(),
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            base_url: None,
            context_window: None,
            max_output_tokens: None,
            api_key: None,
            compat: Some(UserCompatConfig {
                thinking_format: Some("anthropic".into()),
                ..Default::default()
            }),
        };
        let m = Model::from_user_config(&uc);
        assert_eq!(m.compat.thinking_format, Some(ThinkingFormat::Anthropic));
    }

    #[test]
    fn from_user_config_thinking_format_gemini() {
        let uc = UserModelConfig {
            id: "gm".into(),
            api: "google-generative-ai".into(),
            provider: "gemini".into(),
            base_url: None,
            context_window: None,
            max_output_tokens: None,
            api_key: None,
            compat: Some(UserCompatConfig {
                thinking_format: Some("gemini".into()),
                ..Default::default()
            }),
        };
        let m = Model::from_user_config(&uc);
        assert_eq!(m.compat.thinking_format, Some(ThinkingFormat::Gemini));
    }

    #[test]
    fn from_user_config_thinking_format_openai() {
        let uc = UserModelConfig {
            id: "oa".into(),
            api: "openai-completions".into(),
            provider: "openai".into(),
            base_url: None,
            context_window: None,
            max_output_tokens: None,
            api_key: None,
            compat: Some(UserCompatConfig {
                thinking_format: Some("openai".into()),
                ..Default::default()
            }),
        };
        let m = Model::from_user_config(&uc);
        assert_eq!(m.compat.thinking_format, Some(ThinkingFormat::OpenAi));
    }

    #[test]
    fn from_user_config_max_tokens_field_max_completion_tokens() {
        let uc = UserModelConfig {
            id: "test".into(),
            api: "openai-completions".into(),
            provider: "groq".into(),
            base_url: None,
            context_window: None,
            max_output_tokens: None,
            api_key: None,
            compat: Some(UserCompatConfig {
                max_tokens_field: Some("max_completion_tokens".into()),
                ..Default::default()
            }),
        };
        let m = Model::from_user_config(&uc);
        assert_eq!(
            m.compat.max_tokens_field,
            MaxTokensField::MaxCompletionTokens
        );
    }

    #[test]
    fn from_user_config_max_tokens_field_default_to_max_tokens() {
        let uc = UserModelConfig {
            id: "test".into(),
            api: "openai-completions".into(),
            provider: "custom".into(),
            base_url: None,
            context_window: None,
            max_output_tokens: None,
            api_key: None,
            compat: Some(UserCompatConfig {
                max_tokens_field: Some("max_tokens".into()),
                ..Default::default()
            }),
        };
        let m = Model::from_user_config(&uc);
        assert_eq!(m.compat.max_tokens_field, MaxTokensField::MaxTokens);
    }
}
