//! 会话存储层 — SurrealDB v3 异步后端
//!
//! 所有方法为 async，调用方需在 tokio runtime 内使用。
//!
//! **Pi:** 逻辑 API 对齐 Pi Session 的 append / load / `getBranch`；**物理**为 SurrealDB 而非 JSONL 主存。
//! JSONL 导入/导出见 `import` 模块。

use std::path::PathBuf;

use uncode_core::session::{SessionEntry, SessionHeader, SessionMetadata, SessionTree};

use super::surreal_store::SurrealSessionStore;

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

/// 会话存储 — SurrealDB 后端异步封装。
///
/// **Pi:** 对应 Session 持久化门面（Pi 默认 JSONL 文件；uncode 为嵌入式 DB + 可选 JSONL 互操作）。
pub struct SessionStore {
    inner: SurrealSessionStore,
}

impl SessionStore {
    /// 创建持久化 SessionStore（RocksDB 后端）
    pub async fn new(base_dir: PathBuf) -> SessionResult<Self> {
        let inner = SurrealSessionStore::new(&base_dir).await?;
        Ok(Self { inner })
    }

    /// 创建内存 SessionStore（用于测试）
    pub async fn new_memory() -> SessionResult<Self> {
        let inner = SurrealSessionStore::new_memory().await?;
        Ok(Self { inner })
    }

    pub fn default_dir() -> std::io::Result<PathBuf> {
        SurrealSessionStore::default_dir()
    }

    pub async fn init_session(
        &self,
        session_id: &str,
        model: &str,
        working_dir: &str,
    ) -> SessionResult<()> {
        self.inner
            .init_session(session_id, model, working_dir)
            .await
    }

    pub async fn init_session_with_title(
        &self,
        session_id: &str,
        model: &str,
        working_dir: &str,
        title: Option<String>,
    ) -> SessionResult<()> {
        self.inner
            .init_session_with_title(session_id, model, working_dir, title)
            .await
    }

    pub async fn append_entry(&self, session_id: &str, entry: &SessionEntry) -> SessionResult<()> {
        self.inner.append_entry(session_id, entry).await
    }

    pub async fn load_entries(&self, session_id: &str) -> SessionResult<Vec<SessionEntry>> {
        self.inner.load_entries(session_id).await
    }

    pub async fn get_entry(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> SessionResult<Option<SessionEntry>> {
        self.inner.get_entry(session_id, entry_id).await
    }

    pub async fn get_leaf_id(&self, session_id: &str) -> SessionResult<Option<String>> {
        self.inner.get_leaf_id(session_id).await
    }

    pub async fn set_leaf(&self, session_id: &str, target_id: &str) -> SessionResult<()> {
        self.inner.set_leaf(session_id, target_id).await
    }

    pub async fn get_path_to_root(
        &self,
        session_id: &str,
        from_id: &str,
    ) -> SessionResult<Vec<SessionEntry>> {
        self.inner.get_path_to_root(session_id, from_id).await
    }

    pub async fn list_sessions(&self) -> SessionResult<Vec<SessionMetadata>> {
        self.inner.list_sessions().await
    }

    pub async fn find_most_recent(&self) -> SessionResult<Option<SessionMetadata>> {
        self.inner.find_most_recent().await
    }

    pub async fn read_header(&self, session_id: &str) -> SessionResult<SessionHeader> {
        self.inner.read_header(session_id).await
    }

    pub async fn get_children(&self, session_id: &str) -> SessionResult<Vec<SessionMetadata>> {
        self.inner.get_children(session_id).await
    }

    pub async fn fork_session(&self, parent_id: &str, reason: &str) -> SessionResult<String> {
        self.inner.fork_session(parent_id, reason).await
    }

    pub async fn message_count(&self, session_id: &str) -> usize {
        self.inner.message_count(session_id).await
    }

    pub async fn build_tree(&self, session_id: &str) -> SessionResult<SessionTree> {
        self.inner.build_tree(session_id).await
    }

    pub async fn invalidate(&self, session_id: &str) {
        self.inner.invalidate(session_id).await
    }
}
