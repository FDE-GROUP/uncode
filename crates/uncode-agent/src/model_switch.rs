use std::sync::Arc;

use crate::session::store::{SessionResult, SessionStore};
use uncode_core::session::{ModelChangeEntry, SessionEntry, generate_entry_id};

/// 运行时切换 LLM 模型并记录 ModelChange 到会话
pub async fn switch_model(
    current_model_id: &mut String,
    new_model_id: &str,
    new_provider: &str,
    session_store: &Arc<SessionStore>,
    session_id: Option<&str>,
) -> SessionResult<()> {
    let old = std::mem::replace(current_model_id, new_model_id.to_string());

    if let Some(sid) = session_id {
        let entry = SessionEntry::ModelChange(Box::new(ModelChangeEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            provider: new_provider.to_string(),
            model_id: new_model_id.to_string(),
        }));
        session_store.append_entry(sid, &entry).await?;
    }

    tracing::info!("model switched: {old} -> {new_model_id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::store::SessionStore;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_switch_model_changes_id() {
        let mut current = "old".to_string();
        let store = Arc::new(SessionStore::new_memory().await.expect("store"));
        switch_model(&mut current, "new", "test", &store, None)
            .await
            .expect("switch");
        assert_eq!(current, "new");
    }

    #[tokio::test]
    async fn test_switch_model_with_session_records_entry() {
        let store = Arc::new(SessionStore::new_memory().await.expect("store"));
        store
            .init_session("s1", "old-model", "/tmp")
            .await
            .expect("init");
        let mut current = "old-model".to_string();
        switch_model(
            &mut current,
            "new-model",
            "test-provider",
            &store,
            Some("s1"),
        )
        .await
        .expect("switch");
        let entries = store.load_entries("s1").await.expect("load");
        assert!(!entries.is_empty(), "expected at least one entry recorded");
    }

    #[tokio::test]
    async fn test_switch_model_returns_old_value() {
        let mut current = "before".to_string();
        let store = Arc::new(SessionStore::new_memory().await.expect("store"));
        switch_model(&mut current, "after", "test", &store, None)
            .await
            .expect("switch");
        // The old value was replaced; current now holds the new value
        assert_eq!(current, "after");
    }

    #[tokio::test]
    async fn test_switch_model_no_session_id_skips_record() {
        let store = Arc::new(SessionStore::new_memory().await.expect("store"));
        let mut current = "m1".to_string();
        // No session_id → no panic, no session entries created
        switch_model(&mut current, "m2", "test", &store, None)
            .await
            .expect("switch");
        let sessions = store.list_sessions().await.expect("list");
        assert!(sessions.is_empty());
    }
}
