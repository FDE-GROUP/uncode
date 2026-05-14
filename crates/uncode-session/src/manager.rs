use anyhow::Context;
use uncode_core::session::{SessionEntry, SessionMetadata};

pub struct SessionManager {
    store: crate::store::SessionStore,
}

impl SessionManager {
    pub fn new(store: crate::store::SessionStore) -> Self {
        Self { store }
    }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<SessionMetadata>> {
        self.store.list().await
    }

    pub async fn create_session(&self, title: Option<String>) -> anyhow::Result<SessionMetadata> {
        self.store.create(title).await
    }

    pub async fn append_entry(
        &self,
        session_id: &str,
        entry: SessionEntry,
    ) -> anyhow::Result<()> {
        self.store
            .append(session_id, entry)
            .await
            .context("Failed to append session entry")
    }

    pub async fn load_entries(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Vec<SessionEntry>> {
        self.store.load(session_id).await
    }
}
