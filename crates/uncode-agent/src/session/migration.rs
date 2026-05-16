//! v1 → v2 session migration.
//!
//! v1 sessions have no `parent_id` on entries and header version = 1.
//! Migration chains `parent_id` fields sequentially and bumps version to 2.

use std::collections::HashMap;

use uncode_core::session::{SessionEntry, SessionHeader};

/// Migrate a v1 session to v2 in-place.
///
/// - Chains `parent_id` sequentially (each entry's parent is the previous entry)
/// - Sets `header.version = 2`
///
/// Idempotent: returns immediately if `header.version >= 2`.
pub fn migrate_v1_to_v2(
    header: &mut SessionHeader,
    by_id: &mut HashMap<String, SessionEntry>,
    order: &[String],
) {
    if header.version >= 2 {
        return;
    }

    let mut prev_id: Option<String> = None;
    for id in order {
        if let Some(entry) = by_id.get_mut(id) {
            if entry.parent_id().is_none() {
                if let Some(ref pid) = prev_id {
                    entry.set_parent_id(pid.clone());
                }
            }
            prev_id = Some(id.clone());
        }
    }

    header.version = 2;
    tracing::debug!("migrated session {} from v1 to v2", header.id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_core::message::{ContentBlock, Role};
    use uncode_core::session::{MessageEntry, generate_entry_id};

    fn v1_header() -> SessionHeader {
        let mut h = SessionHeader::new("test-v1".into(), "model".into(), "/test".into());
        h.version = 1; // Simulate a v1 header
        h
    }

    #[test]
    fn test_migration_chains_parent_ids() {
        let mut header = v1_header();
        assert_eq!(header.version, 1);

        let id1 = generate_entry_id();
        let id2 = generate_entry_id();
        let id3 = generate_entry_id();

        let mut by_id = HashMap::new();
        by_id.insert(
            id1.clone(),
            SessionEntry::Message(MessageEntry {
                id: id1.clone(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "first".into(),
                }],
                usage: None,
            }),
        );
        by_id.insert(
            id2.clone(),
            SessionEntry::Message(MessageEntry {
                id: id2.clone(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "second".into(),
                }],
                usage: None,
            }),
        );
        by_id.insert(
            id3.clone(),
            SessionEntry::Message(MessageEntry {
                id: id3.clone(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "third".into(),
                }],
                usage: None,
            }),
        );

        let order = vec![id1.clone(), id2.clone(), id3.clone()];

        migrate_v1_to_v2(&mut header, &mut by_id, &order);

        // Version bumped
        assert_eq!(header.version, 2);

        // First entry: no parent (root)
        assert!(by_id.get(&id1).unwrap().parent_id().is_none());

        // Second entry: parent = first
        assert_eq!(
            by_id.get(&id2).unwrap().parent_id().map(|s| s.to_string()),
            Some(id1.clone())
        );

        // Third entry: parent = second
        assert_eq!(
            by_id.get(&id3).unwrap().parent_id().map(|s| s.to_string()),
            Some(id2.clone())
        );
    }

    #[test]
    fn test_migration_idempotent() {
        let mut header = v1_header();
        header.version = 2;
        let mut by_id = HashMap::new();
        let order: Vec<String> = vec![];

        migrate_v1_to_v2(&mut header, &mut by_id, &order);

        assert_eq!(header.version, 2);
    }

    #[test]
    fn test_migration_preserves_existing_parent_ids() {
        let mut header = v1_header();
        let id1 = generate_entry_id();
        let id2 = generate_entry_id();
        let id3 = generate_entry_id();

        // id2 already has a parent_id set (e.g., from a partial v2 write)
        let mut by_id = HashMap::new();
        by_id.insert(
            id1.clone(),
            SessionEntry::Message(MessageEntry {
                id: id1.clone(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "first".into(),
                }],
                usage: None,
            }),
        );
        by_id.insert(
            id2.clone(),
            SessionEntry::Message(MessageEntry {
                id: id2.clone(),
                parent_id: Some(id1.clone()),
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "second".into(),
                }],
                usage: None,
            }),
        );
        by_id.insert(
            id3.clone(),
            SessionEntry::Message(MessageEntry {
                id: id3.clone(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "third".into(),
                }],
                usage: None,
            }),
        );

        let order = vec![id1.clone(), id2.clone(), id3.clone()];

        migrate_v1_to_v2(&mut header, &mut by_id, &order);

        // id2's parent_id was already set, should remain unchanged
        assert_eq!(
            by_id.get(&id2).unwrap().parent_id().map(|s| s.to_string()),
            Some(id1.clone())
        );

        // id3 should get id2 as parent
        assert_eq!(
            by_id.get(&id3).unwrap().parent_id().map(|s| s.to_string()),
            Some(id2.clone())
        );
    }

    #[test]
    fn test_migration_empty_session() {
        let mut header = v1_header();
        let mut by_id = HashMap::new();
        let order: Vec<String> = vec![];

        migrate_v1_to_v2(&mut header, &mut by_id, &order);

        assert_eq!(header.version, 2);
    }
}
