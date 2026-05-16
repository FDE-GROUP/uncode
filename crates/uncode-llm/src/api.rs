use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use uncode_core::api_types::{Context, StreamOptions};
use uncode_core::message::Message;
use uncode_core::model::Model;

use crate::driver::StreamEvent;

/// API 协议抽象——每个 API 协议一个实现（对齐 Pi 的 API-first 架构）
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
    ) -> Result<BoxStream<'static, StreamEvent>, uncode_core::error::UncodeError>;

    /// 非流式补全（默认消费整个流构建完整消息）
    async fn complete(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<Message, uncode_core::error::UncodeError> {
        let s = self.stream(model, context, options).await?;
        collect_assistant_message(s).await
    }
}

/// 消费整个流，构建完整 Assistant 消息
async fn collect_assistant_message(
    mut stream: BoxStream<'static, StreamEvent>,
) -> Result<Message, uncode_core::error::UncodeError> {
    use uncode_core::message::{ContentBlock, Role, UsageInfo};

    let mut text = String::new();
    let mut thinking = String::new();
    let mut tool_calls: Vec<uncode_core::message::ToolCall> = Vec::new();
    let mut current_tool_id = String::new();
    let mut current_tool_name = String::new();
    let mut current_tool_args = String::new();
    let mut usage: Option<UsageInfo> = None;

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
            StreamEvent::ToolCallEnd { arguments, .. } => {
                tool_calls.push(uncode_core::message::ToolCall {
                    id: current_tool_id.clone(),
                    name: current_tool_name.clone(),
                    arguments,
                });
            }
            StreamEvent::Usage(u) => {
                usage = Some(uncode_core::message::UsageInfo {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cost: None,
                });
            }
            StreamEvent::Error(e) => {
                return Err(uncode_core::error::UncodeError::Llm(e));
            }
            StreamEvent::Done => break,
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
        content.push(ContentBlock::ToolCall(tc));
    }

    let mut msg = Message::new(Role::Assistant, content);
    msg.usage = usage;

    Ok(msg)
}
