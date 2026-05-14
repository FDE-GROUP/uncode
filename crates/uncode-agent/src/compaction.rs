use std::sync::Arc;
use uncode_core::message::{ContentBlock, Message, Role};
use uncode_llm::LlmDriver;
use uncode_llm::driver::CompletionRequest;

const COMPACTION_THRESHOLD_NUM: u64 = 80;
const COMPACTION_THRESHOLD_DEN: u64 = 100;
const KEEP_RECENT: usize = 5;
const EST_CHARS_PER_TOKEN: u64 = 4;

/// 估算消息列表的总 token 数
pub fn estimate_context_tokens(messages: &[Message]) -> u64 {
    let mut total: u64 = 0;
    for msg in messages {
        for block in &msg.content {
            let text = match block {
                ContentBlock::Text { text } => text,
                ContentBlock::Thinking { text } => text,
                ContentBlock::ToolCall(tc) => &tc.name,
                ContentBlock::ToolResult(tr) => &tr.content,
                ContentBlock::Image { .. } => "[image]",
                _ => "",
            };
            total += (text.len() as u64).div_ceil(EST_CHARS_PER_TOKEN);
        }
    }
    total
}

/// 判断是否超过模型上下文窗口 80% 阈值
pub fn should_compact(messages: &[Message], model_max_tokens: u64) -> bool {
    let estimated = estimate_context_tokens(messages);
    let threshold = model_max_tokens * COMPACTION_THRESHOLD_NUM / COMPACTION_THRESHOLD_DEN;
    estimated > threshold
}

/// 对消息列表执行上下文压缩（保留最近5轮，旧消息LLM摘要）
pub async fn compact_messages(
    messages: &mut Vec<Message>,
    driver: &Arc<dyn LlmDriver>,
    model: &str,
    model_max_tokens: u64,
) -> anyhow::Result<()> {
    if messages.len() <= KEEP_RECENT + 4 || !should_compact(messages, model_max_tokens) {
        return Ok(());
    }

    let system_idx = messages.iter().position(|m| m.role == Role::System);
    let split_at = messages.len().saturating_sub(KEEP_RECENT);
    let keep_start = system_idx.map(|i| i + 1).unwrap_or(0);
    let summary_count = split_at.saturating_sub(keep_start);

    if summary_count == 0 {
        return Ok(());
    }

    let conversation = messages[keep_start..split_at]
        .iter()
        .filter_map(|m| match &m.role {
            Role::User => Some(format!("用户: {}", extract_text(&m.content))),
            Role::Assistant => Some(format!("Agent: {}", extract_text(&m.content))),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "请用2-3句话总结以下对话的关键内容：目标、已完成工作、当前进展。\n\n{conversation}"
    );

    let request = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message::user(prompt)],
        system: Some("你是一个会话摘要助手。用简洁的中文生成摘要。".into()),
        max_tokens: Some(1024),
        temperature: Some(0.3),
        tools: vec![],
    };

    let mut stream = driver.complete(request).await?;
    use futures::StreamExt;
    let mut summary = String::new();
    while let Some(event) = stream.next().await {
        if let uncode_llm::driver::StreamEvent::TextDelta(text) = event {
            summary.push_str(&text);
        }
    }

    let compact_entry = Message::new(
        Role::System,
        vec![ContentBlock::Text {
            text: format!("[上下文摘要]\n{summary}"),
        }],
    );

    messages.drain(keep_start..split_at);
    messages.insert(keep_start, compact_entry);

    tracing::info!(
        "compaction: summarized {} messages -> {} messages remaining",
        summary_count,
        messages.len()
    );

    Ok(())
}

fn extract_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            ContentBlock::Thinking { .. } => None,
            ContentBlock::ToolCall(_) => Some("\u{1f527}"),
            ContentBlock::ToolResult(tr) => {
                if tr.content.len() > 200 {
                    Some(&tr.content[..200])
                } else {
                    Some(&tr.content)
                }
            }
            ContentBlock::Image { .. } => Some("[image]"),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
