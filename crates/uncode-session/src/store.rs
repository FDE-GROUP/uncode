use std::io::{BufRead, Write};
use std::path::PathBuf;

use uncode_core::session::{
    SessionEntry, SessionHeader, SessionMetadata, SessionNode, SessionTree,
};

/// 会话存储层的错误类型
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

/// 会话存储后端，负责 JSONL 文件的读写操作
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

        if first_line.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "empty session file",
            ));
        }

        let header: SessionHeader = serde_json::from_str(&first_line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let md = std::fs::metadata(path)?;
        let mut meta = SessionMetadata::from(header);
        meta.updated_at =
            chrono::DateTime::<chrono::Utc>::from(md.modified().unwrap_or(std::time::UNIX_EPOCH));

        Ok(meta)
    }

    /// 返回最近更新的会话，按 updated_at 降序取第一个
    pub fn find_most_recent(&self) -> std::io::Result<Option<SessionMetadata>> {
        let mut sessions = self.list_sessions()?;
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions.into_iter().next())
    }

    pub fn init_session(
        &self,
        session_id: &str,
        model: &str,
        working_dir: &str,
    ) -> SessionResult<()> {
        self.init_session_with_title(session_id, model, working_dir, None)
    }

    pub fn init_session_with_title(
        &self,
        session_id: &str,
        model: &str,
        working_dir: &str,
        title: Option<String>,
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
        let header = match title {
            Some(t) => header.with_title(t),
            None => header,
        };
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

    /// 获取直接引用指定 session 为 parent 的子会话
    pub fn get_children(&self, session_id: &str) -> std::io::Result<Vec<SessionMetadata>> {
        let all = self.list_sessions()?;
        let mut children = Vec::new();
        for meta in all {
            if let Ok(entries) = self.load_entries(&meta.id) {
                for entry in &entries {
                    if let SessionEntry::Branch(be) = entry {
                        if be.parent_id == session_id {
                            children.push(meta);
                            break;
                        }
                    }
                }
            }
        }
        Ok(children)
    }

    /// 计算会话中的消息条数
    pub fn message_count(&self, session_id: &str) -> usize {
        self.load_entries(session_id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| matches!(e, SessionEntry::Message(_)))
                    .count()
            })
            .unwrap_or(0)
    }

    /// 构建以指定会话为根的分支树
    pub fn build_tree(&self, session_id: &str) -> std::io::Result<SessionTree> {
        let root = self.build_node(session_id)?;
        Ok(SessionTree { root })
    }

    fn build_node(&self, session_id: &str) -> std::io::Result<SessionNode> {
        let header = self
            .read_header(session_id)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string()))?;
        let msg_count = self.message_count(session_id);

        let children_meta = self.get_children(session_id)?;
        let mut children = Vec::new();
        for child in children_meta {
            children.push(self.build_node(&child.id)?);
        }

        Ok(SessionNode {
            id: session_id.to_string(),
            title: header.title,
            model: header.model,
            message_count: msg_count,
            children,
        })
    }

    /// 从指定会话 fork，返回新会话 ID
    pub fn fork_session(&self, parent_id: &str, reason: &str) -> SessionResult<String> {
        let header = self.read_header(parent_id)?;
        let new_id = uuid::Uuid::new_v4().to_string();

        self.init_session(&new_id, &header.model, &header.working_dir)?;

        let branch_entry = SessionEntry::Branch(uncode_core::session::BranchEntry {
            timestamp: chrono::Utc::now(),
            parent_id: parent_id.to_string(),
            reason: reason.to_string(),
        });
        self.append_entry(&new_id, &branch_entry)?;

        Ok(new_id)
    }
}
