//! 会话树逻辑模型：`SessionEntry` 树、压缩与分支摘要条目。
//!
//! **Pi:** 与 `SessionTreeEntry` / `buildContext()` 消费的条目类型同构（见 Pi `PI_SESSION_MODEL`）。
//! **物理存储**由 `uncode-agent::SessionStore`（SurrealDB）实现，非本模块职责。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api_types::ThinkingLevel;
use crate::message::{ContentBlock, Message, Role, UsageInfo};

/// Generate a time-sortable entry ID from UUIDv7 (full simple format for uniqueness).
///
/// **Pi:** 对应条目 `id`（UUIDv7，时间可排序）。
pub fn generate_entry_id() -> String {
    uuid::Uuid::now_v7().simple().to_string()
}

fn default_session_version() -> u32 {
    1
}

// ── SessionHeader ──

/// 会话元数据头（首条或导出 JSONL 时的 session 记录）。
///
/// **Pi:** 对应 session 级元数据；`working_dir` 与 Pi 工作目录语义一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub id: String,
    #[serde(default = "default_session_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub model: String,
    pub title: Option<String>,
    pub working_dir: String,
}

impl SessionHeader {
    pub fn new(id: String, model: String, working_dir: String) -> Self {
        let now = Utc::now();
        Self {
            entry_type: "session".into(),
            id,
            version: 2,
            parent_session: None,
            created_at: now,
            updated_at: now,
            model,
            title: None,
            working_dir,
        }
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }
}

// ── SessionEntry ──

/// 树状会话条目（serde 外部 tag），逻辑上与 Pi 会话 JSONL 行同构。
///
/// **Pi:** 对应 `SessionTreeEntry` 各 `type`；完整映射见 `UNCODE_PI_MECHANISM_MAP` §4。
/// **OpenCode:** 对照 `MessageV2` / Part 持久化（结构不同）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionEntry {
    #[serde(rename = "message")]
    Message(Box<MessageEntry>),
    #[serde(rename = "system")]
    System(Box<SystemEntry>),
    #[serde(rename = "branch")]
    Branch(Box<BranchEntry>),
    #[serde(rename = "leaf")]
    Leaf(Box<LeafEntry>),
    #[serde(rename = "compaction")]
    Compaction(Box<CompactionEntry>),
    #[serde(rename = "model_change")]
    ModelChange(Box<ModelChangeEntry>),
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange(Box<ThinkingLevelChangeEntry>),
    #[serde(rename = "branch_summary")]
    BranchSummary(Box<BranchSummaryEntry>),
    #[serde(rename = "custom")]
    Custom(Box<CustomEntry>),
    #[serde(rename = "custom_message")]
    CustomMessage(Box<CustomMessageEntry>),
    #[serde(rename = "label")]
    Label(Box<LabelEntry>),
    #[serde(rename = "session_info")]
    SessionInfo(Box<SessionInfoEntry>),
    #[serde(rename = "decision_audit")]
    DecisionAudit(Box<DecisionAuditEntry>),
}

impl SessionEntry {
    pub fn entry_id(&self) -> &str {
        match self {
            SessionEntry::Message(e) => &e.id,
            SessionEntry::System(e) => &e.id,
            SessionEntry::Branch(e) => &e.id,
            SessionEntry::Leaf(e) => &e.id,
            SessionEntry::Compaction(e) => &e.id,
            SessionEntry::ModelChange(e) => &e.id,
            SessionEntry::ThinkingLevelChange(e) => &e.id,
            SessionEntry::BranchSummary(e) => &e.id,
            SessionEntry::Custom(e) => &e.id,
            SessionEntry::CustomMessage(e) => &e.id,
            SessionEntry::Label(e) => &e.id,
            SessionEntry::SessionInfo(e) => &e.id,
            SessionEntry::DecisionAudit(e) => &e.id,
        }
    }

    pub fn parent_id(&self) -> Option<&str> {
        match self {
            SessionEntry::Message(e) => e.parent_id.as_deref(),
            SessionEntry::System(e) => e.parent_id.as_deref(),
            SessionEntry::Branch(e) => e.parent_id.as_deref(),
            SessionEntry::Leaf(e) => e.parent_id.as_deref(),
            SessionEntry::Compaction(e) => e.parent_id.as_deref(),
            SessionEntry::ModelChange(e) => e.parent_id.as_deref(),
            SessionEntry::ThinkingLevelChange(e) => e.parent_id.as_deref(),
            SessionEntry::BranchSummary(e) => e.parent_id.as_deref(),
            SessionEntry::Custom(e) => e.parent_id.as_deref(),
            SessionEntry::CustomMessage(e) => e.parent_id.as_deref(),
            SessionEntry::Label(e) => e.parent_id.as_deref(),
            SessionEntry::SessionInfo(e) => e.parent_id.as_deref(),
            SessionEntry::DecisionAudit(e) => e.parent_id.as_deref(),
        }
    }

    pub fn set_parent_id(&mut self, new_parent: String) {
        match self {
            SessionEntry::Message(e) => e.parent_id = Some(new_parent),
            SessionEntry::System(e) => e.parent_id = Some(new_parent),
            SessionEntry::Branch(e) => e.parent_id = Some(new_parent),
            SessionEntry::Leaf(e) => e.parent_id = Some(new_parent),
            SessionEntry::Compaction(e) => e.parent_id = Some(new_parent),
            SessionEntry::ModelChange(e) => e.parent_id = Some(new_parent),
            SessionEntry::ThinkingLevelChange(e) => e.parent_id = Some(new_parent),
            SessionEntry::BranchSummary(e) => e.parent_id = Some(new_parent),
            SessionEntry::Custom(e) => e.parent_id = Some(new_parent),
            SessionEntry::CustomMessage(e) => e.parent_id = Some(new_parent),
            SessionEntry::Label(e) => e.parent_id = Some(new_parent),
            SessionEntry::SessionInfo(e) => e.parent_id = Some(new_parent),
            SessionEntry::DecisionAudit(e) => e.parent_id = Some(new_parent),
        }
    }
}

// ── Entry types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
}

impl MessageEntry {
    pub fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            role,
            content,
            usage: None,
        }
    }
}

impl From<Message> for MessageEntry {
    fn from(msg: Message) -> Self {
        Self {
            id: if msg.id.is_empty() {
                generate_entry_id()
            } else {
                msg.id
            },
            parent_id: None,
            timestamp: Utc::now(),
            role: msg.role,
            content: msg.content,
            usage: msg.usage,
        }
    }
}

impl From<Message> for Box<MessageEntry> {
    fn from(msg: Message) -> Self {
        Box::new(MessageEntry::from(msg))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub event: SystemEventType,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemEventType {
    SessionStart,
    SessionEnd,
    PhaseSummary,
    Error,
    Compaction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub parent_session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeafEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub target_id: String,
}

/// 压缩摘要边界条目；`build_context` 仅保留 `first_kept_entry_id` 之后的消息。
///
/// **Pi:** 对应 `compaction` 类型条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub first_kept_entry_id: String,
    pub tokens_before: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_read: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_modified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChangeEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingLevelChangeEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub thinking_level: ThinkingLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSummaryEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub from_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub custom_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMessageEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub custom_type: String,
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub display: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub target_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub name: Option<String>,
}

/// 决策审计条目 — 记录每次裁决/拒绝的决策轨迹 (#387)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionAuditEntry {
    #[serde(default = "generate_entry_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub turn_id: String,
    pub tool_name: String,
    pub allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub adjudication_duration_ms: u64,
}

// ── SessionMetadata ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    #[serde(default = "default_session_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: u64,
    pub title: Option<String>,
    pub working_dir: String,
    pub model: String,
}

impl From<SessionHeader> for SessionMetadata {
    fn from(h: SessionHeader) -> Self {
        Self {
            id: h.id,
            version: h.version,
            parent_session: h.parent_session,
            created_at: h.created_at,
            updated_at: h.updated_at,
            message_count: 0,
            title: h.title,
            working_dir: h.working_dir,
            model: h.model,
        }
    }
}

// ── SessionNode / SessionTree ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionNode {
    pub id: String,
    pub title: Option<String>,
    pub model: String,
    pub message_count: usize,
    pub children: Vec<SessionNode>,
}

/// 会话树 UI/列举用结构（子会话嵌套）。
///
/// **Pi:** 分支为隐含树（`leafId` 路径）；uncode 可显式 `Branch` 条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTree {
    pub root: SessionNode,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::ThinkingLevel;
    use crate::message::{ContentBlock, Role};
    use chrono::Utc;

    // ── generate_entry_id ──

    #[test]
    fn generate_entry_id_non_empty() {
        let id = generate_entry_id();
        assert!(!id.is_empty());
    }

    #[test]
    fn generate_entry_id_unique() {
        let id1 = generate_entry_id();
        let id2 = generate_entry_id();
        assert_ne!(id1, id2);
    }

    // ── SessionEntry::entry_id ──

    fn make_message_entry() -> SessionEntry {
        SessionEntry::Message(Box::new(MessageEntry::new(
            Role::User,
            vec![ContentBlock::Text { text: "hi".into() }],
        )))
    }

    fn make_branch_entry() -> SessionEntry {
        SessionEntry::Branch(Box::new(BranchEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            parent_session_id: "p1".into(),
            reason: "fork".into(),
        }))
    }

    fn make_leaf_entry() -> SessionEntry {
        SessionEntry::Leaf(Box::new(LeafEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            target_id: "t1".into(),
        }))
    }

    fn make_compaction_entry() -> SessionEntry {
        SessionEntry::Compaction(Box::new(CompactionEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            summary: "summary".into(),
            first_kept_entry_id: "e1".into(),
            tokens_before: 1000,
            files_read: vec![],
            files_modified: vec![],
        }))
    }

    fn make_model_change_entry() -> SessionEntry {
        SessionEntry::ModelChange(Box::new(ModelChangeEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            provider: "openai".into(),
            model_id: "gpt-4".into(),
        }))
    }

    fn make_thinking_level_change_entry() -> SessionEntry {
        SessionEntry::ThinkingLevelChange(Box::new(ThinkingLevelChangeEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            thinking_level: ThinkingLevel::High,
        }))
    }

    fn make_system_entry() -> SessionEntry {
        SessionEntry::System(Box::new(SystemEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            event: SystemEventType::SessionStart,
            data: serde_json::json!({}),
        }))
    }

    fn make_branch_summary_entry() -> SessionEntry {
        SessionEntry::BranchSummary(Box::new(BranchSummaryEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            from_id: "b1".into(),
            summary: "branch summary".into(),
        }))
    }

    fn make_custom_entry() -> SessionEntry {
        SessionEntry::Custom(Box::new(CustomEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            custom_type: "my_type".into(),
            data: Some(serde_json::json!({"k": "v"})),
        }))
    }

    fn make_custom_message_entry() -> SessionEntry {
        SessionEntry::CustomMessage(Box::new(CustomMessageEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            custom_type: "my_type".into(),
            content: vec![ContentBlock::Text { text: "msg".into() }],
            display: true,
        }))
    }

    fn make_label_entry() -> SessionEntry {
        SessionEntry::Label(Box::new(LabelEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            target_id: "t1".into(),
            label: Some("important".into()),
        }))
    }

    fn make_session_info_entry() -> SessionEntry {
        SessionEntry::SessionInfo(Box::new(SessionInfoEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            name: Some("sess-name".into()),
        }))
    }

    fn make_decision_audit_entry() -> SessionEntry {
        SessionEntry::DecisionAudit(Box::new(DecisionAuditEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: Utc::now(),
            turn_id: "turn-1".into(),
            tool_name: "bash".into(),
            allowed: true,
            reason: Some("approved".into()),
            adjudication_duration_ms: 50,
        }))
    }

    fn all_variants() -> Vec<SessionEntry> {
        vec![
            make_message_entry(),
            make_branch_entry(),
            make_leaf_entry(),
            make_compaction_entry(),
            make_model_change_entry(),
            make_thinking_level_change_entry(),
            make_system_entry(),
            make_branch_summary_entry(),
            make_custom_entry(),
            make_custom_message_entry(),
            make_label_entry(),
            make_session_info_entry(),
            make_decision_audit_entry(),
        ]
    }

    #[test]
    fn session_entry_all_variants_entry_id() {
        for entry in all_variants() {
            assert!(
                !entry.entry_id().is_empty(),
                "entry_id empty for {:?}",
                entry
            );
        }
    }

    #[test]
    fn session_entry_all_variants_parent_id_default_none() {
        for entry in all_variants() {
            assert!(
                entry.parent_id().is_none(),
                "expected None parent_id for {:?}",
                entry
            );
        }
    }

    #[test]
    fn session_entry_set_parent_id_message() {
        let mut entry = make_message_entry();
        entry.set_parent_id("parent-1".into());
        assert_eq!(entry.parent_id(), Some("parent-1"));
    }

    #[test]
    fn session_entry_set_parent_id_branch() {
        let mut entry = make_branch_entry();
        entry.set_parent_id("parent-1".into());
        assert_eq!(entry.parent_id(), Some("parent-1"));
    }

    // ── MessageEntry::new ──

    #[test]
    fn message_entry_new_constructor() {
        let entry = MessageEntry::new(
            Role::Assistant,
            vec![ContentBlock::Text { text: "ok".into() }],
        );
        assert!(!entry.id.is_empty());
        assert!(entry.parent_id.is_none());
        assert_eq!(entry.role, Role::Assistant);
        assert_eq!(entry.content.len(), 1);
        assert!(entry.usage.is_none());
    }

    // ── SessionHeader ──

    #[test]
    fn session_header_construction() {
        let h = SessionHeader::new("sid-1".into(), "gpt-4".into(), "/home".into());
        assert_eq!(h.id, "sid-1");
        assert_eq!(h.model, "gpt-4");
        assert_eq!(h.working_dir, "/home");
        assert_eq!(h.version, 2);
        assert!(h.title.is_none());
    }

    #[test]
    fn session_header_with_title() {
        let h = SessionHeader::new("sid-1".into(), "gpt-4".into(), "/home".into())
            .with_title("My Session".into());
        assert_eq!(h.title.unwrap(), "My Session");
    }

    #[test]
    fn session_header_serde_roundtrip() {
        let h = SessionHeader::new("sid-1".into(), "gpt-4".into(), "/home".into());
        let json = serde_json::to_string(&h).unwrap();
        let decoded: SessionHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(h.id, decoded.id);
        assert_eq!(h.model, decoded.model);
        assert_eq!(h.version, decoded.version);
    }

    // ── SessionMetadata ──

    #[test]
    fn session_metadata_from_header() {
        let h = SessionHeader::new("sid-1".into(), "gpt-4".into(), "/home".into());
        let meta = SessionMetadata::from(h);
        assert_eq!(meta.id, "sid-1");
        assert_eq!(meta.model, "gpt-4");
        assert_eq!(meta.message_count, 0);
        assert!(meta.title.is_none());
    }

    // ── Serde roundtrip for all entry variants ──

    #[test]
    fn session_entry_serde_roundtrip_message() {
        let entry = make_message_entry();
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.entry_id(), decoded.entry_id());
    }

    #[test]
    fn session_entry_serde_roundtrip_branch() {
        let entry = make_branch_entry();
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.entry_id(), decoded.entry_id());
    }

    #[test]
    fn session_entry_serde_roundtrip_compaction() {
        let entry = make_compaction_entry();
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.entry_id(), decoded.entry_id());
    }

    #[test]
    fn session_entry_serde_roundtrip_decision_audit() {
        let entry = make_decision_audit_entry();
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.entry_id(), decoded.entry_id());
    }

    // ── SystemEventType serde ──

    #[test]
    fn system_event_type_serde_roundtrip() {
        let cases = vec![
            SystemEventType::SessionStart,
            SystemEventType::SessionEnd,
            SystemEventType::PhaseSummary,
            SystemEventType::Error,
            SystemEventType::Compaction,
        ];
        for original in cases {
            let json = serde_json::to_string(&original).unwrap();
            let decoded: SystemEventType = serde_json::from_str(&json).unwrap();
            // Compare via debug string since no PartialEq
            assert_eq!(format!("{:?}", original), format!("{:?}", decoded));
        }
    }

    // ── SessionNode / SessionTree ──

    #[test]
    fn session_node_construction() {
        let node = SessionNode {
            id: "n1".into(),
            title: Some("root".into()),
            model: "gpt-4".into(),
            message_count: 5,
            children: vec![],
        };
        assert_eq!(node.id, "n1");
        assert_eq!(node.message_count, 5);
        assert!(node.children.is_empty());
    }

    #[test]
    fn session_tree_construction() {
        let tree = SessionTree {
            root: SessionNode {
                id: "root".into(),
                title: None,
                model: "gpt-4".into(),
                message_count: 0,
                children: vec![],
            },
        };
        assert_eq!(tree.root.id, "root");
    }

    #[test]
    fn session_tree_serde_roundtrip() {
        let tree = SessionTree {
            root: SessionNode {
                id: "root".into(),
                title: Some("title".into()),
                model: "claude".into(),
                message_count: 3,
                children: vec![SessionNode {
                    id: "child".into(),
                    title: None,
                    model: "claude".into(),
                    message_count: 1,
                    children: vec![],
                }],
            },
        };
        let json = serde_json::to_string(&tree).unwrap();
        let decoded: SessionTree = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.root.id, "root");
        assert_eq!(decoded.root.children.len(), 1);
        assert_eq!(decoded.root.children[0].id, "child");
    }

    // ── Inner entry type serde roundtrips ──

    #[test]
    fn test_system_entry_serde() {
        let entry = SystemEntry {
            id: "sys-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            event: SystemEventType::Error,
            data: serde_json::json!({"error": "something failed"}),
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: SystemEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "sys-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(
            format!("{:?}", decoded.event),
            format!("{:?}", SystemEventType::Error)
        );
        assert_eq!(
            decoded.data,
            serde_json::json!({"error": "something failed"})
        );
    }

    #[test]
    fn test_compaction_entry_serde() {
        let entry = CompactionEntry {
            id: "comp-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            summary: "compacted summary".into(),
            first_kept_entry_id: "entry-5".into(),
            tokens_before: 5000,
            files_read: vec!["a.rs".into(), "b.rs".into()],
            files_modified: vec!["c.rs".into()],
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: CompactionEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "comp-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.summary, "compacted summary");
        assert_eq!(decoded.first_kept_entry_id, "entry-5");
        assert_eq!(decoded.tokens_before, 5000);
        assert_eq!(decoded.files_read, vec!["a.rs", "b.rs"]);
        assert_eq!(decoded.files_modified, vec!["c.rs"]);
    }

    #[test]
    fn test_model_change_entry_serde() {
        let entry = ModelChangeEntry {
            id: "mc-1".into(),
            parent_id: None,
            timestamp: Utc::now(),
            provider: "openai".into(),
            model_id: "gpt-4".into(),
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: ModelChangeEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "mc-1");
        assert_eq!(decoded.parent_id, None);
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.provider, "openai");
        assert_eq!(decoded.model_id, "gpt-4");
    }

    #[test]
    fn test_thinking_level_change_entry_serde() {
        let entry = ThinkingLevelChangeEntry {
            id: "tlc-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            thinking_level: ThinkingLevel::XHigh,
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: ThinkingLevelChangeEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "tlc-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.thinking_level, ThinkingLevel::XHigh);
    }

    #[test]
    fn test_branch_summary_entry_serde() {
        let entry = BranchSummaryEntry {
            id: "bs-1".into(),
            parent_id: None,
            timestamp: Utc::now(),
            from_id: "branch-1".into(),
            summary: "all tests passed".into(),
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: BranchSummaryEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "bs-1");
        assert_eq!(decoded.parent_id, None);
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.from_id, "branch-1");
        assert_eq!(decoded.summary, "all tests passed");
    }

    #[test]
    fn test_custom_entry_serde() {
        let entry = CustomEntry {
            id: "cust-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            custom_type: "custom_type_1".into(),
            data: Some(serde_json::json!({"key": "value"})),
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: CustomEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "cust-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.custom_type, "custom_type_1");
        assert_eq!(decoded.data, Some(serde_json::json!({"key": "value"})));
    }

    #[test]
    fn test_custom_entry_serde_none_data() {
        let entry = CustomEntry {
            id: "cust-2".into(),
            parent_id: None,
            timestamp: Utc::now(),
            custom_type: "no_data_type".into(),
            data: None,
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: CustomEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "cust-2");
        assert_eq!(decoded.data, None);
    }

    #[test]
    fn test_custom_message_entry_serde() {
        let entry = CustomMessageEntry {
            id: "cm-1".into(),
            parent_id: None,
            timestamp: Utc::now(),
            custom_type: "announcement".into(),
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
            display: true,
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: CustomMessageEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "cm-1");
        assert_eq!(decoded.parent_id, None);
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.custom_type, "announcement");
        assert_eq!(decoded.content.len(), 1);
        assert!(decoded.display);
    }

    #[test]
    fn test_label_entry_serde() {
        let entry = LabelEntry {
            id: "lbl-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            target_id: "msg-1".into(),
            label: Some("important".into()),
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: LabelEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "lbl-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.target_id, "msg-1");
        assert_eq!(decoded.label, Some("important".into()));
    }

    #[test]
    fn test_label_entry_serde_none_label() {
        let entry = LabelEntry {
            id: "lbl-2".into(),
            parent_id: None,
            timestamp: Utc::now(),
            target_id: "msg-2".into(),
            label: None,
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: LabelEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "lbl-2");
        assert_eq!(decoded.label, None);
    }

    #[test]
    fn test_session_info_entry_serde() {
        let entry = SessionInfoEntry {
            id: "si-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            name: Some("my-session".into()),
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: SessionInfoEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "si-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.name, Some("my-session".into()));
    }

    #[test]
    fn test_session_info_entry_serde_none_name() {
        let entry = SessionInfoEntry {
            id: "si-2".into(),
            parent_id: None,
            timestamp: Utc::now(),
            name: None,
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: SessionInfoEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "si-2");
        assert_eq!(decoded.name, None);
    }

    #[test]
    fn test_decision_audit_entry_serde() {
        let entry = DecisionAuditEntry {
            id: "da-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            turn_id: "turn-1".into(),
            tool_name: "bash".into(),
            allowed: true,
            reason: Some("user approved".into()),
            adjudication_duration_ms: 150,
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: DecisionAuditEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "da-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.turn_id, "turn-1");
        assert_eq!(decoded.tool_name, "bash");
        assert!(decoded.allowed);
        assert_eq!(decoded.reason.as_deref(), Some("user approved"));
        assert_eq!(decoded.adjudication_duration_ms, 150);
    }

    #[test]
    fn test_decision_audit_entry_serde_denied() {
        let entry = DecisionAuditEntry {
            id: "da-2".into(),
            parent_id: None,
            timestamp: Utc::now(),
            turn_id: "turn-2".into(),
            tool_name: "write".into(),
            allowed: false,
            reason: Some("blocked by security policy".into()),
            adjudication_duration_ms: 5,
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: DecisionAuditEntry = serde_json::from_value(val).unwrap();
        assert!(!decoded.allowed);
        assert_eq!(decoded.tool_name, "write");
        assert_eq!(decoded.adjudication_duration_ms, 5);
    }

    #[test]
    fn test_leaf_entry_serde() {
        let entry = LeafEntry {
            id: "leaf-1".into(),
            parent_id: Some("parent-1".into()),
            timestamp: Utc::now(),
            target_id: "branch-2".into(),
        };
        let val = serde_json::to_value(&entry).unwrap();
        let decoded: LeafEntry = serde_json::from_value(val).unwrap();
        assert_eq!(decoded.id, "leaf-1");
        assert_eq!(decoded.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(decoded.timestamp, entry.timestamp);
        assert_eq!(decoded.target_id, "branch-2");
    }

    // ── From<Message> for MessageEntry ──

    #[test]
    fn test_message_entry_from_message() {
        let msg = Message {
            id: "msg-1".into(),
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: "response".into(),
            }],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let entry = MessageEntry::from(msg);
        assert_eq!(entry.id, "msg-1");
        assert_eq!(entry.role, Role::Assistant);
        assert_eq!(entry.content.len(), 1);
        assert!(entry.usage.is_none());
        assert!(entry.parent_id.is_none());
    }

    #[test]
    fn test_message_entry_from_message_empty_id_generates_new() {
        let msg = Message {
            id: String::new(),
            role: Role::User,
            content: vec![],
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        };
        let entry = MessageEntry::from(msg);
        assert!(!entry.id.is_empty(), "empty id should be replaced");
    }
}
