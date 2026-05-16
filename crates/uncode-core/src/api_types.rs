use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::tool::ToolDefinition;

// ── Context ──

/// 对话状态容器，独立于请求参数
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}

// ── StreamOptions ──

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
    pub on_payload: Option<Arc<dyn Fn(&serde_json::Value) + Send + Sync>>,
    pub on_response: Option<Arc<dyn Fn(u16, &HashMap<String, String>) + Send + Sync>>,
}

// ── Thinking ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Low,
    Medium,
    High,
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
}

impl Default for CompatConfig {
    fn default() -> Self {
        Self {
            supports_developer_role: true,
            supports_reasoning_effort: false,
            supports_usage_in_streaming: true,
            supports_strict_mode: false,
            max_tokens_field: MaxTokensField::MaxTokens,
            requires_tool_result_name: false,
            requires_assistant_after_tool_result: false,
            requires_thinking_as_text: false,
            done_breaks_stream: false,
            thinking_format: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxTokensField {
    MaxTokens,
    MaxCompletionTokens,
}
