use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::message::{Role, UsageInfo};
use crate::tool::ToolContent;

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
        let tag = match serde_json::to_value(event) {
            Ok(serde_json::Value::Object(map)) => map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => return,
        };
        if let Some(handlers) = self.sync_handlers.get(&tag) {
            for h in handlers {
                h(event);
            }
        }
    }

    /// Dispatch an event to all hook handlers and collect results.
    /// Returns Vec<HookResult> for the caller to aggregate.
    pub async fn dispatch_hooks(&self, event: &AgentEvent) -> Vec<HookResult> {
        let tag = match serde_json::to_value(event) {
            Ok(serde_json::Value::Object(map)) => map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => return Vec::new(),
        };
        let mut results = Vec::new();
        if let Some(handlers) = self.hook_handlers.get(&tag) {
            for h in handlers {
                results.push(h(event).await);
            }
        }
        results
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}
