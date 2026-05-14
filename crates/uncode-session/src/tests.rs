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
    fn test_store_not_found() {
        let dir = temp_dir();
        let store = SessionStore::new(dir.clone());
        assert!(store.load_entries("nonexistent").is_err());
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
}
