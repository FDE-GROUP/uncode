#[cfg(test)]
mod tests {
    use std::fs;

    use uncode_core::message::Message;
    use uncode_core::session::{MessageEntry, SessionEntry};

    use crate::manager::SessionManager;
    use crate::store::SessionStore;

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
        let entry = SessionEntry::Message(MessageEntry::from(msg));
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
            let entry = SessionEntry::Message(MessageEntry::from(msg));
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
                    &SessionEntry::Message(MessageEntry::from(Message::user("x")))
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
        assert_eq!(header.entry_type, "header");

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
        // 空文件加载 entries 返回空列表（不是错误）
        let entries = store.load_entries("empty").unwrap();
        assert!(entries.is_empty());

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
}
