//! 厂商级 Compat 预设，对齐 Pi `OpenAICompletionsCompat` / vendor 表。
//!
//! 内置 `Model` 只声明模型级 delta；注册时通过 [`apply_provider_preset`] 合并完整 Compat。

use std::collections::HashMap;

use crate::api_types::{CompatConfig, MaxTokensField, ThinkingFormat, ThinkingLevel};
use crate::model::Model;

/// 与 `Model.provider` 字段对齐的厂商预设。
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub default_api: &'static str,
    pub default_base_url: &'static str,
    pub compat: CompatConfig,
    pub thinking_level_map: HashMap<ThinkingLevel, Option<String>>,
}

pub fn provider_preset(provider: &str) -> Option<ProviderPreset> {
    builtin_provider_presets()
        .into_iter()
        .find(|p| p.id == provider)
}

pub fn builtin_provider_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "deepseek",
            default_api: "openai-completions",
            default_base_url: "https://api.deepseek.com/v1",
            compat: CompatConfig {
                supports_developer_role: false,
                thinking_format: Some(ThinkingFormat::DeepSeek),
                supports_reasoning_effort: true,
                supports_usage_in_streaming: true,
                supports_user_id: true,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::from([
                (ThinkingLevel::Minimal, None),
                (ThinkingLevel::Low, None),
                (ThinkingLevel::Medium, None),
                (ThinkingLevel::High, Some("high".into())),
                (ThinkingLevel::XHigh, Some("max".into())),
            ]),
        },
        ProviderPreset {
            id: "glm",
            default_api: "openai-completions",
            default_base_url: "https://open.bigmodel.cn/api/paas/v4",
            compat: CompatConfig {
                supports_developer_role: true,
                done_breaks_stream: true,
                thinking_format: Some(ThinkingFormat::DeepSeek),
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::from([
                (ThinkingLevel::Minimal, Some("low".into())),
                (ThinkingLevel::Low, Some("low".into())),
                (ThinkingLevel::Medium, Some("medium".into())),
                (ThinkingLevel::High, Some("high".into())),
                (ThinkingLevel::XHigh, Some("high".into())),
            ]),
        },
        ProviderPreset {
            id: "openai",
            default_api: "openai-completions",
            default_base_url: "https://api.openai.com/v1",
            compat: CompatConfig {
                supports_developer_role: true,
                send_session_affinity_headers: true,
                supports_long_cache_retention: true,
                supports_usage_in_streaming: true,
                supports_user_id: true,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::new(),
        },
        ProviderPreset {
            id: "anthropic",
            default_api: "anthropic-messages",
            default_base_url: "https://api.anthropic.com/v1",
            compat: CompatConfig {
                supports_developer_role: true,
                thinking_format: Some(ThinkingFormat::Anthropic),
                send_session_affinity_headers: true,
                supports_long_cache_retention: true,
                supports_cache_control_on_tools: true,
                ..CompatConfig::default()
            },
            // Values are `budget_tokens` for extended thinking (see Anthropic Messages API).
            thinking_level_map: HashMap::from([
                (ThinkingLevel::Minimal, Some("1024".into())),
                (ThinkingLevel::Low, Some("4096".into())),
                (ThinkingLevel::Medium, Some("8000".into())),
                (ThinkingLevel::High, Some("16000".into())),
                (ThinkingLevel::XHigh, Some("32000".into())),
            ]),
        },
        ProviderPreset {
            id: "gemini",
            default_api: "google-generative-ai",
            default_base_url: "https://generativelanguage.googleapis.com/v1beta",
            compat: CompatConfig::default(),
            thinking_level_map: HashMap::new(),
        },
        ProviderPreset {
            id: "openrouter",
            default_api: "openai-completions",
            default_base_url: "https://openrouter.ai/api/v1",
            compat: CompatConfig {
                thinking_format: Some(ThinkingFormat::OpenRouter),
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::new(),
        },
        ProviderPreset {
            id: "groq",
            default_api: "openai-completions",
            default_base_url: "https://api.groq.com/openai/v1",
            compat: CompatConfig {
                max_tokens_field: MaxTokensField::MaxCompletionTokens,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::new(),
        },
        ProviderPreset {
            id: "cerebras",
            default_api: "openai-completions",
            default_base_url: "https://api.cerebras.ai/v1",
            compat: CompatConfig {
                max_tokens_field: MaxTokensField::MaxCompletionTokens,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::new(),
        },
        ProviderPreset {
            id: "mistral",
            default_api: "openai-completions",
            default_base_url: "https://api.mistral.ai/v1",
            compat: CompatConfig {
                supports_developer_role: false,
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::new(),
        },
        ProviderPreset {
            id: "xai",
            default_api: "openai-completions",
            default_base_url: "https://api.x.ai/v1",
            compat: CompatConfig {
                max_tokens_field: MaxTokensField::MaxCompletionTokens,
                thinking_format: Some(ThinkingFormat::OpenAi),
                ..CompatConfig::default()
            },
            thinking_level_map: HashMap::new(),
        },
        ProviderPreset {
            id: "ollama",
            default_api: "ollama-native",
            default_base_url: "http://localhost:11434",
            compat: CompatConfig::default(),
            thinking_level_map: HashMap::new(),
        },
    ]
}

/// 将厂商预设合并进模型（注册表加载时调用）。
pub fn apply_provider_preset(mut model: Model) -> Model {
    let Some(preset) = provider_preset(&model.provider) else {
        return model;
    };

    if model.base_url.is_empty() {
        model.base_url = preset.default_base_url.to_string();
    }
    if model.api.is_empty() {
        model.api = preset.default_api.to_string();
    }

    model.compat = CompatConfig::merge_with_overlay(&preset.compat, &model.compat);

    if model.thinking_level_map.is_empty() {
        model.thinking_level_map = preset.thinking_level_map.clone();
    }

    model.thinking_format = model
        .thinking_format
        .or(model.compat.thinking_format)
        .or(preset.compat.thinking_format);

    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::{MaxTokensField, ThinkingFormat};

    #[test]
    fn deepseek_preset() {
        let p = provider_preset("deepseek").unwrap();
        assert_eq!(p.default_api, "openai-completions");
        assert_eq!(p.default_base_url, "https://api.deepseek.com/v1");
        assert_eq!(p.compat.thinking_format, Some(ThinkingFormat::DeepSeek));
        assert!(!p.compat.supports_developer_role);
    }

    #[test]
    fn openai_preset() {
        let p = provider_preset("openai").unwrap();
        assert_eq!(p.default_api, "openai-completions");
        assert_eq!(p.default_base_url, "https://api.openai.com/v1");
        assert!(p.compat.supports_developer_role);
    }

    #[test]
    fn anthropic_preset() {
        let p = provider_preset("anthropic").unwrap();
        assert_eq!(p.default_api, "anthropic-messages");
        assert_eq!(p.default_base_url, "https://api.anthropic.com/v1");
        assert_eq!(p.compat.thinking_format, Some(ThinkingFormat::Anthropic));
        assert!(p.compat.supports_developer_role);
        assert!(p.compat.supports_cache_control_on_tools);
    }

    #[test]
    fn gemini_preset() {
        let p = provider_preset("gemini").unwrap();
        assert_eq!(p.default_api, "google-generative-ai");
        assert_eq!(
            p.default_base_url,
            "https://generativelanguage.googleapis.com/v1beta"
        );
    }

    #[test]
    fn ollama_preset() {
        let p = provider_preset("ollama").unwrap();
        assert_eq!(p.default_api, "ollama-native");
        assert_eq!(p.default_base_url, "http://localhost:11434");
    }

    #[test]
    fn openrouter_preset() {
        let p = provider_preset("openrouter").unwrap();
        assert_eq!(p.default_api, "openai-completions");
        assert_eq!(p.default_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(p.compat.thinking_format, Some(ThinkingFormat::OpenRouter));
    }

    #[test]
    fn groq_preset() {
        let p = provider_preset("groq").unwrap();
        assert_eq!(p.default_api, "openai-completions");
        assert_eq!(p.default_base_url, "https://api.groq.com/openai/v1");
        assert_eq!(
            p.compat.max_tokens_field,
            MaxTokensField::MaxCompletionTokens
        );
    }

    #[test]
    fn cerebras_preset() {
        let p = provider_preset("cerebras").unwrap();
        assert_eq!(p.default_base_url, "https://api.cerebras.ai/v1");
        assert_eq!(
            p.compat.max_tokens_field,
            MaxTokensField::MaxCompletionTokens
        );
    }

    #[test]
    fn mistral_preset() {
        let p = provider_preset("mistral").unwrap();
        assert_eq!(p.default_base_url, "https://api.mistral.ai/v1");
        assert!(!p.compat.supports_developer_role);
    }

    #[test]
    fn xai_preset() {
        let p = provider_preset("xai").unwrap();
        assert_eq!(p.default_base_url, "https://api.x.ai/v1");
        assert_eq!(
            p.compat.max_tokens_field,
            MaxTokensField::MaxCompletionTokens
        );
        assert_eq!(p.compat.thinking_format, Some(ThinkingFormat::OpenAi));
    }

    #[test]
    fn glm_preset() {
        let p = provider_preset("glm").unwrap();
        assert_eq!(p.default_base_url, "https://open.bigmodel.cn/api/paas/v4");
        assert!(p.compat.supports_developer_role);
        assert!(p.compat.done_breaks_stream);
        assert_eq!(p.compat.thinking_format, Some(ThinkingFormat::DeepSeek));
    }

    #[test]
    fn unknown_provider_returns_none() {
        assert!(provider_preset("nonexistent").is_none());
    }

    #[test]
    fn builtin_provider_presets_returns_all() {
        let presets = builtin_provider_presets();
        let ids: Vec<&str> = presets.iter().map(|p| p.id).collect();
        assert_eq!(presets.len(), 11);
        assert!(ids.contains(&"deepseek"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"gemini"));
        assert!(ids.contains(&"ollama"));
        assert!(ids.contains(&"openrouter"));
        assert!(ids.contains(&"groq"));
        assert!(ids.contains(&"cerebras"));
        assert!(ids.contains(&"mistral"));
        assert!(ids.contains(&"xai"));
        assert!(ids.contains(&"glm"));
    }

    #[test]
    fn apply_provider_preset_known_provider() {
        let model = Model {
            id: "test".into(),
            provider: "deepseek".into(),
            api: String::new(),
            base_url: String::new(),
            ..Model::default()
        };
        let result = apply_provider_preset(model);
        assert_eq!(result.api, "openai-completions");
        assert_eq!(result.base_url, "https://api.deepseek.com/v1");
    }

    #[test]
    fn apply_provider_preset_unknown_provider() {
        let model = Model {
            id: "test".into(),
            provider: "unknown".into(),
            ..Model::default()
        };
        let result = apply_provider_preset(model);
        assert_eq!(result.api, "openai-completions");
    }
}
