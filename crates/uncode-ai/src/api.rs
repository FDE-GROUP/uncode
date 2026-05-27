use crate::api_types::{Context, StopReason, StreamOptions};
use crate::message::Message;
use crate::model::Model;
use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};

// ── Stream protocol types ──

/// LLM 流式输出协议事件（须以 [`StreamEvent::Done`] 结束）。
///
/// **Pi:** 对应 `pi-ai` 流式 delta；工具调用遵循 Start → Delta → End 三阶段。
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments: String },
    ToolCallEnd(Box<ToolCallEndData>),
    Usage(UsageInfo),
    Error { reason: StopReason, message: String },
    Done { reason: StopReason },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCallEndData {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_hit_tokens: Option<u64>,
    pub cache_miss_tokens: Option<u64>,
}

/// API 协议抽象——每个 API 协议一个实现（openai-completions、anthropic-messages 等）。
///
/// **Pi:** 对应 `pi-ai` 的 `Api` 分层；供应商通过 `Model` 声明接入，不新增驱动 crate。
#[async_trait]
pub trait Api: Send + Sync {
    /// 此 API 的标识符，如 "openai-completions"
    fn api_name(&self) -> &'static str;

    /// 流式补全
    async fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<BoxStream<'static, StreamEvent>, uncode_shared::error::UncodeError>;

    /// 非流式补全（默认消费整个流构建完整消息）
    async fn complete(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<Message, uncode_shared::error::UncodeError> {
        let s = self.stream(model, context, options).await?;
        collect_assistant_message(s).await
    }
}

/// 消费整个流，构建完整 Assistant 消息
async fn collect_assistant_message(
    mut stream: BoxStream<'static, StreamEvent>,
) -> Result<Message, uncode_shared::error::UncodeError> {
    use crate::message::{ContentBlock, Role, UsageInfo as CoreUsageInfo};

    let mut text = String::with_capacity(2048);
    let mut thinking = String::with_capacity(1024);
    let mut tool_calls: Vec<crate::message::ToolCall> = Vec::new();
    let mut current_tool_id = String::with_capacity(32);
    let mut current_tool_name = String::with_capacity(32);
    let mut current_tool_args = String::with_capacity(512);
    let mut usage: Option<CoreUsageInfo> = None;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(delta) => text.push_str(&delta),
            StreamEvent::ThinkingDelta(delta) => thinking.push_str(&delta),
            StreamEvent::ToolCallStart { id, name } => {
                current_tool_id = id;
                current_tool_name = name;
                current_tool_args.clear();
            }
            StreamEvent::ToolCallDelta { arguments, .. } => current_tool_args.push_str(&arguments),
            StreamEvent::ToolCallEnd(data) => {
                tool_calls.push(crate::message::ToolCall {
                    id: current_tool_id.clone(),
                    name: current_tool_name.clone(),
                    arguments: data.arguments,
                });
            }
            StreamEvent::Usage(u) => {
                usage = Some(CoreUsageInfo {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cost: None,
                });
            }
            StreamEvent::Error { message, .. } => {
                return Err(uncode_shared::error::UncodeError::Llm(message));
            }
            StreamEvent::Done { .. } => break,
        }
    }

    let mut content: Vec<ContentBlock> = Vec::new();
    if !thinking.is_empty() {
        content.push(ContentBlock::Thinking { text: thinking });
    }
    if !text.is_empty() {
        content.push(ContentBlock::Text { text });
    }
    for tc in tool_calls {
        content.push(ContentBlock::ToolCall(Box::new(tc)));
    }

    let mut msg = Message::new(Role::Assistant, content);
    msg.usage = usage;

    Ok(msg)
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentBlock, Role};
    use futures::stream;

    fn stream_from_events(events: Vec<StreamEvent>) -> BoxStream<'static, StreamEvent> {
        stream::iter(events).boxed()
    }

    #[tokio::test]
    async fn test_collect_text_only() {
        let events = vec![
            StreamEvent::TextDelta("Hello, ".into()),
            StreamEvent::TextDelta("world!".into()),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello, world!"),
            _ => panic!("expected Text block"),
        }
    }

    #[tokio::test]
    async fn test_collect_thinking_and_text() {
        let events = vec![
            StreamEvent::ThinkingDelta("Let me think...".into()),
            StreamEvent::TextDelta("Here is the answer.".into()),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert_eq!(msg.content.len(), 2);
        match &msg.content[0] {
            ContentBlock::Thinking { text } => assert_eq!(text, "Let me think..."),
            _ => panic!("expected Thinking block"),
        }
        match &msg.content[1] {
            ContentBlock::Text { text } => assert_eq!(text, "Here is the answer."),
            _ => panic!("expected Text block"),
        }
    }

    #[tokio::test]
    async fn test_collect_single_tool_call() {
        let events = vec![
            StreamEvent::TextDelta("I'll search for you.".into()),
            StreamEvent::ToolCallStart {
                id: "call_1".into(),
                name: "web_search".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "call_1".into(),
                arguments: r#"{"query": "rust"}"#.into(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_1".into(),
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "rust"}),
            })),
            StreamEvent::Done {
                reason: StopReason::ToolUse,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert_eq!(msg.content.len(), 2);
        match &msg.content[1] {
            ContentBlock::ToolCall(tc) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "web_search");
                assert_eq!(tc.arguments, serde_json::json!({"query": "rust"}));
            }
            _ => panic!("expected ToolCall block"),
        }
        // stop_reason is not set by collect_assistant_message, only by the stream
    }

    #[tokio::test]
    async fn test_collect_multiple_tool_calls() {
        let events = vec![
            StreamEvent::ToolCallStart {
                id: "call_1".into(),
                name: "read".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "call_1".into(),
                arguments: r#"{"path": "/tmp/a"}"#.into(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_1".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "/tmp/a"}),
            })),
            StreamEvent::ToolCallStart {
                id: "call_2".into(),
                name: "write".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "call_2".into(),
                arguments: r#"{"path": "/tmp/b"}"#.into(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_2".into(),
                name: "write".into(),
                arguments: serde_json::json!({"path": "/tmp/b"}),
            })),
            StreamEvent::Done {
                reason: StopReason::ToolUse,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert_eq!(msg.content.len(), 2);
        for tc in msg.content.iter().filter_map(|c| match c {
            ContentBlock::ToolCall(tc) => Some(tc.as_ref()),
            _ => None,
        }) {
            assert!(tc.id == "call_1" || tc.id == "call_2");
        }
    }

    #[tokio::test]
    async fn test_collect_error_event_returns_err() {
        let events = vec![
            StreamEvent::TextDelta("Partial text...".into()),
            StreamEvent::Error {
                reason: StopReason::Error,
                message: "rate limit exceeded".into(),
            },
        ];
        let err = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("rate limit exceeded"));
    }

    #[tokio::test]
    async fn test_collect_usage() {
        let events = vec![
            StreamEvent::TextDelta("Answer.".into()),
            StreamEvent::Usage(UsageInfo {
                cache_hit_tokens: None,
                cache_miss_tokens: None,
                input_tokens: 50,
                output_tokens: 10,
            }),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        let usage = msg.usage.unwrap();
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.output_tokens, 10);
    }

    #[tokio::test]
    async fn test_collect_empty_stream() {
        let events = vec![StreamEvent::Done {
            reason: StopReason::Stop,
        }];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert!(msg.content.is_empty());
        assert_eq!(msg.role, Role::Assistant);
    }

    #[tokio::test]
    async fn test_collect_clean_usage_filter() {
        // Two usage events — last one wins
        let events = vec![
            StreamEvent::Usage(UsageInfo {
                cache_hit_tokens: None,
                cache_miss_tokens: None,
                input_tokens: 10,
                output_tokens: 5,
            }),
            StreamEvent::Usage(UsageInfo {
                cache_hit_tokens: None,
                cache_miss_tokens: None,
                input_tokens: 20,
                output_tokens: 8,
            }),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        let usage = msg.usage.unwrap();
        assert_eq!(usage.input_tokens, 20);
        assert_eq!(usage.output_tokens, 8);
    }

    #[tokio::test]
    async fn test_collect_thinking_only() {
        let events = vec![
            StreamEvent::ThinkingDelta("Deep thoughts...".into()),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            ContentBlock::Thinking { text } => assert_eq!(text, "Deep thoughts..."),
            _ => panic!("expected Thinking block"),
        }
    }

    #[tokio::test]
    async fn test_collect_aborted_reason() {
        let events = vec![
            StreamEvent::TextDelta("Before abort...".into()),
            StreamEvent::Done {
                reason: StopReason::Aborted,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Before abort..."),
            _ => panic!("expected Text block"),
        }
    }

    #[tokio::test]
    async fn test_collect_reason_length() {
        let events = vec![
            StreamEvent::TextDelta("Truncated output...".into()),
            StreamEvent::Done {
                reason: StopReason::Length,
            },
        ];
        let msg = collect_assistant_message(stream_from_events(events))
            .await
            .unwrap();
        assert_eq!(msg.content.len(), 1);
    }
}
