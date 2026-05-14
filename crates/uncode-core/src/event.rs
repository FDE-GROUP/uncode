use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::message::UsageInfo;

/// Agent 向 TUI/Platform 广播的事件，驱动四个面板的更新
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaType {
    Thinking,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressType {
    Spinner,
    Percentage { current: u64, total: u64 },
    LogLine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Llm,
    Tool,
    Network,
    Config,
}
