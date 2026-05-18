use uuid::Uuid;

use super::store::{SessionResult, SessionStore};
use uncode_core::session::{SessionEntry, SessionMetadata};

pub struct SessionManager {
    store: SessionStore,
}

impl SessionManager {
    pub fn new(store: SessionStore) -> Self {
        Self { store }
    }

    pub async fn list_sessions(&self) -> SessionResult<Vec<SessionMetadata>> {
        self.store.list_sessions().await
    }

    pub async fn create_session(
        &self,
        model: &str,
        working_dir: &str,
        title: Option<String>,
    ) -> SessionResult<SessionMetadata> {
        let session_id = Uuid::new_v4().to_string();
        self.store
            .init_session_with_title(&session_id, model, working_dir, title)
            .await?;
        let header = self.store.read_header(&session_id).await?;
        Ok(SessionMetadata::from(header))
    }

    pub async fn append_entry(&self, session_id: &str, entry: SessionEntry) -> SessionResult<()> {
        self.store.append_entry(session_id, &entry).await
    }

    pub async fn load_entries(&self, session_id: &str) -> SessionResult<Vec<SessionEntry>> {
        self.store.load_entries(session_id).await
    }

    pub async fn get_metadata(&self, session_id: &str) -> SessionResult<SessionMetadata> {
        let header = self.store.read_header(session_id).await?;
        Ok(SessionMetadata::from(header))
    }

    pub async fn branch_session(
        &self,
        parent_id: &str,
        reason: &str,
    ) -> SessionResult<SessionMetadata> {
        let new_id = Uuid::new_v4().to_string();
        let parent = self.store.read_header(parent_id).await?;
        self.store
            .init_session(&new_id, &parent.model, &parent.working_dir)
            .await?;

        let branch = uncode_core::session::BranchEntry {
            id: uncode_core::session::generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            parent_session_id: parent_id.to_string(),
            reason: reason.to_string(),
        };

        self.store
            .append_entry(
                &new_id,
                &uncode_core::session::SessionEntry::Branch(Box::new(branch)),
            )
            .await?;

        self.get_metadata(&new_id).await
    }
}
