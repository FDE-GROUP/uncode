//! 上下文压缩：token 估算、会话级 Compaction 条目写入。
//!
//! **Pi:** 对应 Compaction 流程与 `session_before_compact` Hook；阈值约 context_window × 80%。

use std::collections::HashMap;

use crate::session::store::SessionStore;
use futures::StreamExt;
use uncode_ai::{ApiRegistry, StreamEvent};
use uncode_core::api_types::{Context, StreamOptions};
use uncode_core::message::{ContentBlock, Message, Role};
use uncode_core::model::Model;
use uncode_core::session::{CompactionEntry, MessageEntry, SessionEntry, generate_entry_id};

const COMPACTION_THRESHOLD_NUM: u64 = 80;
const COMPACTION_THRESHOLD_DEN: u64 = 100;
const KEEP_RECENT_RATIO_NUM: u64 = 20;
const KEEP_RECENT_RATIO_DEN: u64 = 100;
const EST_CHARS_PER_TOKEN: u64 = 4;

// ── Legacy in-memory API (kept for backward compat) ──

/// 估算消息列表的总 token 数
pub fn estimate_context_tokens(messages: &[Message]) -> u64 {
    messages
        .iter()
        .flat_map(|msg| msg.content.iter())
        .map(|block| {
            let text = match block {
                ContentBlock::Text { text } => text,
                ContentBlock::Thinking { text } => text,
                ContentBlock::ToolCall(tc) => &tc.name,
                ContentBlock::ToolResult(tr) => &tr.content,
                ContentBlock::Image { .. } => "[image]",
                _ => "",
            };
            (text.len() as u64).div_ceil(EST_CHARS_PER_TOKEN)
        })
        .sum()
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
    api_registry: &ApiRegistry,
    model: &Model,
    api_keys: &HashMap<String, String>,
) -> anyhow::Result<()> {
    const KEEP_RECENT: usize = 5;
    if messages.len() <= KEEP_RECENT + 4 || !should_compact(messages, model.context_window as u64) {
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

    let summary = generate_summary(&conversation, None, api_registry, model, api_keys).await?;

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

pub(crate) fn extract_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            ContentBlock::Thinking { .. } => None,
            ContentBlock::ToolCall(_) => Some("\u{1f527}"),
            ContentBlock::ToolResult(tr) => {
                if tr.content.len() > 200 {
                    let end = floor_char_boundary(&tr.content, 200);
                    Some(&tr.content[..end])
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

// ── Session-aware Pi-style compaction ──

/// Check if a session needs compaction based on stored entry token estimate.
///
/// **Pi:** 对应压缩触发判断（约 context_window × 80%）。
pub async fn should_compact_session(
    store: &SessionStore,
    session_id: &str,
    context_window: u64,
) -> bool {
    let entries = match store.load_entries(session_id).await {
        Ok(e) => e,
        Err(_) => return false,
    };
    let estimated = estimate_entry_tokens(&entries);
    let threshold = context_window * COMPACTION_THRESHOLD_NUM / COMPACTION_THRESHOLD_DEN;
    estimated > threshold
}

/// Compact a session by summarizing old entries and persisting a CompactionEntry.
///
/// Returns `Some(CompactionEntry)` if compaction was performed, `None` if not needed.
///
/// **Pi:** 对应 Compaction + 迭代摘要；Hook 点见 `session_before_compact`。
pub async fn compact_session(
    store: &SessionStore,
    session_id: &str,
    api_registry: &ApiRegistry,
    model: &Model,
    api_keys: &HashMap<String, String>,
) -> anyhow::Result<Option<CompactionEntry>> {
    let entries = store
        .load_entries(session_id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Find previous CompactionEntry for iterative summarization
    let prev_summary = entries.iter().rev().find_map(|e| match e {
        SessionEntry::Compaction(ce) => Some(ce.summary.clone()),
        _ => None,
    });

    // Calculate tokens to keep (20% of context window)
    let keep_tokens = model.context_window as u64 * KEEP_RECENT_RATIO_NUM / KEEP_RECENT_RATIO_DEN;

    // Find cut point
    let cut_id = match find_cut_point(&entries, keep_tokens) {
        Some(id) => id,
        None => return Ok(None),
    };

    // Collect message entries before cut point for summarization
    let mut to_summarize: Vec<&MessageEntry> = Vec::with_capacity(entries.len());
    for entry in &entries {
        if let SessionEntry::Message(me) = entry {
            if me.id == cut_id {
                break;
            }
            to_summarize.push(me);
        }
    }

    if to_summarize.is_empty() {
        return Ok(None);
    }

    // Extract file paths from ToolCalls in summarized range
    let (files_read, files_modified) = extract_files_from_entries(&to_summarize);

    // Build conversation text for summarization
    let conversation = to_summarize
        .iter()
        .filter_map(|me| match me.role {
            Role::User => Some(format!("用户: {}", extract_text(&me.content))),
            Role::Assistant => Some(format!("Agent: {}", extract_text(&me.content))),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Generate summary (iterative if previous summary exists)
    let summary = generate_summary(
        &conversation,
        prev_summary.as_deref(),
        api_registry,
        model,
        api_keys,
    )
    .await?;

    let tokens_before = estimate_entry_tokens(&entries);

    let compaction = CompactionEntry {
        id: generate_entry_id(),
        parent_id: None,
        timestamp: chrono::Utc::now(),
        summary,
        first_kept_entry_id: cut_id,
        tokens_before,
        files_read,
        files_modified,
    };

    store
        .append_entry(
            session_id,
            &SessionEntry::Compaction(Box::new(compaction.clone())),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    tracing::info!(
        "session compaction: summarized {} entries, {} tokens before",
        to_summarize.len(),
        tokens_before
    );

    Ok(Some(compaction))
}

/// Find the entry id where compaction should cut.
///
/// Walks from the end, accumulating tokens. When the accumulated tokens reach
/// `keep_recent_tokens`, walks forward to find a User message for a clean cut boundary.
/// Detects split-turn (assistant + tool results separated) and moves cut to turn boundary.
/// Returns the id of the first entry to KEEP; everything before it is compacted.
pub(crate) fn find_cut_point(entries: &[SessionEntry], keep_recent_tokens: u64) -> Option<String> {
    let mut accumulated: u64 = 0;
    let mut threshold_idx: Option<usize> = None;

    for (i, entry) in entries.iter().enumerate().rev() {
        if let SessionEntry::Message(me) = entry {
            accumulated += estimate_message_entry_tokens(me);
            if accumulated >= keep_recent_tokens {
                threshold_idx = Some(i);
                break;
            }
        }
    }

    let threshold_idx = threshold_idx?;

    // Walk forward from threshold to find a User message (clean turn boundary)
    for entry in &entries[threshold_idx..] {
        if let SessionEntry::Message(me) = entry
            && me.role == Role::User
        {
            return Some(me.id.clone());
        }
    }

    // No clean User boundary found after threshold
    None
}

/// Detect if a cut at `cut_idx` would split a turn (assistant mid-turn without tool results).
/// Returns true if the cut separates an assistant message from its subsequent tool results.
#[cfg(test)]
pub(crate) fn is_split_turn(entries: &[SessionEntry], cut_idx: usize) -> bool {
    if cut_idx >= entries.len() {
        return false;
    }
    // Look backward from cut: if the entry just before cut is Assistant with ToolCalls,
    // and entries at/after cut include Tool results, it's a split turn.
    let mut has_tool_call_before = false;
    for i in (0..cut_idx).rev() {
        if let SessionEntry::Message(me) = &entries[i] {
            match me.role {
                Role::Assistant => {
                    has_tool_call_before = me
                        .content
                        .iter()
                        .any(|b| matches!(b, ContentBlock::ToolCall(_)));
                    break;
                }
                Role::User => break,
                _ => continue,
            }
        }
    }

    if !has_tool_call_before {
        return false;
    }

    // Check if entries at/after cut start with Tool results
    for entry in entries.iter().skip(cut_idx) {
        if let SessionEntry::Message(me) = entry {
            return me.role == Role::Tool;
        }
    }

    false
}

/// Adjust cut point to avoid splitting a turn. If split-turn detected, move cut backward
/// to the User message that started the incomplete turn.
#[cfg(test)]
pub(crate) fn adjust_for_split_turn(entries: &[SessionEntry], cut_idx: usize) -> Option<String> {
    if !is_split_turn(entries, cut_idx) {
        return entries.get(cut_idx).and_then(|e| match e {
            SessionEntry::Message(me) => Some(me.id.clone()),
            _ => None,
        });
    }

    // Walk backward from cut_idx to find the User that started this turn
    for i in (0..cut_idx).rev() {
        if let SessionEntry::Message(me) = &entries[i] {
            if me.role == Role::User {
                return Some(me.id.clone());
            }
        }
    }

    None
}

// ── Helpers ──

/// Floor to the nearest valid UTF-8 char boundary at or before `max`.
/// Needed because `str::ceil_char_boundary` requires MSRV 1.91+.
fn floor_char_boundary(s: &str, mut max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(max) {
        max -= 1;
    }
    max
}

fn estimate_message_entry_tokens(me: &MessageEntry) -> u64 {
    me.content
        .iter()
        .map(|block| {
            let text = match block {
                ContentBlock::Text { text } => text.as_str(),
                ContentBlock::Thinking { text } => text.as_str(),
                ContentBlock::ToolCall(tc) => tc.name.as_str(),
                ContentBlock::ToolResult(tr) => tr.content.as_str(),
                ContentBlock::Image { .. } => "[image]",
                _ => "",
            };
            (text.len() as u64).div_ceil(EST_CHARS_PER_TOKEN)
        })
        .sum()
}

fn estimate_entry_tokens(entries: &[SessionEntry]) -> u64 {
    entries
        .iter()
        .filter_map(|e| match e {
            SessionEntry::Message(me) => Some(estimate_message_entry_tokens(me)),
            _ => None,
        })
        .sum()
}

fn extract_files_from_entries(entries: &[&MessageEntry]) -> (Vec<String>, Vec<String>) {
    let mut files_read = Vec::with_capacity(entries.len());
    let mut files_modified = Vec::with_capacity(entries.len());

    for me in entries {
        if me.role == Role::Assistant {
            for block in &me.content {
                if let ContentBlock::ToolCall(tc) = block {
                    let path = tc
                        .arguments
                        .get("path")
                        .or_else(|| tc.arguments.get("file_path"))
                        .and_then(|v| v.as_str());
                    if let Some(path) = path {
                        match tc.name.as_str() {
                            "read" | "find" | "grep" | "ls" => files_read.push(path.to_string()),
                            "write" | "edit" => files_modified.push(path.to_string()),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    files_read.sort();
    files_read.dedup();
    files_modified.sort();
    files_modified.dedup();

    (files_read, files_modified)
}

/// Summarization prompt for first-time compaction (8-section format).
const SUMMARIZATION_PROMPT: &str = "\
请分析以下对话内容，生成一个结构化摘要，严格使用以下 8 节格式：

## Goal
（对话的最终目标）

## Constraints & Preferences
（用户提出的约束、偏好、技术选型等）

## Progress
### Done
（已完成的工作项）
### In Progress
（正在进行的工作项）
### Blocked
（被阻塞的工作项及原因）

## Key Decisions
（已做出的关键设计/架构决策）

## Next Steps
（接下来应执行的具体步骤）

## Critical Context
（必须记住的关键上下文信息，如文件路径、错误信息、配置值等）

对话内容：
";

/// Prompt for incremental summary update.
const UPDATE_SUMMARIZATION_PROMPT: &str = "\
你是一个会话摘要助手。以下是之前的历史摘要和新的对话内容。

指令：
- PRESERVE 已有内容——不要删除或改写已有信息
- ADD 新信息到对应章节
- MOVE 在 Done/In Progress/Blocked 之间移动变化了状态的工作项
- UPDATE Next Steps 为最新的下一步计划

之前的摘要：
";

async fn generate_summary(
    conversation: &str,
    prev_summary: Option<&str>,
    api_registry: &ApiRegistry,
    model: &Model,
    api_keys: &HashMap<String, String>,
) -> anyhow::Result<String> {
    let prompt = match prev_summary {
        Some(prev) => {
            format!("{UPDATE_SUMMARIZATION_PROMPT}\n{prev}\n\n新的对话内容：\n{conversation}")
        }
        None => format!("{SUMMARIZATION_PROMPT}\n{conversation}"),
    };

    let api_key = api_keys.get(&model.provider).cloned();
    let context = Context {
        system_prompt: Some("你是一个会话摘要助手。用简洁的中文生成摘要。".into()),
        messages: vec![Message::user(prompt)],
        tools: vec![],
    };
    let options = StreamOptions {
        api_key,
        temperature: Some(0.3),
        max_tokens: Some(1024),
        ..StreamOptions::default()
    };

    let mut stream = uncode_ai::stream(model, &context, &options, api_registry).await?;
    let mut summary = String::with_capacity(512);
    while let Some(event) = stream.next().await {
        if let StreamEvent::TextDelta(text) = event {
            summary.push_str(&text);
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_core::message::{ToolCall, ToolResult};
    use uncode_core::session::{MessageEntry, SessionEntry};

    // ── find_cut_point tests ──

    fn make_msg_entry(id: &str, role: Role, text: &str) -> SessionEntry {
        SessionEntry::Message(Box::new(MessageEntry {
            id: id.to_string(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            role,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            usage: None,
        }))
    }

    #[test]
    fn test_find_cut_point_basic() {
        let entries = vec![
            make_msg_entry("u1", Role::User, &"x".repeat(200)),
            make_msg_entry("a1", Role::Assistant, &"y".repeat(200)),
            make_msg_entry("u2", Role::User, &"z".repeat(200)),
            make_msg_entry("a2", Role::Assistant, &"w".repeat(200)),
        ];

        // keep_recent_tokens=150 → keep ~2 entries from end (a2 + u2 = ~100 tokens)
        // threshold falls on u2 or before → walk forward to u2
        let cut = find_cut_point(&entries, 150);
        assert!(cut.is_some());
        // Should cut at u2 (first User after threshold)
        assert_eq!(cut.unwrap(), "u2");
    }

    #[test]
    fn test_find_cut_point_no_threshold_reached() {
        let entries = vec![
            make_msg_entry("u1", Role::User, "short"),
            make_msg_entry("a1", Role::Assistant, "short"),
        ];

        // Total tokens < 1000, so no cut point
        let cut = find_cut_point(&entries, 1000);
        assert!(cut.is_none());
    }

    #[test]
    fn test_find_cut_point_tool_result_boundary() {
        let entries = vec![
            make_msg_entry("u1", Role::User, &"x".repeat(200)),
            SessionEntry::Message(Box::new(MessageEntry {
                id: "a1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/test.rs"}),
                }))],
                usage: None,
            })),
            SessionEntry::Message(Box::new(MessageEntry {
                id: "t1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(Box::new(ToolResult {
                    tool_call_id: "tc1".into(),
                    content: "file contents".into(),
                    is_error: false,
                }))],
                usage: None,
            })),
            make_msg_entry("u2", Role::User, &"y".repeat(200)),
            make_msg_entry("a2", Role::Assistant, &"z".repeat(200)),
        ];

        // Should cut at u2, keeping the complete tool turn
        // a2(50)+u2(50)=100 >= 99 → threshold at u2 → cut at u2
        let cut = find_cut_point(&entries, 99);
        assert_eq!(cut.unwrap(), "u2");
    }

    #[test]
    fn test_find_cut_point_user_at_start_only() {
        // 3 entries: threshold falls on a2, no User after it → returns None
        let entries = vec![
            make_msg_entry("u1", Role::User, "hello"),
            make_msg_entry("a1", Role::Assistant, &"x".repeat(200)),
            make_msg_entry("a2", Role::Assistant, &"y".repeat(200)),
        ];

        // keep=50: a2(50) >= 50 → threshold at a2. No User after a2 → None
        let cut = find_cut_point(&entries, 50);
        assert!(cut.is_none());
    }

    #[test]
    fn test_find_cut_point_multiple_users() {
        let entries = vec![
            make_msg_entry("u1", Role::User, &"x".repeat(200)),
            make_msg_entry("a1", Role::Assistant, &"x".repeat(200)),
            make_msg_entry("u2", Role::User, &"y".repeat(200)),
            make_msg_entry("a2", Role::Assistant, &"y".repeat(200)),
            make_msg_entry("u3", Role::User, &"z".repeat(200)),
            make_msg_entry("a3", Role::Assistant, &"z".repeat(200)),
        ];

        // keep=100: a3(50)+u3(50)=100 → threshold at u3 → User found → cut at u3
        let cut = find_cut_point(&entries, 100);
        assert_eq!(cut.unwrap(), "u3");
    }

    // ── extract_files_from_entries tests ──

    #[test]
    fn test_extract_files_read() {
        let entries = vec![MessageEntry {
            id: "a1".into(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            role: Role::Assistant,
            content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                id: "tc1".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "/src/main.rs"}),
            }))],
            usage: None,
        }];

        let (read, modified) = extract_files_from_entries(&entries.iter().collect::<Vec<_>>());
        assert_eq!(read, vec!["/src/main.rs"]);
        assert!(modified.is_empty());
    }

    #[test]
    fn test_extract_files_modified() {
        let entries = vec![MessageEntry {
            id: "a1".into(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            role: Role::Assistant,
            content: vec![
                ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc1".into(),
                    name: "edit".into(),
                    arguments: serde_json::json!({"path": "/src/lib.rs"}),
                })),
                ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc2".into(),
                    name: "write".into(),
                    arguments: serde_json::json!({"path": "/src/new.rs"}),
                })),
            ],
            usage: None,
        }];

        let refs: Vec<&MessageEntry> = entries.iter().collect();
        let (read, modified) = extract_files_from_entries(&refs);
        assert!(read.is_empty());
        assert_eq!(modified, vec!["/src/lib.rs", "/src/new.rs"]);
    }

    #[test]
    fn test_extract_files_dedup() {
        let entries = vec![
            MessageEntry {
                id: "a1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/src/main.rs"}),
                }))],
                usage: None,
            },
            MessageEntry {
                id: "a2".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc2".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/src/main.rs"}),
                }))],
                usage: None,
            },
        ];

        let refs: Vec<&MessageEntry> = entries.iter().collect();
        let (read, modified) = extract_files_from_entries(&refs);
        assert_eq!(read, vec!["/src/main.rs"]);
        assert!(modified.is_empty());
    }

    // ── should_compact_session tests ──

    #[tokio::test]
    async fn test_should_compact_session_empty() {
        let store = SessionStore::new_memory().await.expect("store");
        store
            .init_session("test-session", "model", "/test")
            .await
            .unwrap();

        assert!(!should_compact_session(&store, "test-session", 1000).await);
    }

    #[tokio::test]
    async fn test_should_compact_session_small() {
        let store = SessionStore::new_memory().await.expect("store");
        store
            .init_session("test-session", "model", "/test")
            .await
            .unwrap();

        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(Message::user("hello").into()),
            )
            .await
            .unwrap();

        assert!(!should_compact_session(&store, "test-session", 1000).await);
    }

    #[tokio::test]
    async fn test_should_compact_session_large() {
        let store = SessionStore::new_memory().await.expect("store");
        store
            .init_session("test-session", "model", "/test")
            .await
            .unwrap();

        // Add large messages (500 chars each ≈ 125 tokens, need > 800 for 1000 window)
        for _ in 0..10 {
            store
                .append_entry(
                    "test-session",
                    &SessionEntry::Message(Message::user(&"x".repeat(500)).into()),
                )
                .await
                .unwrap();
        }

        // 10 * 500 chars = 5000 chars ≈ 1250 tokens > 1000 * 80% = 800
        assert!(should_compact_session(&store, "test-session", 1000).await);
    }

    // ── split-turn detection tests ──

    #[test]
    fn test_is_split_turn_false() {
        let entries = vec![
            make_msg_entry("u1", Role::User, "hello"),
            make_msg_entry("a1", Role::Assistant, "world"),
            make_msg_entry("u2", Role::User, "next"),
        ];
        // cut at u2 (idx=2): a1 has no tool calls → not split
        assert!(!is_split_turn(&entries, 2));
    }

    #[test]
    fn test_is_split_turn_true() {
        let entries = vec![
            make_msg_entry("u1", Role::User, "hello"),
            SessionEntry::Message(Box::new(MessageEntry {
                id: "a1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/test.rs"}),
                }))],
                usage: None,
            })),
            SessionEntry::Message(Box::new(MessageEntry {
                id: "t1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(Box::new(ToolResult {
                    tool_call_id: "tc1".into(),
                    content: "file contents".into(),
                    is_error: false,
                }))],
                usage: None,
            })),
            make_msg_entry("u2", Role::User, "next"),
        ];
        // cut at t1 (idx=2): a1 has tool call, t1 is Tool → split turn
        assert!(is_split_turn(&entries, 2));
    }

    #[test]
    fn test_adjust_for_split_turn_no_split() {
        let entries = vec![
            make_msg_entry("u1", Role::User, "hello"),
            make_msg_entry("a1", Role::Assistant, "world"),
            make_msg_entry("u2", Role::User, "next"),
        ];
        // cut at u2, no split → returns u2
        assert_eq!(adjust_for_split_turn(&entries, 2), Some("u2".into()));
    }

    #[test]
    fn test_adjust_for_split_turn_moves_back() {
        let entries = vec![
            make_msg_entry("u1", Role::User, "read this"),
            SessionEntry::Message(Box::new(MessageEntry {
                id: "a1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/test.rs"}),
                }))],
                usage: None,
            })),
            SessionEntry::Message(Box::new(MessageEntry {
                id: "t1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(Box::new(ToolResult {
                    tool_call_id: "tc1".into(),
                    content: "contents".into(),
                    is_error: false,
                }))],
                usage: None,
            })),
            make_msg_entry("u2", Role::User, "continue"),
        ];
        // cut at t1 (idx=2): split detected → move back to u1
        assert_eq!(adjust_for_split_turn(&entries, 2), Some("u1".into()));
    }
}
