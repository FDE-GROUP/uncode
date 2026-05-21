#[cfg(test)]
mod tests {
    use uncode_core::message::Message;
    use uncode_core::session::{MessageEntry, SessionEntry};

    use crate::session::manager::SessionManager;
    use crate::session::store::SessionStore;

    async fn new_store() -> SessionStore {
        SessionStore::new_memory().await.unwrap()
    }

    #[tokio::test]
    async fn test_store_init_and_list() {
        let store = new_store().await;

        store
            .init_session("test-session", "deepseek-v3", "/test")
            .await
            .unwrap();
        let sessions = store.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "test-session");
    }

    #[tokio::test]
    async fn test_store_init_idempotent() {
        let store = new_store().await;

        store.init_session("s1", "model", "/test").await.unwrap();
        store.init_session("s1", "model", "/test").await.unwrap();
        let sessions = store.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_store_append_and_load() {
        let store = new_store().await;
        store
            .init_session("test-session", "deepseek-v3", "/test")
            .await
            .unwrap();

        let msg = Message::user("hello world");
        let entry = SessionEntry::Message(Box::new(MessageEntry::from(msg)));
        store.append_entry("test-session", &entry).await.unwrap();

        let entries = store.load_entries("test-session").await.unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn test_store_multiple_entries() {
        let store = new_store().await;
        store
            .init_session("multi-session", "deepseek-v3", "/test")
            .await
            .unwrap();

        for i in 0..5 {
            let msg = Message::user(format!("message {i}"));
            let entry = SessionEntry::Message(Box::new(MessageEntry::from(msg)));
            store.append_entry("multi-session", &entry).await.unwrap();
        }

        let entries = store.load_entries("multi-session").await.unwrap();
        assert_eq!(entries.len(), 5);
    }

    #[tokio::test]
    async fn test_store_not_found() {
        let store = new_store().await;
        assert!(store.load_entries("nonexistent").await.is_err());
        assert!(store.read_header("nonexistent").await.is_err());
        assert!(
            store
                .append_entry(
                    "nonexistent",
                    &SessionEntry::Message(Box::new(MessageEntry::from(Message::user("x")))),
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_store_read_header() {
        let store = new_store().await;
        store
            .init_session("test-session", "deepseek-v3", "/test")
            .await
            .unwrap();

        let header = store.read_header("test-session").await.unwrap();
        assert_eq!(header.id, "test-session");
        assert_eq!(header.model, "deepseek-v3");
        assert_eq!(header.entry_type, "session");
    }

    #[tokio::test]
    async fn test_store_nonexistent_session() {
        let store = new_store().await;

        // For SurrealDB, reading a nonexistent session should return NotFound
        assert!(store.read_header("nonexistent").await.is_err());
        assert!(store.load_entries("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_store_empty_list() {
        let store = new_store().await;

        // Empty store lists zero sessions
        let sessions = store.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 0);
    }

    #[tokio::test]
    async fn test_store_list_only_real_sessions() {
        let store = new_store().await;

        store
            .init_session("real-session", "model", "/test")
            .await
            .unwrap();

        let sessions = store.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "real-session");
    }

    #[tokio::test]
    async fn test_manager_create_and_list() {
        let store = new_store().await;
        let manager = SessionManager::new(store);

        let meta = manager
            .create_session("deepseek-v3", "/test", Some("my session".into()))
            .await
            .unwrap();
        assert!(meta.title.as_deref() == Some("my session"));

        let sessions = manager.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_manager_create_without_title() {
        let store = new_store().await;
        let manager = SessionManager::new(store);

        let meta = manager
            .create_session("deepseek-v3", "/test", None)
            .await
            .unwrap();
        assert!(meta.title.is_none());
    }

    #[tokio::test]
    async fn test_manager_branch_session() {
        let store = new_store().await;
        let manager = SessionManager::new(store);

        let parent = manager
            .create_session("deepseek-v3", "/test", Some("parent".into()))
            .await
            .unwrap();

        let branch = manager
            .branch_session(&parent.id, "try alternative approach")
            .await
            .unwrap();

        assert_ne!(branch.id, parent.id);
        assert_eq!(branch.model, "deepseek-v3");

        let entries = manager.load_entries(&branch.id).await.unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn test_manager_get_metadata() {
        let store = new_store().await;
        let manager = SessionManager::new(store);

        let created = manager
            .create_session("glm-5.1", "/workspace", None)
            .await
            .unwrap();

        let loaded = manager.get_metadata(&created.id).await.unwrap();
        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.model, "glm-5.1");
    }

    #[tokio::test]
    async fn test_find_most_recent_empty() {
        let store = new_store().await;

        let result = store.find_most_recent().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_find_most_recent_single() {
        let store = new_store().await;

        store
            .init_session("only-session", "deepseek-v3", "/test")
            .await
            .unwrap();

        let result = store.find_most_recent().await.unwrap();
        assert_eq!(result.unwrap().id, "only-session");
    }

    #[tokio::test]
    async fn test_find_most_recent_returns_latest() {
        let store = new_store().await;

        // 创建第一个会话
        store
            .init_session("older-session", "deepseek-v3", "/test")
            .await
            .unwrap();

        // 稍等后创建第二个会话（updated_at 更新）
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        store
            .init_session("newer-session", "glm-5.1", "/test")
            .await
            .unwrap();

        let result = store.find_most_recent().await.unwrap();
        assert_eq!(result.unwrap().id, "newer-session");
    }

    // ── Tree operation tests ──

    #[tokio::test]
    async fn test_append_entry_auto_parent_id() {
        let store = new_store().await;
        store
            .init_session("tree-test", "model", "/test")
            .await
            .unwrap();

        let e1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("first"))));
        store.append_entry("tree-test", &e1).await.unwrap();

        let e2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("second"))));
        store.append_entry("tree-test", &e2).await.unwrap();

        let entries = store.load_entries("tree-test").await.unwrap();
        // First entry has no parent (root)
        assert!(entries[0].parent_id().is_none());
        // Second entry's parent should be first entry's id
        assert_eq!(
            entries[1].parent_id().map(|s| s.to_string()),
            Some(entries[0].entry_id().to_string())
        );
    }

    #[tokio::test]
    async fn test_get_leaf_id_initial() {
        let store = new_store().await;
        store
            .init_session("leaf-test", "model", "/test")
            .await
            .unwrap();

        let e = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("hello"))));
        store.append_entry("leaf-test", &e).await.unwrap();

        let leaf = store.get_leaf_id("leaf-test").await.unwrap();
        assert!(leaf.is_some());

        let entries = store.load_entries("leaf-test").await.unwrap();
        assert_eq!(leaf.as_deref(), Some(entries[0].entry_id()));
    }

    #[tokio::test]
    async fn test_set_leaf_moves_pointer() {
        let store = new_store().await;
        store
            .init_session("set-leaf-test", "model", "/test")
            .await
            .unwrap();

        let e1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("first"))));
        store.append_entry("set-leaf-test", &e1).await.unwrap();
        let e1_id = store
            .get_entry(
                "set-leaf-test",
                store
                    .get_leaf_id("set-leaf-test")
                    .await
                    .unwrap()
                    .unwrap()
                    .as_str(),
            )
            .await
            .unwrap()
            .unwrap()
            .entry_id()
            .to_string();

        let e2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("second"))));
        store.append_entry("set-leaf-test", &e2).await.unwrap();

        // Leaf should be on e2
        let leaf = store.get_leaf_id("set-leaf-test").await.unwrap();
        assert_ne!(leaf.as_deref(), Some(e1_id.as_str()));

        // Move leaf back to e1
        store.set_leaf("set-leaf-test", &e1_id).await.unwrap();

        // Verify a LeafEntry was created targeting e1
        let entries = store.load_entries("set-leaf-test").await.unwrap();
        let has_leaf = entries.iter().any(|e| {
            if let SessionEntry::Leaf(l) = e {
                l.target_id == e1_id
            } else {
                false
            }
        });
        assert!(has_leaf);
    }

    #[tokio::test]
    async fn test_get_entry_found_and_missing() {
        let store = new_store().await;
        store
            .init_session("entry-test", "model", "/test")
            .await
            .unwrap();

        let e = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("hello"))));
        store.append_entry("entry-test", &e).await.unwrap();

        let entries = store.load_entries("entry-test").await.unwrap();
        let id = entries[0].entry_id().to_string();

        assert!(store.get_entry("entry-test", &id).await.unwrap().is_some());
        assert!(
            store
                .get_entry("entry-test", "nonexistent")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_get_path_to_root_linear() {
        let store = new_store().await;
        store
            .init_session("path-test", "model", "/test")
            .await
            .unwrap();

        let e1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("a"))));
        store.append_entry("path-test", &e1).await.unwrap();
        let e2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("b"))));
        store.append_entry("path-test", &e2).await.unwrap();
        let e3 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("c"))));
        store.append_entry("path-test", &e3).await.unwrap();

        let entries = store.load_entries("path-test").await.unwrap();
        let e3_id = entries[2].entry_id().to_string();

        let path = store.get_path_to_root("path-test", &e3_id).await.unwrap();
        // Path from leaf to root: e3 -> e2 -> e1
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].entry_id(), entries[2].entry_id());
        assert_eq!(path[1].entry_id(), entries[1].entry_id());
        assert_eq!(path[2].entry_id(), entries[0].entry_id());
    }

    #[tokio::test]
    async fn test_v1_jsonl_end_to_end_migration() {
        let store = new_store().await;

        // For SurrealDB backend, there is no v1 JSONL migration path.
        // Instead, verify that a freshly created session has version 2
        // and tree operations work correctly.
        store
            .init_session("v1-session", "test-model", "/test")
            .await
            .unwrap();

        let e1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("hello"))));
        store.append_entry("v1-session", &e1).await.unwrap();
        let e2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("world"))));
        store.append_entry("v1-session", &e2).await.unwrap();

        let header = store.read_header("v1-session").await.unwrap();
        assert_eq!(
            header.version, 2,
            "version should be 2 for SurrealDB backend"
        );

        let entries = store.load_entries("v1-session").await.unwrap();
        assert_eq!(entries.len(), 2);
        // First entry: no parent (root)
        assert!(entries[0].parent_id().is_none());
        // Second entry: parent = first entry's id
        assert_eq!(
            entries[1].parent_id().map(|s| s.to_string()),
            Some(entries[0].entry_id().to_string())
        );

        // get_path_to_root should work
        let path = store
            .get_path_to_root("v1-session", entries[1].entry_id())
            .await
            .unwrap();
        assert_eq!(path.len(), 2);
    }

    // ── Undo Turn tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_undo_turn_removes_last_user_message() {
        let store = new_store().await;
        store
            .init_session("undo-test", "model", "/test")
            .await
            .unwrap();

        // U1 → A1 → U2 → A2
        let u1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("u1"))));
        store.append_entry("undo-test", &u1).await.unwrap();
        let a1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::assistant("a1"))));
        store.append_entry("undo-test", &a1).await.unwrap();
        let u2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("u2"))));
        store.append_entry("undo-test", &u2).await.unwrap();
        let a2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::assistant("a2"))));
        store.append_entry("undo-test", &a2).await.unwrap();

        // Undo 1 turn → leaf moves back to A1
        let target = store.undo_turn("undo-test", 1).await.unwrap();

        // Verify leaf points to A1
        let entries = store.load_entries("undo-test").await.unwrap();
        let a1_id = entries[1].entry_id();
        assert_eq!(target, a1_id);
    }

    #[tokio::test]
    async fn test_undo_turn_multiple() {
        let store = new_store().await;
        store
            .init_session("undo-multi", "model", "/test")
            .await
            .unwrap();

        // U1 → A1 → U2 → A2
        let u1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("u1"))));
        store.append_entry("undo-multi", &u1).await.unwrap();
        let a1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::assistant("a1"))));
        store.append_entry("undo-multi", &a1).await.unwrap();
        let u2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("u2"))));
        store.append_entry("undo-multi", &u2).await.unwrap();
        let a2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::assistant("a2"))));
        store.append_entry("undo-multi", &a2).await.unwrap();

        // Undo 2 turns → nothing to point to (before first entry)
        let result = store.undo_turn("undo-multi", 2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_undo_turn_empty_session() {
        let store = new_store().await;
        store
            .init_session("undo-empty", "model", "/test")
            .await
            .unwrap();

        let result = store.undo_turn("undo-empty", 1).await;
        assert!(result.is_err());
    }

    // ── Search / Filter tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_search_sessions_by_title() {
        let store = new_store().await;

        store
            .init_session_with_title(
                "s1",
                "model",
                "/test",
                Some("Rust refactoring session".into()),
            )
            .await
            .unwrap();
        store
            .init_session_with_title("s2", "model", "/test", Some("Python debugging".into()))
            .await
            .unwrap();

        let results = store.search_sessions("rust").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "s1");
    }

    #[tokio::test]
    async fn test_search_sessions_case_insensitive() {
        let store = new_store().await;

        store
            .init_session_with_title("s1", "model", "/test", Some("Rust Session".into()))
            .await
            .unwrap();

        let results = store.search_sessions("rust").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_search_sessions_no_match() {
        let store = new_store().await;

        store
            .init_session_with_title("s1", "model", "/test", Some("Rust session".into()))
            .await
            .unwrap();

        let results = store.search_sessions("python").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_list_sessions_by_model() {
        let store = new_store().await;

        store
            .init_session("s1", "deepseek-v3", "/test")
            .await
            .unwrap();
        store.init_session("s2", "glm-5.1", "/test").await.unwrap();
        store
            .init_session("s3", "deepseek-v3", "/test")
            .await
            .unwrap();

        let results = store.list_sessions_by_model("deepseek-v3").await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_update_title() {
        let store = new_store().await;
        store.init_session("s1", "model", "/test").await.unwrap();

        store.update_title("s1", "New Title").await.unwrap();

        let header = store.read_header("s1").await.unwrap();
        assert_eq!(header.title.as_deref(), Some("New Title"));
    }
}
