//! SurrealDB v3 会话存储后端
//!
//! 使用嵌入式 SurrealDB (kv-rocksdb) 替代 JSONL 文件。
//! 所有方法为 async，调用方需在 tokio runtime 内使用。

use std::path::PathBuf;

use surrealdb::Surreal;
use surrealdb::engine::any::{Any, connect};
use uncode_core::session::{
    BranchEntry, LeafEntry, SessionEntry, SessionHeader, SessionMetadata, SessionNode, SessionTree,
    generate_entry_id,
};

use super::store::{SessionError, SessionResult};

// ── DDL ──

const SCHEMA: &str = r#"
DEFINE TABLE session SCHEMAFULL;
DEFINE FIELD id ON session TYPE string;
DEFINE FIELD version ON session TYPE number DEFAULT 2;
DEFINE FIELD parent_session ON session TYPE option<string>;
DEFINE FIELD created_at ON session TYPE string;
DEFINE FIELD updated_at ON session TYPE string;
DEFINE FIELD model ON session TYPE string;
DEFINE FIELD title ON session TYPE option<string>;
DEFINE FIELD working_dir ON session TYPE string;
DEFINE INDEX idx_session_updated ON session COLUMNS updated_at;
DEFINE INDEX idx_session_model ON session COLUMNS model;

DEFINE TABLE entry SCHEMAFULL;
DEFINE FIELD session_id ON entry TYPE string;
DEFINE FIELD entry_id ON entry TYPE string;
DEFINE FIELD entry_type ON entry TYPE string;
DEFINE FIELD parent_entry_id ON entry TYPE option<string>;
DEFINE FIELD timestamp ON entry TYPE string;
DEFINE FIELD data ON entry TYPE any;
DEFINE INDEX idx_entry_session ON entry COLUMNS session_id;
DEFINE INDEX idx_entry_id ON entry COLUMNS session_id, entry_id;
DEFINE INDEX idx_entry_parent ON entry COLUMNS session_id, parent_entry_id;

DEFINE TABLE leaf SCHEMAFULL;
DEFINE FIELD session_id ON leaf TYPE string;
DEFINE FIELD target_id ON leaf TYPE string;
DEFINE INDEX idx_leaf_session ON leaf COLUMNS session_id UNIQUE;
"#;

// ── Helpers ──

fn header_to_value(h: &SessionHeader) -> serde_json::Value {
    let mut v = serde_json::json!({
        "id": h.id,
        "version": h.version,
        "created_at": h.created_at.to_rfc3339(),
        "updated_at": h.updated_at.to_rfc3339(),
        "model": h.model,
        "working_dir": h.working_dir,
    });
    if let Some(ref ps) = h.parent_session {
        v["parent_session"] = serde_json::Value::String(ps.clone());
    }
    if let Some(ref t) = h.title {
        v["title"] = serde_json::Value::String(t.clone());
    }
    v
}

fn extract_id(v: &serde_json::Value) -> String {
    let raw = v["id"].as_str().unwrap_or_default();
    // SurrealDB returns "table:`key`" or "table:key" format — extract just the key
    let key = if let Some(colon_pos) = raw.rfind(':') {
        &raw[colon_pos + 1..]
    } else {
        raw
    };
    // Strip surrounding backticks if present
    key.trim_start_matches('`')
        .trim_end_matches('`')
        .to_string()
}

fn value_to_header(v: &serde_json::Value) -> SessionResult<SessionHeader> {
    let id = extract_id(v);
    let version = v["version"].as_u64().unwrap_or(2) as u32;
    let parent_session = v["parent_session"].as_str().map(|s| s.to_string());
    let created_at = v["created_at"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(chrono::Utc::now);
    let updated_at = v["updated_at"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(chrono::Utc::now);
    let model = v["model"].as_str().unwrap_or_default().to_string();
    let title = v["title"].as_str().map(|s| s.to_string());
    let working_dir = v["working_dir"].as_str().unwrap_or_default().to_string();

    Ok(SessionHeader {
        entry_type: "session".into(),
        id,
        version,
        parent_session,
        created_at,
        updated_at,
        model,
        title,
        working_dir,
    })
}

fn value_to_metadata(v: &serde_json::Value, message_count: u64) -> SessionResult<SessionMetadata> {
    let header = value_to_header(v)?;
    let mut meta = SessionMetadata::from(header);
    meta.message_count = message_count;
    Ok(meta)
}

// ── SurrealSessionStore ──

pub struct SurrealSessionStore {
    db: Surreal<Any>,
}

impl SurrealSessionStore {
    /// 创建持久化 RocksDB 后端
    pub async fn new(path: &std::path::Path) -> SessionResult<Self> {
        let db = connect(format!("rocksdb://{}", path.display()))
            .await
            .map_err(|e| {
                SessionError::Io(std::io::Error::other(format!(
                    "surrealdb rocksdb init: {e}"
                )))
            })?;

        db.use_ns("uncode").use_db("sessions").await.map_err(|e| {
            SessionError::Io(std::io::Error::other(format!("surrealdb ns/db: {e}")))
        })?;

        let store = Self { db };
        store.init_schema().await?;
        Ok(store)
    }

    /// 创建内存后端（用于测试）
    pub async fn new_memory() -> SessionResult<Self> {
        let db = connect("mem://").await.map_err(|e| {
            SessionError::Io(std::io::Error::other(format!("surrealdb mem init: {e}")))
        })?;

        db.use_ns("uncode").use_db("sessions").await.map_err(|e| {
            SessionError::Io(std::io::Error::other(format!("surrealdb ns/db: {e}")))
        })?;

        let store = Self { db };
        store.init_schema().await?;
        Ok(store)
    }

    async fn init_schema(&self) -> SessionResult<()> {
        self.db
            .query(SCHEMA)
            .await
            .map_err(|e| SessionError::InvalidData(format!("schema init: {e}")))?;
        Ok(())
    }

    // ── 公开 API ──

    pub async fn init_session(
        &self,
        session_id: &str,
        model: &str,
        working_dir: &str,
    ) -> SessionResult<()> {
        self.init_session_with_title(session_id, model, working_dir, None)
            .await
    }

    pub async fn init_session_with_title(
        &self,
        session_id: &str,
        model: &str,
        working_dir: &str,
        title: Option<String>,
    ) -> SessionResult<()> {
        let existing: Option<serde_json::Value> = self
            .db
            .select(("session", session_id))
            .await
            .map_err(db_err("select session"))?;
        if existing.is_some() {
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

        let _: Option<serde_json::Value> = self
            .db
            .create(("session", session_id))
            .content(header_to_value(&header))
            .await
            .map_err(db_err("create session"))?;

        Ok(())
    }

    pub async fn append_entry(&self, session_id: &str, entry: &SessionEntry) -> SessionResult<()> {
        // 确认 session 存在
        let _: Option<serde_json::Value> = self
            .db
            .select(("session", session_id))
            .await
            .map_err(db_err("select session"))?
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        // Auto-set parent_id to current leaf
        let mut entry = entry.clone();
        let current_leaf = self.get_leaf_id(session_id).await?;
        if entry.parent_id().is_none()
            && let Some(ref leaf) = current_leaf
        {
            entry.set_parent_id(leaf.clone());
        }

        let entry_type = match &entry {
            SessionEntry::Message(_) => "message",
            SessionEntry::System(_) => "system",
            SessionEntry::Branch(_) => "branch",
            SessionEntry::Leaf(_) => "leaf",
            SessionEntry::Compaction(_) => "compaction",
            SessionEntry::ModelChange(_) => "model_change",
            SessionEntry::ThinkingLevelChange(_) => "thinking_level_change",
            SessionEntry::BranchSummary(_) => "branch_summary",
            SessionEntry::Custom(_) => "custom",
            SessionEntry::CustomMessage(_) => "custom_message",
            SessionEntry::Label(_) => "label",
            SessionEntry::SessionInfo(_) => "session_info",
            SessionEntry::DecisionAudit(_) => "decision_audit",
            _ => "unknown",
        };

        let entry_id = entry.entry_id().to_string();
        let parent_entry_id = entry.parent_id().map(|s| s.to_string());
        let timestamp = match &entry {
            SessionEntry::Message(e) => e.timestamp.to_rfc3339(),
            SessionEntry::System(e) => e.timestamp.to_rfc3339(),
            SessionEntry::Branch(e) => e.timestamp.to_rfc3339(),
            SessionEntry::Leaf(e) => e.timestamp.to_rfc3339(),
            SessionEntry::Compaction(e) => e.timestamp.to_rfc3339(),
            SessionEntry::ModelChange(e) => e.timestamp.to_rfc3339(),
            SessionEntry::ThinkingLevelChange(e) => e.timestamp.to_rfc3339(),
            SessionEntry::BranchSummary(e) => e.timestamp.to_rfc3339(),
            SessionEntry::Custom(e) => e.timestamp.to_rfc3339(),
            SessionEntry::CustomMessage(e) => e.timestamp.to_rfc3339(),
            SessionEntry::Label(e) => e.timestamp.to_rfc3339(),
            SessionEntry::SessionInfo(e) => e.timestamp.to_rfc3339(),
            _ => chrono::Utc::now().to_rfc3339(),
        };

        let data = serde_json::to_value(&entry).map_err(SessionError::Serialization)?;

        let mut record = serde_json::json!({
            "session_id": session_id,
            "entry_id": entry_id,
            "entry_type": entry_type,
            "timestamp": timestamp,
            "data": data,
        });
        if let Some(ref pid) = parent_entry_id {
            record["parent_entry_id"] = serde_json::Value::String(pid.clone());
        }

        let surreal_id = format!("{session_id}{entry_id}");
        let _: Option<serde_json::Value> = self
            .db
            .create(("entry", surreal_id.as_str()))
            .content(record)
            .await
            .map_err(db_err("create entry"))?;

        // 更新 leaf 指针
        let new_leaf = match &entry {
            SessionEntry::Leaf(leaf) => leaf.target_id.clone(),
            _ => entry_id.clone(),
        };
        self.set_leaf_internal(session_id, &new_leaf).await?;

        // 更新 session 的 updated_at
        let now = chrono::Utc::now().to_rfc3339();
        self.db
            .query("UPDATE session SET updated_at = $now WHERE id = $sid")
            .bind(("now", now))
            .bind(("sid", session_id.to_string()))
            .await
            .map_err(db_err("update session timestamp"))?;

        Ok(())
    }

    pub async fn load_entries(&self, session_id: &str) -> SessionResult<Vec<SessionEntry>> {
        // Verify session exists
        let _: Option<serde_json::Value> = self
            .db
            .select(("session", session_id))
            .await
            .map_err(db_err("select session for entries"))?
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        let mut resp = self
            .db
            .query("SELECT * FROM entry WHERE session_id = $sid ORDER BY timestamp")
            .bind(("sid", session_id.to_string()))
            .await
            .map_err(db_err("select entries"))?;

        let results: Vec<serde_json::Value> = resp.take(0).map_err(db_err("take entries"))?;

        let mut entries = Vec::with_capacity(results.len());
        for v in results {
            let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
            let entry: SessionEntry =
                serde_json::from_value(data).map_err(SessionError::Serialization)?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub async fn get_entry(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> SessionResult<Option<SessionEntry>> {
        let surreal_id = format!("{session_id}{entry_id}");
        let record: Option<serde_json::Value> = self
            .db
            .select(("entry", surreal_id.as_str()))
            .await
            .map_err(db_err("select entry"))?;

        match record {
            Some(v) => {
                let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
                let entry: SessionEntry =
                    serde_json::from_value(data).map_err(SessionError::Serialization)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    pub async fn get_leaf_id(&self, session_id: &str) -> SessionResult<Option<String>> {
        let mut resp = self
            .db
            .query("SELECT target_id FROM leaf WHERE session_id = $sid")
            .bind(("sid", session_id.to_string()))
            .await
            .map_err(db_err("select leaf"))?;

        let results: Vec<serde_json::Value> = resp.take(0).map_err(db_err("take leaf"))?;

        Ok(results
            .into_iter()
            .next()
            .and_then(|v| v.get("target_id")?.as_str().map(|s| s.to_string())))
    }

    pub async fn set_leaf(&self, session_id: &str, target_id: &str) -> SessionResult<()> {
        let entry = self.get_entry(session_id, target_id).await?;
        if entry.is_none() {
            return Err(SessionError::InvalidData(format!(
                "target entry not found: {target_id}"
            )));
        }

        let leaf = LeafEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            target_id: target_id.to_string(),
        };

        self.append_entry(session_id, &SessionEntry::Leaf(Box::new(leaf)))
            .await?;
        Ok(())
    }

    async fn set_leaf_internal(&self, session_id: &str, target_id: &str) -> SessionResult<()> {
        let _: Option<serde_json::Value> = self
            .db
            .query(
                "BEGIN TRANSACTION;
                 DELETE leaf WHERE session_id = $sid;
                 CREATE leaf CONTENT { session_id: $sid, target_id: $tid };
                 COMMIT TRANSACTION;",
            )
            .bind(("sid", session_id.to_string()))
            .bind(("tid", target_id.to_string()))
            .await
            .map_err(db_err("set_leaf transaction"))?
            .take(0)
            .ok()
            .flatten();

        Ok(())
    }

    pub async fn get_path_to_root(
        &self,
        session_id: &str,
        from_id: &str,
    ) -> SessionResult<Vec<SessionEntry>> {
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

            match self.get_entry(session_id, &id).await? {
                Some(entry) => {
                    current = entry.parent_id().map(|s| s.to_string());
                    path.push(entry);
                }
                None => break,
            }
        }

        Ok(path)
    }

    pub async fn list_sessions(&self) -> SessionResult<Vec<SessionMetadata>> {
        let mut resp = self
            .db
            .query("SELECT * FROM session ORDER BY updated_at DESC")
            .await
            .map_err(db_err("list sessions"))?;

        let results: Vec<serde_json::Value> = resp.take(0).map_err(db_err("take sessions"))?;

        let mut metas = Vec::with_capacity(results.len());
        for v in results {
            let sid = v["id"].as_str().unwrap_or_default().to_string();
            let mc = self.message_count(&sid).await as u64;
            metas.push(value_to_metadata(&v, mc)?);
        }
        Ok(metas)
    }

    pub async fn find_most_recent(&self) -> SessionResult<Option<SessionMetadata>> {
        let mut resp = self
            .db
            .query("SELECT * FROM session ORDER BY updated_at DESC LIMIT 1")
            .await
            .map_err(db_err("find most recent"))?;

        let results: Vec<serde_json::Value> = resp.take(0).map_err(db_err("take recent"))?;

        match results.into_iter().next() {
            Some(v) => {
                let sid = v["id"].as_str().unwrap_or_default().to_string();
                let mc = self.message_count(&sid).await as u64;
                Ok(Some(value_to_metadata(&v, mc)?))
            }
            None => Ok(None),
        }
    }

    pub async fn read_header(&self, session_id: &str) -> SessionResult<SessionHeader> {
        let record: Option<serde_json::Value> = self
            .db
            .select(("session", session_id))
            .await
            .map_err(db_err("read header"))?;

        match record {
            Some(v) => {
                let header = value_to_header(&v)?;
                if !std::path::Path::new(&header.working_dir).exists() {
                    tracing::warn!(
                        "session {} working_dir '{}' no longer exists",
                        session_id,
                        header.working_dir
                    );
                }
                Ok(header)
            }
            None => Err(SessionError::NotFound(session_id.to_string())),
        }
    }

    pub async fn get_children(&self, session_id: &str) -> SessionResult<Vec<SessionMetadata>> {
        let mut resp = self
            .db
            .query("SELECT * FROM session WHERE parent_session = $sid")
            .bind(("sid", session_id.to_string()))
            .await
            .map_err(db_err("get children"))?;

        let children: Vec<serde_json::Value> = resp.take(0).map_err(db_err("take children"))?;

        let mut metas = Vec::with_capacity(children.len());
        for v in children {
            let sid = v["id"].as_str().unwrap_or_default().to_string();
            let mc = self.message_count(&sid).await as u64;
            metas.push(value_to_metadata(&v, mc)?);
        }
        Ok(metas)
    }

    pub async fn fork_session(&self, parent_id: &str, reason: &str) -> SessionResult<String> {
        let header = self.read_header(parent_id).await?;
        let new_id = uuid::Uuid::new_v4().to_string();

        self.init_session(&new_id, &header.model, &header.working_dir)
            .await?;

        self.db
            .query("UPDATE session SET parent_session = $pid WHERE id = $sid")
            .bind(("pid", parent_id.to_string()))
            .bind(("sid", new_id.clone()))
            .await
            .map_err(db_err("set parent_session"))?;

        let branch_entry = SessionEntry::Branch(Box::new(BranchEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            parent_session_id: parent_id.to_string(),
            reason: reason.to_string(),
        }));
        self.append_entry(&new_id, &branch_entry).await?;

        Ok(new_id)
    }

    pub async fn message_count(&self, session_id: &str) -> usize {
        let result = self
            .db
            .query("SELECT count() AS count FROM entry WHERE session_id = $sid AND entry_type = 'message' GROUP ALL")
            .bind(("sid", session_id.to_string()))
            .await;

        match result {
            Ok(mut resp) => {
                let values: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
                values
                    .first()
                    .and_then(|v| v.get("count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize
            }
            Err(_) => 0,
        }
    }

    pub async fn build_tree(&self, session_id: &str) -> SessionResult<SessionTree> {
        let root = self.build_node(session_id).await?;
        Ok(SessionTree { root })
    }

    fn build_node<'a>(
        &'a self,
        session_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = SessionResult<SessionNode>> + 'a>> {
        Box::pin(async move {
            let header = self.read_header(session_id).await?;
            let msg_count = self.message_count(session_id).await;

            let children_meta = self.get_children(session_id).await?;
            let mut children = Vec::with_capacity(children_meta.len());
            for child in children_meta {
                children.push(self.build_node(&child.id).await?);
            }

            Ok(SessionNode {
                id: session_id.to_string(),
                title: header.title,
                model: header.model,
                message_count: msg_count,
                children,
            })
        })
    }

    /// 将 leaf 指针回退 n 个 User Message entry（跳过中间的 Assistant/Tool 等非 User 消息）。
    ///
    /// **Pi:** 对照 `navigateTree(targetId)` — 回退到历史 entry 并重建上下文。
    pub async fn undo_turn(&self, session_id: &str, n: u64) -> SessionResult<String> {
        let entries = self.load_entries(session_id).await?;
        if entries.is_empty() {
            return Err(SessionError::InvalidData("nothing to undo".into()));
        }

        let current_leaf = self.get_leaf_id(session_id).await?;

        // 确定 leaf 在 entries 中的位置
        let leaf_pos = match &current_leaf {
            Some(lid) => entries
                .iter()
                .position(|e| e.entry_id() == lid.as_str())
                .unwrap_or(entries.len().saturating_sub(1)),
            None => entries.len().saturating_sub(1),
        };

        // 从 leaf 位置倒序扫描，数 n 个 User Message
        let mut user_count = 0u64;
        let mut target_idx: Option<usize> = None;

        for i in (0..=leaf_pos).rev() {
            if let SessionEntry::Message(me) = &entries[i] {
                if me.role == uncode_core::message::Role::User {
                    user_count += 1;
                    if user_count == n {
                        // 指向这条 User 消息之前的 entry
                        target_idx = if i > 0 { Some(i - 1) } else { None };
                        break;
                    }
                }
            }
        }

        let target_id = match target_idx {
            Some(idx) => entries[idx].entry_id().to_string(),
            None => return Err(SessionError::InvalidData("nothing to undo".into())),
        };

        self.set_leaf(session_id, &target_id).await?;
        Ok(target_id)
    }

    /// 按标题模糊搜索（大小写不敏感）。
    pub async fn search_sessions(&self, query: &str) -> SessionResult<Vec<SessionMetadata>> {
        let lower_query = query.to_lowercase();
        let mut resp = self
            .db
            .query("SELECT * FROM session WHERE string::contains(string::lowercase(title), $q) ORDER BY updated_at DESC")
            .bind(("q", lower_query))
            .await
            .map_err(db_err("search sessions"))?;

        let results: Vec<serde_json::Value> =
            resp.take(0).map_err(db_err("take search results"))?;

        let mut metas = Vec::with_capacity(results.len());
        for v in results {
            let sid = v["id"].as_str().unwrap_or_default().to_string();
            let mc = self.message_count(&sid).await as u64;
            metas.push(value_to_metadata(&v, mc)?);
        }
        Ok(metas)
    }

    /// 按模型过滤 session 列表。
    pub async fn list_sessions_by_model(&self, model: &str) -> SessionResult<Vec<SessionMetadata>> {
        let mut resp = self
            .db
            .query("SELECT * FROM session WHERE model = $model ORDER BY updated_at DESC")
            .bind(("model", model.to_string()))
            .await
            .map_err(db_err("list by model"))?;

        let results: Vec<serde_json::Value> = resp.take(0).map_err(db_err("take by model"))?;

        let mut metas = Vec::with_capacity(results.len());
        for v in results {
            let sid = v["id"].as_str().unwrap_or_default().to_string();
            let mc = self.message_count(&sid).await as u64;
            metas.push(value_to_metadata(&v, mc)?);
        }
        Ok(metas)
    }

    /// 按更新时间日期范围过滤。
    pub async fn list_sessions_by_date(
        &self,
        from: &chrono::DateTime<chrono::Utc>,
        to: &chrono::DateTime<chrono::Utc>,
    ) -> SessionResult<Vec<SessionMetadata>> {
        let mut resp = self
            .db
            .query("SELECT * FROM session WHERE updated_at >= $from AND updated_at <= $to ORDER BY updated_at DESC")
            .bind(("from", from.to_rfc3339()))
            .bind(("to", to.to_rfc3339()))
            .await
            .map_err(db_err("list by date"))?;

        let results: Vec<serde_json::Value> = resp.take(0).map_err(db_err("take by date"))?;

        let mut metas = Vec::with_capacity(results.len());
        for v in results {
            let sid = v["id"].as_str().unwrap_or_default().to_string();
            let mc = self.message_count(&sid).await as u64;
            metas.push(value_to_metadata(&v, mc)?);
        }
        Ok(metas)
    }

    /// 更新 session 标题。
    pub async fn update_title(&self, session_id: &str, title: &str) -> SessionResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let _: Option<serde_json::Value> = self
            .db
            .update(("session", session_id))
            .merge(serde_json::json!({
                "title": title,
                "updated_at": now,
            }))
            .await
            .map_err(db_err("update title"))?;
        Ok(())
    }

    pub async fn invalidate(&self, _session_id: &str) {
        // no-op
    }

    pub fn default_dir() -> std::io::Result<PathBuf> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("uncode")
            .join("data")
            .join("sessions.db");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

fn db_err(context: &'static str) -> impl Fn(surrealdb::Error) -> SessionError {
    move |e: surrealdb::Error| {
        SessionError::Io(std::io::Error::other(format!("surrealdb {context}: {e}")))
    }
}
