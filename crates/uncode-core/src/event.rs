use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::message::{Role, UsageInfo};

/// Agent 向 TUI/Platform 广播的事件，驱动对话区更新
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentEvent {
    // ── Session lifecycle ──
    SessionStart {
        session_id: String,
        timestamp: DateTime<Utc>,
    },
    SessionEnd {
        session_id: String,
        total_turns: u64,
        total_tokens: UsageInfo,
        exit_reason: String,
    },

    // ── Turn lifecycle ──
    TurnStart {
        turn: u64,
    },
    TurnEnd {
        turn: u64,
        usage: UsageInfo,
    },

    // ── Message lifecycle ──
    MessageStart {
        role: Role,
        message_id: String,
    },
    MessageEnd {
        role: Role,
        message_id: String,
    },

    // ── Content streaming ──
    ContentDelta {
        delta_type: DeltaType,
        content: String,
        content_index: Option<usize>,
    },

    // ── Tool execution lifecycle ──
    ToolCallStart {
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
    },
    ToolCallProgress {
        tool_id: String,
        progress_type: ProgressType,
        detail: String,
    },
    ToolCallEnd {
        tool_id: String,
        tool_name: String,
        arguments: String,
        status: ToolCallStatus,
        duration_ms: u64,
        output_size: Option<usize>,
        result_summary: Option<String>,
        is_error: bool,
    },

    // ── Task/Phase (reserved) ──
    TaskUpdate {
        task_id: String,
        status: TaskStatus,
        title: String,
        subtasks: Vec<String>,
        depends_on: Vec<String>,
    },
    PhaseSummary {
        phase: u64,
        completed: Vec<String>,
        issues: Vec<String>,
        next_steps: Vec<String>,
        token_usage: UsageInfo,
    },

    // ── Session compaction ──
    CompactionComplete {
        messages_replaced: usize,
        tokens_before: u64,
        tokens_after: u64,
        summary_text: String,
    },

    // ── Message queue ──
    MessageQueued {
        text: String,
    },
    MessageDelivered {
        text: String,
    },

    // ── Error / Interrupt ──
    Error {
        category: ErrorCategory,
        message: String,
        recoverable: bool,
    },
    AgentInterrupted {
        turn: u64,
        partial_response: bool,
    },

    // ── Settled state (after SessionEnd, agent fully idle) ──
    AgentSettled {
        session_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DeltaType {
    Thinking,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProgressType {
    Spinner,
    Percentage { current: u64, total: u64 },
    LogLine,
    Stdout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCallStatus {
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCategory {
    Llm,
    Tool,
    Network,
    Config,
}

/// Type-erased handler for a specific AgentEvent variant.
pub type EventHandler = Box<dyn Fn(&AgentEvent) + Send + Sync>;

/// EventRouter dispatches AgentEvents to type-specific handlers,
/// aligned with Pi's `on(type, handler)` subscription pattern.
pub struct EventRouter {
    handlers: std::collections::HashMap<String, Vec<EventHandler>>,
}

impl EventRouter {
    pub fn new() -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
        }
    }

    /// Register a handler for a specific event type (by serde tag name).
    pub fn on(&mut self, event_type: &str, handler: EventHandler) {
        self.handlers
            .entry(event_type.to_string())
            .or_default()
            .push(handler);
    }

    /// Dispatch an event to all registered handlers for its type.
    pub fn dispatch(&self, event: &AgentEvent) {
        let tag = match serde_json::to_value(event) {
            Ok(serde_json::Value::Object(map)) => map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => return,
        };
        if let Some(handlers) = self.handlers.get(&tag) {
            for h in handlers {
                h(event);
            }
        }
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}
