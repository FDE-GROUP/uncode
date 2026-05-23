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
