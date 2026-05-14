use std::io::{BufRead, Write};
use std::path::PathBuf;

use uncode_core::session::{SessionEntry, SessionHeader, SessionMetadata};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Session not found: {0}")]
    NotFound(String),
    #[error("Invalid session data: {0}")]
    InvalidData(String),
}

pub type SessionResult<T> = Result<T, SessionError>;

pub struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn default_dir() -> std::io::Result<PathBuf> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("uncode")
            .join("sessions");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn session_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{session_id}.jsonl"))
    }

    pub fn list_sessions(&self) -> std::io::Result<Vec<SessionMetadata>> {
        let mut sessions = Vec::new();
        if !self.base_dir.exists() {
            return Ok(sessions);
        }
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                if let Ok(meta) = self.read_metadata(&path) {
                    sessions.push(meta);
                }
            }
        }
        Ok(sessions)
    }

    fn read_metadata(&self, path: &std::path::Path) -> std::io::Result<SessionMetadata> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let first_line = reader.lines().next().transpose()?.unwrap_or_default();

        let header: SessionHeader = serde_json::from_str(&first_line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let _metadata = std::fs::metadata(path)?;

        Ok(SessionMetadata {
            id: header.id,
            created_at: header.created_at,
            updated_at: header.updated_at,
            message_count: 0,
            title: header.title,
            working_dir: header.working_dir,
            model: header.model,
        })
    }

    pub fn init_session(
        &self,
        session_id: &str,
        model: &str,
        working_dir: &str,
    ) -> SessionResult<()> {
        let path = self.session_path(session_id);
        if path.exists() {
            return Ok(());
        }

        let header = SessionHeader::new(
            session_id.to_string(),
            model.to_string(),
            working_dir.to_string(),
        );
        let line = serde_json::to_string(&header)?;
        let mut file = std::fs::File::create(&path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    pub fn append_entry(&self, session_id: &str, entry: &SessionEntry) -> SessionResult<()> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(SessionError::NotFound(session_id.to_string()));
        }
        let line = serde_json::to_string(entry)?;
        let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    pub fn load_entries(&self, session_id: &str) -> SessionResult<Vec<SessionEntry>> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(SessionError::NotFound(session_id.to_string()));
        }
        let file = std::fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);

        let mut entries = Vec::new();
        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if i == 0 || line.trim().is_empty() {
                continue; // skip header line
            }
            let entry: SessionEntry = serde_json::from_str(&line)?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub fn read_header(&self, session_id: &str) -> SessionResult<SessionHeader> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(SessionError::NotFound(session_id.to_string()));
        }
        let file = std::fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);
        let first_line = reader
            .lines()
            .next()
            .ok_or_else(|| SessionError::InvalidData("empty file".into()))??;
        let header: SessionHeader = serde_json::from_str(&first_line)?;
        Ok(header)
    }
}
