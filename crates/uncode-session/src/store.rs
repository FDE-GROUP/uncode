use chrono::Utc;
use uncode_core::session::{SessionEntry, SessionMetadata};

pub struct SessionStore {
    base_dir: std::path::PathBuf,
}

impl SessionStore {
    pub fn new(base_dir: std::path::PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn default_dir() -> anyhow::Result<std::path::PathBuf> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("uncode")
            .join("sessions");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub async fn list(&self) -> anyhow::Result<Vec<SessionMetadata>> {
        let mut sessions = Vec::new();
        if self.base_dir.exists() {
            for entry in std::fs::read_dir(&self.base_dir)? {
                let entry = entry?;
                if entry.path().extension().map_or(false, |e| e == "jsonl") {
                    let metadata = entry.metadata()?;
                    sessions.push(SessionMetadata {
                        id: entry
                            .path()
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        created_at: metadata.created()?.into(),
                        updated_at: metadata.modified()?.into(),
                        message_count: 0,
                        title: None,
                    });
                }
            }
        }
        Ok(sessions)
    }

    pub async fn create(&self, _title: Option<String>) -> anyhow::Result<SessionMetadata> {
        let id = uuid::Uuid::new_v4().to_string();
        let file_path = self.base_dir.join(format!("{}.jsonl", id));
        std::fs::File::create(&file_path)?;

        Ok(SessionMetadata {
            id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 0,
            title: None,
        })
    }

    pub async fn append(&self, session_id: &str, entry: SessionEntry) -> anyhow::Result<()> {
        use std::io::Write;

        let file_path = self.base_dir.join(format!("{}.jsonl", session_id));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&file_path)?;

        let line = serde_json::to_string(&entry)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    pub async fn load(&self, session_id: &str) -> anyhow::Result<Vec<SessionEntry>> {
        use std::io::BufRead;

        let file_path = self.base_dir.join(format!("{}.jsonl", session_id));
        let file = std::fs::File::open(&file_path)?;
        let reader = std::io::BufReader::new(file);

        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: SessionEntry = serde_json::from_str(&line)?;
            entries.push(entry);
        }
        Ok(entries)
    }
}
