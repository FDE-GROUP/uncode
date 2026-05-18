//! In-place branching with automatic summarization of abandoned branches.
//!
//! When the user moves the leaf pointer (via set_leaf), the entries between the
//! old leaf and the new leaf's common ancestor form an "abandoned branch". This
//! module generates a structured summary of that branch and appends a
//! BranchSummaryEntry before moving the leaf.

use crate::session::store::SessionStore;
use uncode_core::message::{ContentBlock, Role};
use uncode_core::session::{BranchSummaryEntry, SessionEntry, generate_entry_id};

/// Move the session leaf to a target entry, optionally summarizing the
/// abandoned branch between the old leaf and the common ancestor.
///
/// Returns `Ok(())` on success. If no old leaf exists, just sets the new leaf
/// without summarization.
pub async fn branch_with_summary(
    store: &SessionStore,
    session_id: &str,
    target_id: &str,
    reason: &str,
) -> anyhow::Result<()> {
    let old_leaf_id = store.get_leaf_id(session_id).await?;

    // Move the leaf
    store.set_leaf(session_id, target_id).await?;

    // Summarize the abandoned branch if there was a previous leaf
    if let Some(old_id) = old_leaf_id {
        if old_id != target_id {
            if let Ok(path) = store.get_path_to_root(session_id, &old_id).await {
                let summary = summarize_branch_entries(&path, reason);
                let entry = SessionEntry::BranchSummary(Box::new(BranchSummaryEntry {
                    id: generate_entry_id(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    from_id: old_id,
                    summary,
                }));
                store.append_entry(session_id, &entry).await?;
            }
        }
    }

    Ok(())
}

/// Generate a structured summary from a list of branch entries.
fn summarize_branch_entries(entries: &[SessionEntry], reason: &str) -> String {
    let mut goals = Vec::with_capacity(entries.len());
    let mut progress = Vec::with_capacity(entries.len());
    let mut decisions = Vec::with_capacity(entries.len());

    for entry in entries {
        if let SessionEntry::Message(me) = entry {
            let text = extract_summary_text(&me.content);
            if text.is_empty() {
                continue;
            }
            match me.role {
                Role::User => {
                    goals.push(text);
                }
                Role::Assistant => {
                    progress.push(text);
                }
                _ => {}
            }
        } else if let SessionEntry::Compaction(ce) = entry {
            decisions.push(ce.summary.clone());
        }
    }

    let mut parts = Vec::new();
    if !reason.is_empty() {
        parts.push(format!("分支原因: {reason}"));
    }
    if !goals.is_empty() {
        let g = goals.iter().take(3).cloned().collect::<Vec<_>>().join("; ");
        parts.push(format!("目标: {g}"));
    }
    if !progress.is_empty() {
        let p = progress
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
        parts.push(format!("进展: {p}"));
    }
    if !decisions.is_empty() {
        let d = decisions
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
        parts.push(format!("关键决策: {d}"));
    }

    if parts.is_empty() {
        "空分支".into()
    } else {
        parts.join("\n")
    }
}

fn extract_summary_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => {
                if text.len() > 200 {
                    let end = floor_char_boundary(text, 200);
                    Some(text[..end].to_string())
                } else {
                    Some(text.clone())
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn floor_char_boundary(s: &str, mut max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(max) {
        max -= 1;
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_core::session::*;

    #[tokio::test]
    async fn test_branch_with_summary_basic() {
        let store = SessionStore::new_memory().await.expect("store");
        store
            .init_session("test-session", "model", "/test")
            .await
            .unwrap();

        // Create a linear chain: msg1 → msg2 → msg3
        let msg1_id = generate_entry_id();
        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(Box::new(MessageEntry {
                    id: msg1_id.clone(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "implement auth".into(),
                    }],
                    usage: None,
                })),
            )
            .await
            .unwrap();

        let msg2_id = generate_entry_id();
        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(Box::new(MessageEntry {
                    id: msg2_id.clone(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text {
                        text: "added JWT tokens".into(),
                    }],
                    usage: None,
                })),
            )
            .await
            .unwrap();

        let msg3_id = generate_entry_id();
        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(Box::new(MessageEntry {
                    id: msg3_id.clone(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "try different approach".into(),
                    }],
                    usage: None,
                })),
            )
            .await
            .unwrap();

        // Branch back to msg1 (abandon msg2 and msg3)
        branch_with_summary(&store, "test-session", &msg1_id, "try alternative")
            .await
            .unwrap();

        // Verify: a LeafEntry targeting msg1 should exist
        let entries = store.load_entries("test-session").await.unwrap();
        let has_leaf = entries.iter().any(|e| {
            if let SessionEntry::Leaf(l) = e {
                l.target_id == msg1_id
            } else {
                false
            }
        });
        assert!(has_leaf, "expected a LeafEntry targeting msg1");

        // Verify: a BranchSummaryEntry should exist
        let entries = store.load_entries("test-session").await.unwrap();
        let has_summary = entries
            .iter()
            .any(|e| matches!(e, SessionEntry::BranchSummary(_)));
        assert!(has_summary, "expected a BranchSummaryEntry after branching");
    }

    #[tokio::test]
    async fn test_branch_no_previous_leaf() {
        let store = SessionStore::new_memory().await.expect("store");
        store
            .init_session("test-session", "model", "/test")
            .await
            .unwrap();

        // Only one entry, no previous leaf
        let msg1_id = generate_entry_id();
        store
            .append_entry(
                "test-session",
                &SessionEntry::Message(Box::new(MessageEntry {
                    id: msg1_id.clone(),
                    parent_id: None,
                    timestamp: chrono::Utc::now(),
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "hello".into(),
                    }],
                    usage: None,
                })),
            )
            .await
            .unwrap();

        // set_leaf on the same entry (no-op branch)
        branch_with_summary(&store, "test-session", &msg1_id, "test")
            .await
            .unwrap();

        // No BranchSummary should be created (old leaf == new target)
        let entries = store.load_entries("test-session").await.unwrap();
        let has_summary = entries
            .iter()
            .any(|e| matches!(e, SessionEntry::BranchSummary(_)));
        assert!(!has_summary);
    }

    #[test]
    fn test_summarize_branch_entries() {
        let entries = vec![
            SessionEntry::Message(Box::new(MessageEntry {
                id: "u1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "implement caching".into(),
                }],
                usage: None,
            })),
            SessionEntry::Message(Box::new(MessageEntry {
                id: "a1".into(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "added Redis cache layer".into(),
                }],
                usage: None,
            })),
        ];

        let summary = summarize_branch_entries(&entries, "performance issue");
        assert!(summary.contains("performance issue"));
        assert!(summary.contains("implement caching"));
        assert!(summary.contains("Redis cache"));
    }
}
