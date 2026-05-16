use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use parking_lot::RwLock;
use uncode_core::session::{
    LeafEntry, SessionEntry, SessionHeader, SessionMetadata, SessionNode, SessionTree,
    generate_entry_id,
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

/// 内存中的会话状态
struct SessionState {
    header: SessionHeader,
    by_id: HashMap<String, SessionEntry>,
    order: Vec<String>,
    leaf_id: Option<String>,
}

/// 会话存储后端，JSONL 文件 + 内存索引，对齐 Pi 的 SessionStorage
pub struct SessionStore {
    base_dir: PathBuf,
    sessions: RwLock<HashMap<String, SessionState>>,
}

impl SessionStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            sessions: RwLock::new(HashMap::new()),
        }
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

    // ── 懒加载 ──

    /// 从文件加载会话到内存。已加载则跳过。
    fn ensure_loaded(&self, session_id: &str) -> SessionResult<()> {
        if self.sessions.read().contains_key(session_id) {
            return Ok(());
        }

        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(SessionError::NotFound(session_id.to_string()));
        }

        let file = std::fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);
        let lines: Vec<_> = reader.lines().collect::<Result<_, _>>()?;

        if lines.is_empty() {
            return Err(SessionError::InvalidData("empty file".into()));
        }

        let header: SessionHeader = serde_json::from_str(&lines[0])
            .map_err(|e| SessionError::InvalidData(format!("header parse: {e}")))?;

        let mut by_id = HashMap::new();
        let mut order = Vec::new();
        let mut leaf_id: Option<String> = None;

        for (i, line) in lines.iter().enumerate() {
            if i == 0 || line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<SessionEntry>(line) {
                let id = entry.entry_id().to_string();

                // v1 entries have parent_id=None via serde default.
                // get_path_to_root stops at parent_id=None (treated as root),
                // which correctly models v1's linear sequence.

                if let SessionEntry::Leaf(leaf) = &entry {
                    leaf_id = Some(leaf.target_id.clone());
                }

                by_id.insert(id.clone(), entry);
                order.push(id);
            }
        }

        // 无 LeafEntry 则 leaf = 最后一个条目
        if leaf_id.is_none() {
            leaf_id = order.last().cloned();
        }

        let mut state = SessionState {
            header,
            by_id,
            order,
            leaf_id,
        };

        // v1 自动迁移
        if state.header.version < 2 {
            super::migration::migrate_v1_to_v2(&mut state.header, &mut state.by_id, &state.order);
        }

        // CWD 校验
        if !std::path::Path::new(&state.header.working_dir).exists() {
            tracing::warn!(
                "session {} working_dir '{}' no longer exists",
                session_id,
                state.header.working_dir
            );
        }

        self.sessions.write().insert(session_id.to_string(), state);
        Ok(())
    }

    // ── 树导航 (对齐 Pi SessionStorage) ──

    /// 获取当前叶条目 ID（对齐 Pi `getLeafId`）
    pub fn get_leaf_id(&self, session_id: &str) -> SessionResult<Option<String>> {
        self.ensure_loaded(session_id)?;
        let sessions = self.sessions.read();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;
        Ok(state.leaf_id.clone())
    }

    /// 移动叶指针到指定条目（对齐 Pi `setLeafId`）
    pub fn set_leaf(&self, session_id: &str, target_id: &str) -> SessionResult<()> {
        self.ensure_loaded(session_id)?;

        {
            let sessions = self.sessions.read();
            let state = sessions
                .get(session_id)
                .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;
            if !state.by_id.contains_key(target_id) {
                return Err(SessionError::InvalidData(format!(
                    "target entry not found: {target_id}"
                )));
            }
        }

        let leaf = LeafEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            target_id: target_id.to_string(),
        };

        self.append_entry(session_id, &SessionEntry::Leaf(leaf))?;
        Ok(())
    }

    /// 获取指定 ID 的条目（对齐 Pi `getEntry`）
    pub fn get_entry(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> SessionResult<Option<SessionEntry>> {
        self.ensure_loaded(session_id)?;
        let sessions = self.sessions.read();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;
        Ok(state.by_id.get(entry_id).cloned())
    }

    /// 从指定条目回溯到根（对齐 Pi `getPathToRoot`）
    pub fn get_path_to_root(
        &self,
        session_id: &str,
        from_id: &str,
    ) -> SessionResult<Vec<SessionEntry>> {
        self.ensure_loaded(session_id)?;
        let sessions = self.sessions.read();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        let mut path = Vec::new();
        let mut current = Some(from_id.to_string());
        let mut depth = 0u32;
        const MAX_DEPTH: u32 = 10000;

        while let Some(id) = current {
            if depth >= MAX_DEPTH {
                tracing::warn!("get_path_to_root: exceeded max depth, possible cycle");
                break;
            }
            depth += 1;
            if let Some(entry) = state.by_id.get(&id) {
                current = entry.parent_id().map(|s| s.to_string());
                path.push(entry.clone());
            } else {
                break;
            }
        }

        Ok(path)
    }

    // ── 现有 API（保持向后兼容） ──

    pub fn list_sessions(&self) -> std::io::Result<Vec<SessionMetadata>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let dir_entries = std::fs::read_dir(&self.base_dir)?.collect::<Vec<_>>();
        let mut sessions = Vec::with_capacity(dir_entries.len());
        for entry in dir_entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                if let Ok(meta) = self.read_metadata_from_file(&path) {
                    sessions.push(meta);
                }
            }
        }
        Ok(sessions)
    }

    fn read_metadata_from_file(&self, path: &std::path::Path) -> std::io::Result<SessionMetadata> {
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

        if header.entry_type != "session" && header.entry_type != "header" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid session type: {}", header.entry_type),
            ));
        }

        let md = std::fs::metadata(path)?;
        let mut meta = SessionMetadata::from(header);
        meta.updated_at =
            chrono::DateTime::<chrono::Utc>::from(md.modified().unwrap_or(std::time::UNIX_EPOCH));

        Ok(meta)
    }

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

        // 预加载到内存
        let state = SessionState {
            header,
            by_id: HashMap::new(),
            order: Vec::new(),
            leaf_id: None,
        };
        self.sessions.write().insert(session_id.to_string(), state);

        Ok(())
    }

    pub fn append_entry(&self, session_id: &str, entry: &SessionEntry) -> SessionResult<()> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(SessionError::NotFound(session_id.to_string()));
        }

        // Auto-set parent_id to current leaf for tree structure
        let mut entry = entry.clone();
        {
            let sessions = self.sessions.read();
            if let Some(state) = sessions.get(session_id) {
                if entry.parent_id().is_none() {
                    if let Some(ref leaf) = state.leaf_id {
                        entry.set_parent_id(leaf.clone());
                    }
                }
            }
        }

        let line = serde_json::to_string(&entry)?;
        let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
        writeln!(file, "{line}")?;

        // 更新内存索引
        let entry_id = entry.entry_id().to_string();
        let mut sessions = self.sessions.write();
        if let Some(state) = sessions.get_mut(session_id) {
            // LeafEntry 更新 leaf 指针但不改变 leaf_id 为自身
            if let SessionEntry::Leaf(leaf) = &entry {
                state.leaf_id = Some(leaf.target_id.clone());
            } else {
                state.leaf_id = Some(entry_id.clone());
            }
            state.by_id.insert(entry_id.clone(), entry);
            state.order.push(entry_id);
        }

        Ok(())
    }

    pub fn load_entries(&self, session_id: &str) -> SessionResult<Vec<SessionEntry>> {
        self.ensure_loaded(session_id)?;
        let sessions = self.sessions.read();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;
        Ok(state
            .order
            .iter()
            .filter_map(|id| state.by_id.get(id).cloned())
            .collect())
    }

    pub fn read_header(&self, session_id: &str) -> SessionResult<SessionHeader> {
        self.ensure_loaded(session_id)?;
        let sessions = self.sessions.read();
        let state = sessions
            .get(session_id)
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;
        Ok(state.header.clone())
    }

    pub fn get_children(&self, session_id: &str) -> std::io::Result<Vec<SessionMetadata>> {
        let all = self.list_sessions()?;
        let mut children = Vec::new();
        for meta in all {
            if let Ok(entries) = self.load_entries(&meta.id) {
                for entry in &entries {
                    if let SessionEntry::Branch(be) = entry {
                        if be.parent_session_id == session_id {
                            children.push(meta);
                            break;
                        }
                    }
                }
            }
        }
        Ok(children)
    }

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

    pub fn fork_session(&self, parent_id: &str, reason: &str) -> SessionResult<String> {
        let header = self.read_header(parent_id)?;
        let new_id = uuid::Uuid::new_v4().to_string();

        self.init_session(&new_id, &header.model, &header.working_dir)?;

        let branch_entry = SessionEntry::Branch(uncode_core::session::BranchEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            parent_session_id: parent_id.to_string(),
            reason: reason.to_string(),
        });
        self.append_entry(&new_id, &branch_entry)?;

        Ok(new_id)
    }

    /// 使内存缓存失效，下次访问时重新从文件加载
    pub fn invalidate(&self, session_id: &str) {
        self.sessions.write().remove(session_id);
    }
}
