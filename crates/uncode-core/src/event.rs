use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::message::UsageInfo;

/// Agent 向 TUI/Platform 广播的事件，驱动对话区更新
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentEvent {
    SessionStart {
        session_id: String,
        timestamp: DateTime<Utc>,
    },
    TaskUpdate {
        task_id: String,
        status: TaskStatus,
        title: String,
        subtasks: Vec<String>,
        depends_on: Vec<String>,
    },
    ContentDelta {
        delta_type: DeltaType,
        content: String,
    },
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
    },
    PhaseSummary {
        phase: u64,
        completed: Vec<String>,
        issues: Vec<String>,
        next_steps: Vec<String>,
        token_usage: UsageInfo,
    },
    Error {
        category: ErrorCategory,
        message: String,
        recoverable: bool,
    },
    TurnEnd {
        turn: u64,
        usage: UsageInfo,
    },
    SessionEnd {
        session_id: String,
        total_turns: u64,
        total_tokens: UsageInfo,
        exit_reason: String,
    },
    CompactionComplete {
        messages_replaced: usize,
        tokens_before: u64,
        tokens_after: u64,
        summary_text: String,
    },
    MessageQueued {
        text: String,
    },
    MessageDelivered {
        text: String,
    },
    AgentInterrupted {
        turn: u64,
        partial_response: bool,
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
