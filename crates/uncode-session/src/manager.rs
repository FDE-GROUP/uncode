use uuid::Uuid;

use crate::store::{SessionResult, SessionStore};
use uncode_core::session::{SessionEntry, SessionMetadata};

pub struct SessionManager {
    store: SessionStore,
}

impl SessionManager {
    pub fn new(store: SessionStore) -> Self {
        Self { store }
    }

    pub fn list_sessions(&self) -> std::io::Result<Vec<SessionMetadata>> {
        self.store.list_sessions()
    }

    pub fn create_session(
        &self,
        model: &str,
        working_dir: &str,
        title: Option<String>,
    ) -> SessionResult<SessionMetadata> {
        let session_id = Uuid::new_v4().to_string();
        self.store.init_session(&session_id, model, working_dir)?;
        let mut header = self.store.read_header(&session_id)?;
        header.title = title;
        Ok(SessionMetadata::from(header))
    }

    pub fn append_entry(&self, session_id: &str, entry: SessionEntry) -> SessionResult<()> {
        self.store.append_entry(session_id, &entry)
    }

    pub fn load_entries(&self, session_id: &str) -> SessionResult<Vec<SessionEntry>> {
        self.store.load_entries(session_id)
    }

    pub fn get_metadata(&self, session_id: &str) -> SessionResult<SessionMetadata> {
        let header = self.store.read_header(session_id)?;
        Ok(SessionMetadata::from(header))
    }
}
