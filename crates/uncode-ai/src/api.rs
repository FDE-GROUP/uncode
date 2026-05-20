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

#[derive(Debug, Clone)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
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
