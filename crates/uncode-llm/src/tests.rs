#[cfg(test)]
mod tests {
    use uncode_core::api_types::{CompatConfig, StopReason, ThinkingFormat, ThinkingLevel};
    use uncode_core::model::Model;

    // ── StopReason 映射测试 ──

    #[test]
    fn test_openai_map_finish_reason_stop() {
        use crate::providers::openai_completions::map_finish_reason;
        assert_eq!(map_finish_reason("stop"), StopReason::Stop);
        assert_eq!(map_finish_reason("end"), StopReason::Stop);
    }

    #[test]
    fn test_openai_map_finish_reason_length() {
        use crate::providers::openai_completions::map_finish_reason;
        assert_eq!(map_finish_reason("length"), StopReason::Length);
        assert_eq!(map_finish_reason("max_tokens"), StopReason::Length);
    }

    #[test]
    fn test_openai_map_finish_reason_tool_use() {
        use crate::providers::openai_completions::map_finish_reason;
        assert_eq!(map_finish_reason("tool_calls"), StopReason::ToolUse);
        assert_eq!(map_finish_reason("function_call"), StopReason::ToolUse);
    }

    #[test]
    fn test_openai_map_finish_reason_content_filter() {
        use crate::providers::openai_completions::map_finish_reason;
        assert_eq!(map_finish_reason("content_filter"), StopReason::Error);
    }

    #[test]
    fn test_openai_map_finish_reason_unknown_defaults_to_stop() {
        use crate::providers::openai_completions::map_finish_reason;
        assert_eq!(map_finish_reason("something_else"), StopReason::Stop);
    }

    // ── Gemini finishReason 映射测试 ──

    #[test]
    fn test_gemini_map_finish_reason() {
        use crate::providers::gemini_generative::map_gemini_finish_reason;
        assert_eq!(map_gemini_finish_reason("STOP"), StopReason::Stop);
        assert_eq!(
            map_gemini_finish_reason("FINISH_REASON_STOP"),
            StopReason::Stop
        );
        assert_eq!(map_gemini_finish_reason("MAX_TOKENS"), StopReason::Length);
        assert_eq!(
            map_gemini_finish_reason("FINISH_REASON_MAX_TOKENS"),
            StopReason::Length
        );
        assert_eq!(map_gemini_finish_reason("SAFETY"), StopReason::Error);
        assert_eq!(map_gemini_finish_reason("RECITATION"), StopReason::Error);
        assert_eq!(
            map_gemini_finish_reason("FINISH_REASON_SAFETY"),
            StopReason::Error
        );
        assert_eq!(
            map_gemini_finish_reason("FINISH_REASON_RECITATION"),
            StopReason::Error
        );
        assert_eq!(map_gemini_finish_reason("UNKNOWN"), StopReason::Stop);
    }

    // ── StreamEvent 测试 ──

    #[test]
    fn test_stream_event_done_carries_reason() {
        use crate::api::StreamEvent;
        let event = StreamEvent::Done {
            reason: StopReason::ToolUse,
        };
        match event {
            StreamEvent::Done { reason } => assert_eq!(reason, StopReason::ToolUse),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn test_stream_event_error_carries_reason() {
        use crate::api::StreamEvent;
        let event = StreamEvent::Error {
            reason: StopReason::Error,
            message: "timeout".into(),
        };
        match event {
            StreamEvent::Error { reason, message } => {
                assert_eq!(reason, StopReason::Error);
                assert_eq!(message, "timeout");
            }
            _ => panic!("expected Error"),
        }
    }

    // ── ApiRegistry 测试 ──

    #[test]
    fn test_api_registry_names() {
        use crate::api_registry::ApiRegistry;
        let reg = ApiRegistry::new();
        assert!(reg.names().is_empty());
        assert!(!reg.has("openai-completions"));
    }

    // ── ModelRegistry 测试 ──

    #[test]
    fn test_model_registry_from_builtin() {
        use crate::model_registry::ModelRegistry;
        let reg = ModelRegistry::from_builtin();
        assert!(reg.has("deepseek-chat"));
        assert!(reg.has("claude-sonnet-4-6"));
        assert!(reg.has("gpt-4o"));
        assert!(reg.has("gemini-2.0-flash"));
        assert!(reg.has("ollama"));
        assert!(!reg.has("nonexistent-model"));
    }

    #[test]
    fn test_model_registry_merge_user_models() {
        use crate::model_registry::ModelRegistry;
        let mut reg = ModelRegistry::from_builtin();
        let custom = Model {
            id: "my-custom".into(),
            name: "Custom".into(),
            api: "openai-completions".into(),
            base_url: "http://localhost:8080/v1".into(),
            ..Model::default()
        };
        reg.merge_user_models(vec![custom]);
        assert!(reg.has("my-custom"));
        let m = reg.get("my-custom").unwrap();
        assert_eq!(m.base_url, "http://localhost:8080/v1");
    }

    // ── ThinkingLevelMap 测试 ──

    #[test]
    fn test_deepseek_thinking_level_map() {
        let reg = crate::model_registry::ModelRegistry::from_builtin();
        let model = reg.get("deepseek-chat").unwrap();

        assert_eq!(
            model.thinking_level_map.get(&ThinkingLevel::High),
            Some(&Some("high".into()))
        );
        assert_eq!(
            model.thinking_level_map.get(&ThinkingLevel::XHigh),
            Some(&Some("max".into()))
        );
        assert_eq!(
            model.thinking_level_map.get(&ThinkingLevel::Minimal),
            Some(&None)
        );
        assert_eq!(
            model.thinking_level_map.get(&ThinkingLevel::Low),
            Some(&None)
        );
    }

    #[test]
    fn test_thinking_level_all_variants() {
        let all = [
            ThinkingLevel::Off,
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
            ThinkingLevel::XHigh,
        ];
        assert_eq!(all.len(), 6);
    }

    // ── CompatConfig 新字段测试 ──

    #[test]
    fn test_compat_config_new_fields_default_false() {
        let compat = CompatConfig::default();
        assert!(!compat.send_session_affinity_headers);
        assert!(!compat.supports_long_cache_retention);
        assert!(!compat.supports_store);
        assert!(!compat.requires_reasoning_content_on_assistant_messages);
        assert!(!compat.supports_eager_tool_input_streaming);
        assert!(!compat.supports_cache_control_on_tools);
    }

    #[test]
    fn test_gpt4o_session_affinity_enabled() {
        let reg = crate::model_registry::ModelRegistry::from_builtin();
        let model = reg.get("gpt-4o").unwrap();
        assert!(model.compat.send_session_affinity_headers);
        assert!(model.compat.supports_long_cache_retention);
    }

    #[test]
    fn test_claude_anthropic_compat_enabled() {
        let reg = crate::model_registry::ModelRegistry::from_builtin();
        let model = reg.get("claude-sonnet-4-6").unwrap();
        assert!(model.compat.send_session_affinity_headers);
        assert!(model.compat.supports_long_cache_retention);
        assert!(model.compat.supports_cache_control_on_tools);
        assert!(model.reasoning);
        assert_eq!(model.thinking_format, Some(ThinkingFormat::Anthropic));
    }

    // ── CacheRetention 测试 ──

    #[test]
    fn test_cache_retention_default() {
        use uncode_core::api_types::CacheRetention;
        assert_eq!(CacheRetention::default(), CacheRetention::Short);
    }

    #[test]
    fn test_cache_retention_serde() {
        use uncode_core::api_types::CacheRetention;
        let json = serde_json::to_string(&CacheRetention::Long).unwrap();
        assert_eq!(json, "\"long\"");
        let parsed: CacheRetention = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, CacheRetention::Long);
    }

    // ── Pricing cache 字段测试 ──

    #[test]
    fn test_model_pricing_cache_fields() {
        let reg = crate::model_registry::ModelRegistry::from_builtin();
        let model = reg.get("deepseek-chat").unwrap();
        assert_eq!(model.pricing.cache_read, 0.07);
        assert_eq!(model.pricing.cache_write, 0.27);

        let model = reg.get("claude-sonnet-4-6").unwrap();
        assert_eq!(model.pricing.cache_read, 0.30);
        assert_eq!(model.pricing.cache_write, 3.75);
    }

    #[test]
    fn test_model_pricing_default_cache_zero() {
        let reg = crate::model_registry::ModelRegistry::from_builtin();
        let model = reg.get("gpt-4o").unwrap();
        assert_eq!(model.pricing.cache_read, 0.0);
        assert_eq!(model.pricing.cache_write, 0.0);
    }
}
