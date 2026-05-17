#[cfg(test)]
mod tests {
    use std::fs;

    use uncode_core::message::Message;
    use uncode_core::session::{MessageEntry, SessionEntry};

    use crate::session::manager::SessionManager;
    use crate::session::store::SessionStore;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("uncode-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_store_init_and_list() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        store
            .init_session("test-session", "deepseek-v3", "/test")
            .unwrap();
        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "test-session");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_init_idempotent() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        store.init_session("s1", "model", "/test").unwrap();
        store.init_session("s1", "model", "/test").unwrap();
        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_append_and_load() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "deepseek-v3", "/test")
            .unwrap();

        let msg = Message::user("hello world");
        let entry = SessionEntry::Message(Box::new(MessageEntry::from(msg)));
        store.append_entry("test-session", &entry).unwrap();

        let entries = store.load_entries("test-session").unwrap();
        assert_eq!(entries.len(), 1);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_multiple_entries() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("multi-session", "deepseek-v3", "/test")
            .unwrap();

        for i in 0..5 {
            let msg = Message::user(format!("message {i}"));
            let entry = SessionEntry::Message(Box::new(MessageEntry::from(msg)));
            store.append_entry("multi-session", &entry).unwrap();
        }

        let entries = store.load_entries("multi-session").unwrap();
        assert_eq!(entries.len(), 5);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_not_found() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        assert!(store.load_entries("nonexistent").is_err());
        assert!(store.read_header("nonexistent").is_err());
        assert!(
            store
                .append_entry(
                    "nonexistent",
                    &SessionEntry::Message(Box::new(MessageEntry::from(Message::user("x")))),
                )
                .is_err()
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_read_header() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("test-session", "deepseek-v3", "/test")
            .unwrap();

        let header = store.read_header("test-session").unwrap();
        assert_eq!(header.id, "test-session");
        assert_eq!(header.model, "deepseek-v3");
        assert_eq!(header.entry_type, "session");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_corrupted_jsonl() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        // 写一个损坏的 JSONL 文件
        let path = dir.join("corrupted.jsonl");
        fs::write(&path, "this is not valid json\n").unwrap();

        // list_sessions 应该跳过损坏的文件，不 panic
        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 0);

        // read_header 对损坏文件应返回错误
        assert!(store.read_header("corrupted").is_err());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_empty_jsonl() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        let path = dir.join("empty.jsonl");
        fs::write(&path, "").unwrap();

        // 空文件无法读取 header
        assert!(store.read_header("empty").is_err());
        // 空文件加载 entries 也是错误（无有效 header）
        assert!(store.load_entries("empty").is_err());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_store_list_skips_non_jsonl() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        // 非 .jsonl 文件不应出现在列表中
        fs::write(dir.join("readme.txt"), "not a session").unwrap();
        store
            .init_session("real-session", "model", "/test")
            .unwrap();

        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "real-session");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_manager_create_and_list() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        let manager = SessionManager::new(store);

        let meta = manager
            .create_session("deepseek-v3", "/test", Some("my session".into()))
            .unwrap();
        assert!(meta.title.as_deref() == Some("my session"));

        let sessions = manager.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_manager_create_without_title() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        let manager = SessionManager::new(store);

        let meta = manager
            .create_session("deepseek-v3", "/test", None)
            .unwrap();
        assert!(meta.title.is_none());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_manager_branch_session() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        let manager = SessionManager::new(store);

        let parent = manager
            .create_session("deepseek-v3", "/test", Some("parent".into()))
            .unwrap();

        let branch = manager
            .branch_session(&parent.id, "try alternative approach")
            .unwrap();

        assert_ne!(branch.id, parent.id);
        assert_eq!(branch.model, "deepseek-v3");

        let entries = manager.load_entries(&branch.id).unwrap();
        assert_eq!(entries.len(), 1);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_manager_get_metadata() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        let manager = SessionManager::new(store);

        let created = manager
            .create_session("glm-5.1", "/workspace", None)
            .unwrap();

        let loaded = manager.get_metadata(&created.id).unwrap();
        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.model, "glm-5.1");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_find_most_recent_empty() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        let result = store.find_most_recent().unwrap();
        assert!(result.is_none());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_find_most_recent_single() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        store
            .init_session("only-session", "deepseek-v3", "/test")
            .unwrap();

        let result = store.find_most_recent().unwrap();
        assert_eq!(result.unwrap().id, "only-session");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_find_most_recent_returns_latest() {
        use std::io::Write;

        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());

        // 创建第一个会话
        store
            .init_session("older-session", "deepseek-v3", "/test")
            .unwrap();

        // 稍等后创建第二个会话（updated_at 更新）
        std::thread::sleep(std::time::Duration::from_millis(50));
        store
            .init_session("newer-session", "glm-5.1", "/test")
            .unwrap();

        // 手动 touch 第一个文件使其更新时间更晚
        std::thread::sleep(std::time::Duration::from_millis(50));
        let older_path = dir.join("older-session.jsonl");
        let content = fs::read_to_string(&older_path).unwrap();
        {
            let mut f = fs::File::create(&older_path).unwrap();
            write!(f, "{content}").unwrap();
        }

        let result = store.find_most_recent().unwrap();
        assert_eq!(result.unwrap().id, "older-session");

        fs::remove_dir_all(dir).ok();
    }

    // ── Tree operation tests ──

    #[test]
    fn test_append_entry_auto_parent_id() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store.init_session("tree-test", "model", "/test").unwrap();

        let e1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("first"))));
        store.append_entry("tree-test", &e1).unwrap();

        let e2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("second"))));
        store.append_entry("tree-test", &e2).unwrap();

        let entries = store.load_entries("tree-test").unwrap();
        // First entry has no parent (root)
        assert!(entries[0].parent_id().is_none());
        // Second entry's parent should be first entry's id
        assert_eq!(
            entries[1].parent_id().map(|s| s.to_string()),
            Some(entries[0].entry_id().to_string())
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_get_leaf_id_initial() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store.init_session("leaf-test", "model", "/test").unwrap();

        let e = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("hello"))));
        store.append_entry("leaf-test", &e).unwrap();

        let leaf = store.get_leaf_id("leaf-test").unwrap();
        assert!(leaf.is_some());

        let entries = store.load_entries("leaf-test").unwrap();
        assert_eq!(leaf.as_deref(), Some(entries[0].entry_id()));

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_set_leaf_moves_pointer() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store
            .init_session("set-leaf-test", "model", "/test")
            .unwrap();

        let e1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("first"))));
        store.append_entry("set-leaf-test", &e1).unwrap();
        let e1_id = store
            .get_entry(
                "set-leaf-test",
                store
                    .get_leaf_id("set-leaf-test")
                    .unwrap()
                    .unwrap()
                    .as_str(),
            )
            .unwrap()
            .unwrap()
            .entry_id()
            .to_string();

        let e2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("second"))));
        store.append_entry("set-leaf-test", &e2).unwrap();

        // Leaf should be on e2
        let leaf = store.get_leaf_id("set-leaf-test").unwrap();
        assert_ne!(leaf.as_deref(), Some(e1_id.as_str()));

        // Move leaf back to e1
        store.set_leaf("set-leaf-test", &e1_id).unwrap();

        // Verify a LeafEntry was created targeting e1
        let entries = store.load_entries("set-leaf-test").unwrap();
        let has_leaf = entries.iter().any(|e| {
            if let SessionEntry::Leaf(l) = e {
                l.target_id == e1_id
            } else {
                false
            }
        });
        assert!(has_leaf);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_get_entry_found_and_missing() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store.init_session("entry-test", "model", "/test").unwrap();

        let e = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("hello"))));
        store.append_entry("entry-test", &e).unwrap();

        let entries = store.load_entries("entry-test").unwrap();
        let id = entries[0].entry_id().to_string();

        assert!(store.get_entry("entry-test", &id).unwrap().is_some());
        assert!(
            store
                .get_entry("entry-test", "nonexistent")
                .unwrap()
                .is_none()
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_get_path_to_root_linear() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        store.init_session("path-test", "model", "/test").unwrap();

        let e1 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("a"))));
        store.append_entry("path-test", &e1).unwrap();
        let e2 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("b"))));
        store.append_entry("path-test", &e2).unwrap();
        let e3 = SessionEntry::Message(Box::new(MessageEntry::from(Message::user("c"))));
        store.append_entry("path-test", &e3).unwrap();

        let entries = store.load_entries("path-test").unwrap();
        let e3_id = entries[2].entry_id().to_string();

        let path = store.get_path_to_root("path-test", &e3_id).unwrap();
        // Path from leaf to root: e3 → e2 → e1
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].entry_id(), entries[2].entry_id());
        assert_eq!(path[1].entry_id(), entries[1].entry_id());
        assert_eq!(path[2].entry_id(), entries[0].entry_id());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_v1_jsonl_end_to_end_migration() {
        use std::io::Write;

        let dir = temp_dir();
        let path = dir.join("v1-session.jsonl");

        // Write a v1-style JSONL: header (version=1) + entries without parent_id
        let header_json = r#"{"type":"session","id":"v1-session","version":1,"created_at":"2025-01-01T00:00:00Z","updated_at":"2025-01-01T00:00:00Z","model":"test-model","working_dir":"/test"}"#;
        let entry1 = r#"{"type":"message","id":"aaa111","timestamp":"2025-01-01T00:00:01Z","role":"user","content":[{"type":"text","text":"hello"}]}"#;
        let entry2 = r#"{"type":"message","id":"bbb222","timestamp":"2025-01-01T00:00:02Z","role":"assistant","content":[{"type":"text","text":"world"}]}"#;

        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "{header_json}").unwrap();
            writeln!(f, "{entry1}").unwrap();
            writeln!(f, "{entry2}").unwrap();
        }

        // Load via SessionStore — should auto-migrate
        let store = SessionStore::new(dir.clone());
        let header = store.read_header("v1-session").unwrap();
        assert_eq!(header.version, 2, "version should be migrated to 2");

        let entries = store.load_entries("v1-session").unwrap();
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
            .unwrap();
        assert_eq!(path.len(), 2);

        fs::remove_dir_all(dir).ok();
    }
}
