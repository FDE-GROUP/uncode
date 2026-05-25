use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::tool_def::ToolDefinition;

// ── Type aliases for complex callback types ──

/// 流式 payload 回调 — 接收 `&mut` 以允许扩展修改非核心字段。
pub type PayloadCallback = Arc<dyn Fn(&mut serde_json::Value) + Send + Sync>;
/// HTTP 响应回调
pub type ResponseCallback = Arc<dyn Fn(u16, &HashMap<String, String>) + Send + Sync>;
/// 上下文变换回调
pub type TransformContextCallback = Arc<dyn Fn(&mut Vec<Message>) + Send + Sync>;

// ── Context ──

/// 对话状态容器，独立于请求参数
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}

// ── StreamOptions ──

/// LLM 传输协议
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    #[default]
    Sse,
    WebSocket,
    WebSocketCached,
    Auto,
}

/// 请求参数，每次调用可独立设置
#[derive(Clone, Default)]
pub struct StreamOptions {
    pub api_key: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub signal: Option<tokio_util::sync::CancellationToken>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
    pub headers: Option<HashMap<String, String>>,
    pub thinking_level: Option<ThinkingLevel>,
    pub thinking_budget_tokens: Option<u32>,
    pub session_id: Option<String>,
    pub cache_retention: Option<CacheRetention>,
    pub on_payload: Option<PayloadCallback>,
    pub on_response: Option<ResponseCallback>,
    pub transport: Option<Transport>,
    pub metadata: Option<HashMap<String, String>>,
}

impl std::fmt::Debug for StreamOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamOptions")
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("timeout_ms", &self.timeout_ms)
            .field("max_retries", &self.max_retries)
            .field("max_retry_delay_ms", &self.max_retry_delay_ms)
            .field("headers", &self.headers)
            .field("thinking_level", &self.thinking_level)
            .field("thinking_budget_tokens", &self.thinking_budget_tokens)
            .field("session_id", &self.session_id)
            .field("cache_retention", &self.cache_retention)
            .field("transport", &self.transport)
            .field("metadata", &self.metadata)
            .finish()
    }
}

// ── Thinking ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingFormat {
    OpenAi,
    DeepSeek,
    Anthropic,
    Gemini,
    OpenRouter,
    Together,
    ZAi,
    Qwen,
    QwenChatTemplate,
}

// ── StopReason ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

// ── InputModality ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputModality {
    Text,
    Image,
    Audio,
}

// ── CacheRetention ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CacheRetention {
    None,
    #[default]
    Short,
    Long,
}

// ── CompatConfig ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatConfig {
    pub supports_developer_role: bool,
    pub supports_reasoning_effort: bool,
    pub supports_usage_in_streaming: bool,
    pub supports_strict_mode: bool,
    pub max_tokens_field: MaxTokensField,
    pub requires_tool_result_name: bool,
    pub requires_assistant_after_tool_result: bool,
    pub requires_thinking_as_text: bool,
    pub done_breaks_stream: bool,
    pub thinking_format: Option<ThinkingFormat>,
    #[serde(default)]
    pub send_session_affinity_headers: bool,
    #[serde(default)]
    pub supports_long_cache_retention: bool,
    #[serde(default)]
    pub supports_store: bool,
    #[serde(default)]
    pub requires_reasoning_content_on_assistant_messages: bool,
    #[serde(default)]
    pub supports_eager_tool_input_streaming: bool,
    #[serde(default)]
    pub supports_cache_control_on_tools: bool,
}

impl Default for CompatConfig {
    fn default() -> Self {
        Self {
            supports_developer_role: false,
            supports_reasoning_effort: false,
            supports_usage_in_streaming: true,
            supports_strict_mode: false,
            max_tokens_field: MaxTokensField::MaxTokens,
            requires_tool_result_name: false,
            requires_assistant_after_tool_result: false,
            requires_thinking_as_text: false,
            done_breaks_stream: false,
            thinking_format: None,
            send_session_affinity_headers: false,
            supports_long_cache_retention: false,
            supports_store: false,
            requires_reasoning_content_on_assistant_messages: false,
            supports_eager_tool_input_streaming: false,
            supports_cache_control_on_tools: false,
        }
    }
}

impl CompatConfig {
    /// 厂商 preset 为 base，模型级 `compat` 中非 default 字段覆盖 base。
    pub fn merge_with_overlay(base: &CompatConfig, overlay: &CompatConfig) -> CompatConfig {
        let d = CompatConfig::default();
        CompatConfig {
            supports_developer_role: pick_bool(
                base.supports_developer_role,
                overlay.supports_developer_role,
                d.supports_developer_role,
            ),
            supports_reasoning_effort: pick_bool(
                base.supports_reasoning_effort,
                overlay.supports_reasoning_effort,
                d.supports_reasoning_effort,
            ),
            supports_usage_in_streaming: pick_bool(
                base.supports_usage_in_streaming,
                overlay.supports_usage_in_streaming,
                d.supports_usage_in_streaming,
            ),
            supports_strict_mode: pick_bool(
                base.supports_strict_mode,
                overlay.supports_strict_mode,
                d.supports_strict_mode,
            ),
            max_tokens_field: if overlay.max_tokens_field != d.max_tokens_field {
                overlay.max_tokens_field
            } else {
                base.max_tokens_field
            },
            requires_tool_result_name: pick_bool(
                base.requires_tool_result_name,
                overlay.requires_tool_result_name,
                d.requires_tool_result_name,
            ),
            requires_assistant_after_tool_result: pick_bool(
                base.requires_assistant_after_tool_result,
                overlay.requires_assistant_after_tool_result,
                d.requires_assistant_after_tool_result,
            ),
            requires_thinking_as_text: pick_bool(
                base.requires_thinking_as_text,
                overlay.requires_thinking_as_text,
                d.requires_thinking_as_text,
            ),
            done_breaks_stream: pick_bool(
                base.done_breaks_stream,
                overlay.done_breaks_stream,
                d.done_breaks_stream,
            ),
            thinking_format: overlay.thinking_format.or(base.thinking_format),
            send_session_affinity_headers: pick_bool(
                base.send_session_affinity_headers,
                overlay.send_session_affinity_headers,
                d.send_session_affinity_headers,
            ),
            supports_long_cache_retention: pick_bool(
                base.supports_long_cache_retention,
                overlay.supports_long_cache_retention,
                d.supports_long_cache_retention,
            ),
            supports_store: pick_bool(
                base.supports_store,
                overlay.supports_store,
                d.supports_store,
            ),
            requires_reasoning_content_on_assistant_messages: pick_bool(
                base.requires_reasoning_content_on_assistant_messages,
                overlay.requires_reasoning_content_on_assistant_messages,
                d.requires_reasoning_content_on_assistant_messages,
            ),
            supports_eager_tool_input_streaming: pick_bool(
                base.supports_eager_tool_input_streaming,
                overlay.supports_eager_tool_input_streaming,
                d.supports_eager_tool_input_streaming,
            ),
            supports_cache_control_on_tools: pick_bool(
                base.supports_cache_control_on_tools,
                overlay.supports_cache_control_on_tools,
                d.supports_cache_control_on_tools,
            ),
        }
    }
}

fn pick_bool(base: bool, overlay: bool, default: bool) -> bool {
    if overlay != default { overlay } else { base }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxTokensField {
    MaxTokens,
    MaxCompletionTokens,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Transport ──
    #[test]
    fn transport_serde_roundtrip() {
        let cases = [
            Transport::Sse,
            Transport::WebSocket,
            Transport::WebSocketCached,
            Transport::Auto,
        ];
        for original in &cases {
            let json = serde_json::to_string(original).unwrap();
            let decoded: Transport = serde_json::from_str(&json).unwrap();
            assert_eq!(*original, decoded);
        }
    }

    #[test]
    fn transport_default_is_sse() {
        assert_eq!(Transport::default(), Transport::Sse);
    }

    // ── ThinkingLevel ──
    #[test]
    fn thinking_level_serde_roundtrip() {
        let cases = [
            ThinkingLevel::Off,
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
            ThinkingLevel::XHigh,
        ];
        for original in &cases {
            let json = serde_json::to_string(original).unwrap();
            let decoded: ThinkingLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(*original, decoded);
        }
    }

    #[test]
    fn thinking_level_copy_eq() {
        let a = ThinkingLevel::Medium;
        let b = a;
        assert_eq!(a, b);
    }

    // ── ThinkingFormat ──
    #[test]
    fn thinking_format_serde_roundtrip() {
        let cases = [
            ThinkingFormat::OpenAi,
            ThinkingFormat::DeepSeek,
            ThinkingFormat::Anthropic,
            ThinkingFormat::Gemini,
            ThinkingFormat::OpenRouter,
            ThinkingFormat::Together,
            ThinkingFormat::ZAi,
            ThinkingFormat::Qwen,
            ThinkingFormat::QwenChatTemplate,
        ];
        for original in &cases {
            let json = serde_json::to_string(original).unwrap();
            let decoded: ThinkingFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(*original, decoded);
        }
    }

    // ── StopReason ──
    #[test]
    fn stop_reason_serde_roundtrip() {
        let cases = [
            StopReason::Stop,
            StopReason::Length,
            StopReason::ToolUse,
            StopReason::Error,
            StopReason::Aborted,
        ];
        for original in &cases {
            let json = serde_json::to_string(original).unwrap();
            let decoded: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(*original, decoded);
        }
    }

    // ── InputModality ──
    #[test]
    fn input_modality_serde_roundtrip() {
        let cases = [
            InputModality::Text,
            InputModality::Image,
            InputModality::Audio,
        ];
        for original in &cases {
            let json = serde_json::to_string(original).unwrap();
            let decoded: InputModality = serde_json::from_str(&json).unwrap();
            assert_eq!(*original, decoded);
        }
    }

    // ── CacheRetention ──
    #[test]
    fn cache_retention_serde_roundtrip() {
        let cases = [
            CacheRetention::None,
            CacheRetention::Short,
            CacheRetention::Long,
        ];
        for original in &cases {
            let json = serde_json::to_string(original).unwrap();
            let decoded: CacheRetention = serde_json::from_str(&json).unwrap();
            assert_eq!(*original, decoded);
        }
    }

    #[test]
    fn cache_retention_default_is_short() {
        assert_eq!(CacheRetention::default(), CacheRetention::Short);
    }

    // ── MaxTokensField ──
    #[test]
    fn max_tokens_field_serde_roundtrip() {
        let cases = [
            MaxTokensField::MaxTokens,
            MaxTokensField::MaxCompletionTokens,
        ];
        for original in &cases {
            let json = serde_json::to_string(original).unwrap();
            let decoded: MaxTokensField = serde_json::from_str(&json).unwrap();
            assert_eq!(*original, decoded);
        }
    }

    // ── CompatConfig ──
    #[test]
    fn compat_config_default() {
        let c = CompatConfig::default();
        assert!(!c.supports_developer_role);
        assert!(!c.supports_reasoning_effort);
        assert!(c.supports_usage_in_streaming);
        assert!(!c.supports_strict_mode);
        assert_eq!(c.max_tokens_field, MaxTokensField::MaxTokens);
        assert!(!c.requires_tool_result_name);
        assert!(!c.requires_assistant_after_tool_result);
        assert!(!c.requires_thinking_as_text);
        assert!(!c.done_breaks_stream);
        assert!(c.thinking_format.is_none());
        assert!(!c.send_session_affinity_headers);
        assert!(!c.supports_long_cache_retention);
        assert!(!c.supports_store);
        assert!(!c.requires_reasoning_content_on_assistant_messages);
        assert!(!c.supports_eager_tool_input_streaming);
        assert!(!c.supports_cache_control_on_tools);
    }

    #[test]
    fn compat_config_merge_overlay_wins() {
        let base = CompatConfig::default();
        let mut overlay = CompatConfig::default();
        overlay.supports_strict_mode = true;
        let result = CompatConfig::merge_with_overlay(&base, &overlay);
        assert!(result.supports_strict_mode);
    }

    #[test]
    fn compat_config_merge_base_when_overlay_default() {
        let mut base = CompatConfig::default();
        base.supports_developer_role = true;
        let overlay = CompatConfig::default();
        let result = CompatConfig::merge_with_overlay(&base, &overlay);
        assert!(result.supports_developer_role);
    }

    #[test]
    fn compat_config_merge_both_default() {
        let base = CompatConfig::default();
        let overlay = CompatConfig::default();
        let result = CompatConfig::merge_with_overlay(&base, &overlay);
        assert!(!result.supports_strict_mode);
        assert!(!result.supports_developer_role);
    }

    #[test]
    fn compat_config_merge_max_tokens_field() {
        let base = CompatConfig::default();
        let mut overlay = CompatConfig::default();
        overlay.max_tokens_field = MaxTokensField::MaxCompletionTokens;
        let result = CompatConfig::merge_with_overlay(&base, &overlay);
        assert_eq!(result.max_tokens_field, MaxTokensField::MaxCompletionTokens);
        let result2 = CompatConfig::merge_with_overlay(&base, &CompatConfig::default());
        assert_eq!(result2.max_tokens_field, MaxTokensField::MaxTokens);
    }

    #[test]
    fn compat_config_merge_thinking_format() {
        let mut base = CompatConfig::default();
        base.thinking_format = Some(ThinkingFormat::DeepSeek);
        let overlay = CompatConfig::default();
        let result = CompatConfig::merge_with_overlay(&base, &overlay);
        assert_eq!(result.thinking_format, Some(ThinkingFormat::DeepSeek));
        let mut overlay2 = CompatConfig::default();
        overlay2.thinking_format = Some(ThinkingFormat::Anthropic);
        let result2 = CompatConfig::merge_with_overlay(&base, &overlay2);
        assert_eq!(result2.thinking_format, Some(ThinkingFormat::Anthropic));
    }

    // ── Context ──
    #[test]
    fn context_default() {
        let ctx = Context::default();
        assert!(ctx.system_prompt.is_none());
        assert!(ctx.messages.is_empty());
        assert!(ctx.tools.is_empty());
    }

    #[test]
    fn context_construction() {
        let ctx = Context {
            system_prompt: Some("You are helpful".into()),
            messages: vec![],
            tools: vec![],
        };
        assert_eq!(ctx.system_prompt.as_deref(), Some("You are helpful"));
    }

    // ── StreamOptions ──
    #[test]
    fn stream_options_debug_hides_api_key() {
        let opts = StreamOptions {
            api_key: Some("sk-abc123".into()),
            ..Default::default()
        };
        let debug = format!("{:?}", opts);
        assert!(debug.contains("***"));
        assert!(!debug.contains("sk-abc123"));
    }

    #[test]
    fn stream_options_construction() {
        let opts = StreamOptions {
            api_key: Some("key".into()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_ms: Some(30_000),
            max_retries: Some(3),
            max_retry_delay_ms: Some(2000),
            headers: Some(HashMap::from([("X-Custom".into(), "val".into())])),
            thinking_level: Some(ThinkingLevel::High),
            thinking_budget_tokens: Some(4000),
            session_id: Some("sess-1".into()),
            cache_retention: Some(CacheRetention::Long),
            transport: Some(Transport::Auto),
            metadata: Some(HashMap::from([("key".into(), "value".into())])),
            ..Default::default()
        };
        assert_eq!(opts.api_key.as_deref(), Some("key"));
        assert_eq!(opts.temperature, Some(0.7));
        assert_eq!(opts.transport, Some(Transport::Auto));
        assert_eq!(
            opts.metadata.as_ref().and_then(|m| m.get("key")),
            Some(&"value".into())
        );
    }
}
