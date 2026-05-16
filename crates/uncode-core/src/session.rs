use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api_types::ThinkingLevel;
use crate::message::{ContentBlock, Message, Role, UsageInfo};

/// Generate a time-sortable entry ID from UUIDv7 (full simple format for uniqueness).
pub fn generate_entry_id() -> String {
    uuid::Uuid::now_v7().simple().to_string()
}

fn default_session_version() -> u32 {
    1
}

// ── SessionHeader ──

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionEntry {
    #[serde(rename = "message")]
    Message(MessageEntry),
    #[serde(rename = "system")]
    System(SystemEntry),
    #[serde(rename = "branch")]
    Branch(BranchEntry),
    #[serde(rename = "leaf")]
    Leaf(LeafEntry),
    #[serde(rename = "compaction")]
    Compaction(CompactionEntry),
    #[serde(rename = "model_change")]
    ModelChange(ModelChangeEntry),
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange(ThinkingLevelChangeEntry),
    #[serde(rename = "branch_summary")]
    BranchSummary(BranchSummaryEntry),
    #[serde(rename = "custom")]
    Custom(CustomEntry),
    #[serde(rename = "custom_message")]
    CustomMessage(CustomMessageEntry),
    #[serde(rename = "label")]
    Label(LabelEntry),
    #[serde(rename = "session_info")]
    SessionInfo(SessionInfoEntry),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_read: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_modified: Option<Vec<String>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTree {
    pub root: SessionNode,
}
