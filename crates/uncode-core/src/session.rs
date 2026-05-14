use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::message::{ContentBlock, Message, Role, UsageInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub id: String,
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
            entry_type: "header".into(),
            id,
            created_at: now,
            updated_at: now,
            model,
            title: None,
            working_dir,
        }
    }
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEntry {
    pub timestamp: DateTime<Utc>,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
}

impl MessageEntry {
    pub fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self {
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
            timestamp: Utc::now(),
            role: msg.role,
            content: msg.content,
            usage: msg.usage,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEntry {
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
    pub timestamp: DateTime<Utc>,
    pub parent_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
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
            created_at: h.created_at,
            updated_at: h.updated_at,
            message_count: 0,
            title: h.title,
            working_dir: h.working_dir,
            model: h.model,
        }
    }
}
