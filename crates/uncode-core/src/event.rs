use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::message::{Role, UsageInfo};
use crate::tool::ToolContent;

// ── Boxed data structs for large AgentEvent variants ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndData {
    pub session_id: String,
    pub total_turns: u64,
    pub total_tokens: UsageInfo,
    pub exit_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEndEventData {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub duration_ms: u64,
    pub output_size: Option<usize>,
    pub result_summary: Option<String>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUpdateData {
    pub task_id: String,
    pub status: TaskStatus,
    pub title: String,
    pub subtasks: Vec<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseSummaryData {
    pub phase: u64,
    pub completed: Vec<String>,
    pub issues: Vec<String>,
    pub next_steps: Vec<String>,
    pub token_usage: UsageInfo,
}

/// Agent 向 TUI/Platform 广播的事件，驱动对话区更新。
///
/// **Pi:** 对应终端四层 `AgentEvent`（`agent_*` / `turn_*` / `message_*` / `tool_execution_*`）
/// 及 Harness 观察事件；完整 1:1 / 1:N 对照见 `docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md` §5。
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
        #[serde(flatten)]
        data: Box<SessionEndData>,
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
        #[serde(flatten)]
        data: Box<ToolCallEndEventData>,
    },

    // ── Task/Phase (reserved) ──
    TaskUpdate {
        #[serde(flatten)]
        data: Box<TaskUpdateData>,
    },
    PhaseSummary {
        #[serde(flatten)]
        data: Box<PhaseSummaryData>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCallStatus {
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCategory {
    Llm,
    Tool,
    Network,
    Config,
}

/// Hook 返回值 — 事件监听器可返回控制指令修改 Agent 行为
#[derive(Debug, Clone, Default)]
pub enum HookResult {
    /// 无干预，继续正常流程
    #[default]
    Continue,
    /// 阻止执行（如阻止工具调用）
    Block { reason: String },
    /// 替换上下文消息数组
    PatchMessages {
        messages: Vec<crate::message::Message>,
    },
    /// 修改工具执行结果
    PatchToolResult {
        content: Option<Vec<ToolContent>>,
        terminate: Option<bool>,
    },
    /// 取消压缩操作
    CancelCompaction,
}

/// 同步观察 handler（fire-and-forget，不返回值）
pub type SyncEventHandler = Box<dyn Fn(&AgentEvent) + Send + Sync>;

/// 异步 hook handler（可返回 HookResult 控制指令）
pub type AsyncHookHandler =
    Box<dyn for<'a> Fn(&'a AgentEvent) -> BoxFuture<'a, HookResult> + Send + Sync>;

/// EventRouter dispatches AgentEvents to type-specific handlers,
/// aligned with Pi's `on(type, handler)` subscription pattern.
///
/// 双通道设计：
/// - sync_handlers：观察型，fire-and-forget
/// - hook_handlers：控制型，异步返回 HookResult
pub struct EventRouter {
    sync_handlers: std::collections::HashMap<String, Vec<SyncEventHandler>>,
    hook_handlers: std::collections::HashMap<String, Vec<AsyncHookHandler>>,
}

impl EventRouter {
    pub fn new() -> Self {
        Self {
            sync_handlers: std::collections::HashMap::new(),
            hook_handlers: std::collections::HashMap::new(),
        }
    }

    /// Register a sync observation handler for a specific event type (by serde tag name).
    pub fn on(&mut self, event_type: &str, handler: SyncEventHandler) {
        self.sync_handlers
            .entry(event_type.to_string())
            .or_default()
            .push(handler);
    }

    /// Register an async hook handler that can return control instructions.
    pub fn on_hook(&mut self, event_type: &str, handler: AsyncHookHandler) {
        self.hook_handlers
            .entry(event_type.to_string())
            .or_default()
            .push(handler);
    }

    /// Dispatch an event to all sync handlers (fire-and-forget).
    pub fn dispatch(&self, event: &AgentEvent) {
        let tag = event_tag(event);
        if let Some(handlers) = self.sync_handlers.get(tag) {
            for h in handlers {
                h(event);
            }
        }
    }

    /// Dispatch an event to all hook handlers and collect results.
    /// Returns Vec<HookResult> for the caller to aggregate.
    pub async fn dispatch_hooks(&self, event: &AgentEvent) -> Vec<HookResult> {
        let tag = event_tag(event);
        let mut results = Vec::new();
        if let Some(handlers) = self.hook_handlers.get(tag) {
            for h in handlers {
                results.push(h(event).await);
            }
        }
        results
    }
}

/// Extract the serde tag name from an AgentEvent without serializing.
/// Matches the `#[serde(tag = "type", rename_all = "snake_case")]` output.
fn event_tag(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::SessionStart { .. } => "session_start",
        AgentEvent::SessionEnd { .. } => "session_end",
        AgentEvent::TurnStart { .. } => "turn_start",
        AgentEvent::TurnEnd { .. } => "turn_end",
        AgentEvent::MessageStart { .. } => "message_start",
        AgentEvent::MessageEnd { .. } => "message_end",
        AgentEvent::ContentDelta { .. } => "content_delta",
        AgentEvent::ToolCallStart { .. } => "tool_call_start",
        AgentEvent::ToolCallProgress { .. } => "tool_call_progress",
        AgentEvent::ToolCallEnd { .. } => "tool_call_end",
        AgentEvent::TaskUpdate { .. } => "task_update",
        AgentEvent::PhaseSummary { .. } => "phase_summary",
        AgentEvent::CompactionComplete { .. } => "compaction_complete",
        AgentEvent::MessageQueued { .. } => "message_queued",
        AgentEvent::MessageDelivered { .. } => "message_delivered",
        AgentEvent::Error { .. } => "error",
        AgentEvent::AgentInterrupted { .. } => "agent_interrupted",
        AgentEvent::AgentSettled { .. } => "agent_settled",
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}
