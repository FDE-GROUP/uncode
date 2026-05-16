//! Tree-aware context assembly — builds LLM context from session entries.
//!
//! Reads session entries from the store and reconstructs a message list suitable
//! for sending to an LLM, handling CompactionEntry boundaries, BranchSummary
//! injection, and metadata tracking (ModelChange, ThinkingLevelChange).
//!
//! Currently uses insertion order (load_entries). Will switch to tree traversal
//! (get_path_to_root) when in-place branching is implemented (Stage 5).

use crate::session::store::{SessionResult, SessionStore};
use uncode_core::api_types::ThinkingLevel;
use uncode_core::message::{ContentBlock, Message, Role};
use uncode_core::session::SessionEntry;

/// Result of building context from the session tree.
pub struct BuiltContext {
    /// Reconstructed messages in conversation order.
    pub messages: Vec<Message>,
    /// Latest model ID seen on the path (for session resume).
    pub effective_model: Option<String>,
    /// Latest thinking level seen on the path (for session resume).
    pub effective_thinking_level: Option<ThinkingLevel>,
}

/// Build message context from session entries in insertion order.
///
/// Algorithm:
/// 1. Pre-scan for the last CompactionEntry (latest compaction wins)
/// 2. Inject compaction summary first (if any)
/// 3. Walk entries in insertion order:
///    - Skip all entries before the compaction's first_kept_entry_id
///    - Skip all CompactionEntries themselves
///    - BranchSummaryEntry → inject as context message
///    - ModelChangeEntry → track effective model
///    - ThinkingLevelChangeEntry → track effective thinking level
///    - MessageEntry → convert to Message
///    - Other entries → skip
pub fn build_context(store: &SessionStore, session_id: &str) -> SessionResult<BuiltContext> {
    let entries = store.load_entries(session_id)?;

    let mut messages = Vec::with_capacity(entries.len());
    let mut effective_model: Option<String> = None;
    let mut effective_thinking_level: Option<ThinkingLevel> = None;

    // Pre-scan: find the last CompactionEntry (latest compaction wins)
    let mut skip_before_id: Option<String> = None;
    let mut compaction_summary: Option<String> = None;
    for entry in &entries {
        if let SessionEntry::Compaction(ce) = entry {
            skip_before_id = Some(ce.first_kept_entry_id.clone());
            compaction_summary = Some(ce.summary.clone());
        }
    }

    // Inject compaction summary first
    if let Some(summary) = &compaction_summary {
        messages.push(Message::new(
            Role::System,
            vec![ContentBlock::Text {
                text: format!("[上下文摘要]\n{}", summary),
            }],
        ));
    }

    let mut skip_active = skip_before_id.is_some();

    for entry in &entries {
        match entry {
            SessionEntry::Message(me) => {
                if skip_active {
                    if Some(&me.id) == skip_before_id.as_ref() {
                        skip_active = false;
                    } else {
                        continue;
                    }
                }
                let mut msg = Message::new(me.role.clone(), me.content.clone());
                msg.usage = me.usage.clone();
                messages.push(msg);
            }
            SessionEntry::Compaction(_) => {} // Already handled in pre-scan
            SessionEntry::BranchSummary(bs) => {
                let ctx_msg = Message::new(
                    Role::System,
                    vec![ContentBlock::Text {
                        text: format!("[分支摘要]\n{}", bs.summary),
                    }],
                );
                messages.push(ctx_msg);
            }
            SessionEntry::ModelChange(mc) => {
                effective_model = Some(mc.model_id.clone());
            }
            SessionEntry::ThinkingLevelChange(tl) => {
                effective_thinking_level = Some(tl.thinking_level);
            }
            _ => {}
        }
    }

    Ok(BuiltContext {
        messages,
        effective_model,
        effective_thinking_level,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_core::session::*;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("uncode-test-ctx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_build_context_empty_session() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "model", "/test")
            .unwrap();

        let ctx = build_context(&store, "test-session").unwrap();
        assert!(ctx.messages.is_empty());
        assert!(ctx.effective_model.is_none());

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_build_context_linear_chain() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "model", "/test")
            .unwrap();

        let user_msg = Message::user("hello");
        store
            .append_entry("test-session", &SessionEntry::Message(user_msg.into()))
            .unwrap();

        let asst_msg = Message::assistant("world");
        store
            .append_entry("test-session", &SessionEntry::Message(asst_msg.into()))
            .unwrap();

        let ctx = build_context(&store, "test-session").unwrap();
        assert_eq!(ctx.messages.len(), 2);
        assert_eq!(ctx.messages[0].role, Role::User);
        assert_eq!(ctx.messages[1].role, Role::Assistant);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_build_context_with_compaction() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "model", "/test")
            .unwrap();

        // Old messages (will be compacted away)
        let msg1_id = generate_entry_id();
        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(MessageEntry {
                    id: msg1_id.clone(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "old message".into(),
                    }],
                    usage: None,
                }),
            )
            .unwrap();

        let msg2_id = generate_entry_id();
        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(MessageEntry {
                    id: msg2_id.clone(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text {
                        text: "old response".into(),
                    }],
                    usage: None,
                }),
            )
            .unwrap();

        // CompactionEntry — keeps from msg2_id onward
        store
            .append_entry(
                "test-session",
                &SessionEntry::Compaction(CompactionEntry {
                    id: generate_entry_id(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    summary: "Discussion about old topic".into(),
                    first_kept_entry_id: msg2_id.clone(),
                    tokens_before: 1000,
                    files_read: None,
                    files_modified: None,
                }),
            )
            .unwrap();

        // Recent message (should be included)
        let msg3_id = generate_entry_id();
        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(MessageEntry {
                    id: msg3_id,
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "recent message".into(),
                    }],
                    usage: None,
                }),
            )
            .unwrap();

        let ctx = build_context(&store, "test-session").unwrap();
        // Summary + msg2 (first_kept) + msg3
        assert_eq!(ctx.messages.len(), 3);
        assert_eq!(ctx.messages[0].role, Role::System);
        assert!(matches!(
            &ctx.messages[0].content[0],
            ContentBlock::Text { text } if text.contains("[上下文摘要]")
        ));
        assert_eq!(ctx.messages[1].role, Role::Assistant);
        assert_eq!(ctx.messages[2].role, Role::User);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_build_context_model_change() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "model-a", "/test")
            .unwrap();

        store
            .append_entry(
                "test-session",
                &SessionEntry::ModelChange(ModelChangeEntry {
                    id: generate_entry_id(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    provider: "openai".into(),
                    model_id: "gpt-4".into(),
                }),
            )
            .unwrap();

        let ctx = build_context(&store, "test-session").unwrap();
        assert_eq!(ctx.effective_model.as_deref(), Some("gpt-4"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_build_context_branch_summary() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "model", "/test")
            .unwrap();

        store
            .append_entry(
                "test-session",
                &SessionEntry::BranchSummary(BranchSummaryEntry {
                    id: generate_entry_id(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    from_id: "some_entry".into(),
                    summary: "Explored alternative approach".into(),
                }),
            )
            .unwrap();

        let ctx = build_context(&store, "test-session").unwrap();
        assert_eq!(ctx.messages.len(), 1);
        assert_eq!(ctx.messages[0].role, Role::System);
        assert!(matches!(
            &ctx.messages[0].content[0],
            ContentBlock::Text { text } if text.contains("Explored alternative")
        ));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_build_context_thinking_level_change() {
        use uncode_core::api_types::ThinkingLevel;

        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "model", "/test")
            .unwrap();

        store
            .append_entry(
                "test-session",
                &SessionEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
                    id: generate_entry_id(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    thinking_level: ThinkingLevel::High,
                }),
            )
            .unwrap();

        let ctx = build_context(&store, "test-session").unwrap();
        assert_eq!(ctx.effective_thinking_level, Some(ThinkingLevel::High));

        std::fs::remove_dir_all(dir).ok();
    }
}
