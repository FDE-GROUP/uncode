use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::message::{Role, UsageInfo};
use crate::tool::ToolContent;

use crate::api_types::ThinkingLevel;

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
    /// Tool execution blocked until user confirms (TUI permission gate).
    ToolCallAwaitingApproval {
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
        /// Built-in tool description from registry (shown in TUI confirm UI).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_description: Option<String>,
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

    // ── Session compaction lifecycle ──
    CompactionStart {
        #[serde(flatten)]
        data: Box<CompactionStartData>,
    },
    CompactionComplete {
        messages_replaced: usize,
        tokens_before: u64,
        tokens_after: u64,
        summary_text: String,
        #[serde(default)]
        reason: CompactionReason,
    },

    // ── Auto retry ──
    RetryAttempt {
        #[serde(flatten)]
        data: Box<RetryAttemptData>,
    },

    // ── Model / Thinking change ──
    ModelChanged {
        #[serde(flatten)]
        data: Box<ModelChangedData>,
    },
    ThinkingLevelChanged {
        #[serde(flatten)]
        data: Box<ThinkingLevelChangedData>,
    },

    // ── Message queue ──
    MessageQueued {
        text: String,
    },
    MessageDelivered {
        text: String,
    },

    // ── LLM request/response lifecycle ──
    LlmRequestStart {
        #[serde(flatten)]
        data: Box<LlmRequestStartData>,
    },
    LlmRequestEnd {
        #[serde(flatten)]
        data: Box<LlmRequestEndData>,
    },

    // ── Queue state ──
    QueueUpdate {
        #[serde(flatten)]
        data: Box<QueueUpdateData>,
    },

    // ── Session info change ──
    SessionInfoChanged {
        #[serde(flatten)]
        data: Box<SessionInfoChangedData>,
    },

    // ── Context usage warning ──
    ContextThreshold {
        #[serde(flatten)]
        data: Box<ContextThresholdData>,
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

    // ── Uncertainty (认知层 — 认知与决策驱动设计) ──
    /// Emitted when an uncertainty is classified and resolved.
    /// Maps to UncertaintyClass in cognition/uncertainty.rs.
    /// Part of the Cognitive Layer in the Cognition & Decision-Driven Design paradigm.
    /// See `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3
    UncertaintyEncountered {
        /// "generative" | "cognitive" | "executional"
        uncertainty_kind: String,
        turn_id: String,
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolution_strategy: Option<String>,
    },

    // ── Decision audit (认知与决策驱动设计) ──
    /// Emitted when the adjudication pipeline rejects or approves an action.
    /// Part of the Decision Layer in the Cognition & Decision-Driven Design paradigm.
    /// See `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3
    DecisionMade {
        turn_id: String,
        tool_name: String,
        allowed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },

    // ── Evaluation (决策层审计 — H0-H3 评估阶梯) ──
    /// Emitted after turn evaluation completes.
    /// Part of the Harness Engineering evaluation framework.
    /// See `docs/ai-agent-archi/cognition-decision-driven-design.md` §4.1
    EvaluationScore {
        turn_id: String,
        /// "H0" | "H1" | "H2" | "H3"
        level: String,
        /// 0.0 - 1.0
        quality_score: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
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

/// Compaction 触发原因。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CompactionReason {
    /// Token 接近上下文窗口阈值
    #[default]
    Threshold,
    /// LLM 返回 context overflow 错误后自动触发
    Overflow,
    /// 用户或系统手动触发
    Manual,
}

/// Model 变更来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModelChangeSource {
    /// 用户主动切换
    User,
    /// prepare_next_turn 自动决策
    Auto,
    /// 系统内部逻辑（如 fallback）
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStartData {
    pub session_id: String,
    pub reason: CompactionReason,
    pub tokens_before: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAttemptData {
    pub attempt: u32,
    pub max_attempts: u32,
    pub delay_ms: u64,
    pub error: String,
    pub final_success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChangedData {
    pub from: Option<String>,
    pub to: String,
    pub source: ModelChangeSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingLevelChangedData {
    pub from: Option<ThinkingLevel>,
    pub to: ThinkingLevel,
}

// ── E4: LLM request/response ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequestStartData {
    pub model_id: String,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequestEndData {
    pub model_id: String,
    pub duration_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub status: LlmRequestStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmRequestStatus {
    Success,
    Error,
    Cancelled,
}

// ── E5: Queue state ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueUpdateData {
    pub steering_count: usize,
    pub follow_up_count: usize,
    pub next_turn_count: usize,
}

// ── E6: Session info change ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoChangedData {
    pub session_id: String,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

// ── E8: Context threshold ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextThresholdData {
    pub session_id: String,
    pub usage_ratio: f64,
    pub threshold: f64,
    pub context_window: u64,
}

/// Hook 返回值 — 事件监听器可返回控制指令修改 Agent 行为。
///
/// **Pi:** 对应 Harness hook 的 typed return（block context / patch tool result / cancel compact 等）。
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
///
/// **Pi:** 对应 `AgentHarness.on(event, handler)`；非 Pi 全套 Harness Hook 的超集实现。
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
        let tag = agent_event_tag(event);
        if let Some(handlers) = self.sync_handlers.get(tag) {
            for h in handlers {
                h(event);
            }
        }
    }

    /// Dispatch an event to all hook handlers and collect results.
    /// Returns `Vec<HookResult>` for the caller to aggregate.
    pub async fn dispatch_hooks(&self, event: &AgentEvent) -> Vec<HookResult> {
        let tag = agent_event_tag(event);
        let mut results = Vec::new();
        if let Some(handlers) = self.hook_handlers.get(tag) {
            for h in handlers {
                results.push(h(event).await);
            }
        }
        results
    }
}

/// Serde `type` 字段（snake_case），与 `#[serde(tag = "type")]` 输出一致。
pub fn agent_event_tag(event: &AgentEvent) -> &'static str {
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
        AgentEvent::ToolCallAwaitingApproval { .. } => "tool_call_awaiting_approval",
        AgentEvent::ToolCallEnd { .. } => "tool_call_end",
        AgentEvent::TaskUpdate { .. } => "task_update",
        AgentEvent::PhaseSummary { .. } => "phase_summary",
        AgentEvent::CompactionStart { .. } => "compaction_start",
        AgentEvent::CompactionComplete { .. } => "compaction_complete",
        AgentEvent::RetryAttempt { .. } => "retry_attempt",
        AgentEvent::ModelChanged { .. } => "model_changed",
        AgentEvent::ThinkingLevelChanged { .. } => "thinking_level_changed",
        AgentEvent::MessageQueued { .. } => "message_queued",
        AgentEvent::MessageDelivered { .. } => "message_delivered",
        AgentEvent::LlmRequestStart { .. } => "llm_request_start",
        AgentEvent::LlmRequestEnd { .. } => "llm_request_end",
        AgentEvent::QueueUpdate { .. } => "queue_update",
        AgentEvent::SessionInfoChanged { .. } => "session_info_changed",
        AgentEvent::ContextThreshold { .. } => "context_threshold",
        AgentEvent::Error { .. } => "error",
        AgentEvent::AgentInterrupted { .. } => "agent_interrupted",
        AgentEvent::UncertaintyEncountered { .. } => "uncertainty_encountered",
        AgentEvent::DecisionMade { .. } => "decision_made",
        AgentEvent::EvaluationScore { .. } => "evaluation_score",
        AgentEvent::AgentSettled { .. } => "agent_settled",
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Pi 四层 `AgentEvent` 在文档级 1:1 映射的 uncode 标签（见 `UNCODE_PI_MECHANISM_MAP.md` §5.1）。
pub fn pi_equivalent_event_name(uncode_tag: &str) -> Option<&'static str> {
    match uncode_tag {
        "session_start" => Some("agent_start"),
        "session_end" => Some("agent_end"),
        "turn_start" => Some("turn_start"),
        "turn_end" => Some("turn_end"),
        "message_start" => Some("message_start"),
        "message_end" => Some("message_end"),
        "content_delta" => Some("message_update"),
        "tool_call_start" => Some("tool_execution_start"),
        "tool_call_progress" => Some("tool_execution_update"),
        "tool_call_end" => Some("tool_execution_end"),
        "compaction_start" => Some("compaction_start"),
        "compaction_complete" => Some("compaction_complete"),
        "retry_attempt" => Some("auto_retry_attempt"),
        "model_changed" => Some("model_select"),
        "thinking_level_changed" => Some("thinking_level_select"),
        "llm_request_start" => Some("llm_request_start"),
        "llm_request_end" => Some("llm_request_end"),
        "queue_update" => Some("queue_update"),
        "context_threshold" => Some("context_threshold"),
        _ => None,
    }
}

/// 单轮 ReAct 内事件的生命周期秩（越大越靠后）。会话级/队列等事件返回 `None` 并跳过。
fn turn_lifecycle_rank(tag: &str) -> Option<u8> {
    match tag {
        "turn_start" => Some(0),
        "message_start" => Some(1),
        "content_delta" | "tool_call_start" => Some(2),
        "tool_call_progress" => Some(3),
        "tool_call_end" => Some(4),
        "message_end" => Some(5),
        "turn_end" => Some(6),
        _ => None,
    }
}

/// 校验事件切片是否符合 Pi 式 turn 内顺序（`turn_start` … `turn_end`）。
///
/// 用于 fixture 测试与回归；不校验跨 turn 的 `session_*` / 队列事件。
pub fn validate_pi_turn_lifecycle_order(events: &[AgentEvent]) -> Result<(), String> {
    let mut last_rank: Option<u8> = None;
    let mut in_turn = false;

    for event in events {
        let tag = agent_event_tag(event);
        let Some(rank) = turn_lifecycle_rank(tag) else {
            continue;
        };

        if tag == "turn_start" {
            in_turn = true;
            last_rank = Some(rank);
            continue;
        }

        if !in_turn {
            return Err(format!("{tag} before turn_start"));
        }

        if let Some(prev) = last_rank
            && rank < prev
        {
            return Err(format!(
                "out of order: {tag} (rank {rank}) after rank {prev}"
            ));
        }
        last_rank = Some(rank);

        if tag == "turn_end" {
            in_turn = false;
            last_rank = None;
        }
    }

    if in_turn {
        return Err("turn_start without matching turn_end".into());
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Event detail level — 认知与决策驱动设计 治理层 §3.2
// ═══════════════════════════════════════════════════════════════

/// 事件重要性分级（用于导出和保留策略）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EventDetailLevel {
    /// 必须记录 — 关键决策和系统事件
    Critical = 0,
    /// 默认记录 — 常规流程
    Standard = 1,
    /// 仅调试时记录 — 高频细粒度事件
    Verbose = 2,
}

impl AgentEvent {
    /// 返回事件的重要性分级
    ///
    /// 对应 `docs/ai-agent-archi/uncodenow-refactoring-roadmap.md` §3.2
    pub fn detail_level(&self) -> EventDetailLevel {
        match self {
            // Critical: 关键决策和生命周期事件
            Self::SessionStart { .. }
            | Self::SessionEnd { .. }
            | Self::TurnStart { .. }
            | Self::TurnEnd { .. }
            | Self::ToolCallEnd { .. }
            | Self::DecisionMade { .. }
            | Self::Error { .. }
            | Self::CompactionComplete { .. }
            | Self::AgentInterrupted { .. } => EventDetailLevel::Critical,

            // Verbose: 高频细粒度事件
            Self::ToolCallProgress { .. } | Self::ToolCallAwaitingApproval { .. } => {
                EventDetailLevel::Verbose
            }

            // Standard: 其余所有
            _ => EventDetailLevel::Standard,
        }
    }
}

#[cfg(test)]
mod pi_event_fixture {
    use super::*;
    use crate::message::UsageInfo;

    /// 文档 §5.1 中标注为 1:1 的 Pi ↔ uncode 标签对（fixture 表）。
    const PI_ONE_TO_ONE: &[(&str, &str)] = &[
        ("turn_start", "turn_start"),
        ("turn_end", "turn_end"),
        ("message_start", "message_start"),
        ("message_end", "message_end"),
        ("tool_call_start", "tool_execution_start"),
        ("tool_call_progress", "tool_execution_update"),
        ("tool_call_end", "tool_execution_end"),
    ];

    #[test]
    fn pi_one_to_one_mapping_table() {
        for (uncode_tag, pi_name) in PI_ONE_TO_ONE {
            assert_eq!(
                pi_equivalent_event_name(uncode_tag),
                Some(*pi_name),
                "uncode tag {uncode_tag}"
            );
        }
    }

    #[test]
    fn pi_minimal_text_turn_fixture() {
        let events = vec![
            AgentEvent::TurnStart { turn: 1 },
            AgentEvent::MessageStart {
                role: crate::message::Role::Assistant,
                message_id: "m1".into(),
            },
            AgentEvent::ContentDelta {
                delta_type: DeltaType::Text,
                content: "hi".into(),
                content_index: None,
            },
            AgentEvent::MessageEnd {
                role: crate::message::Role::Assistant,
                message_id: "m1".into(),
            },
            AgentEvent::TurnEnd {
                turn: 1,
                usage: UsageInfo::default(),
            },
        ];
        validate_pi_turn_lifecycle_order(&events).expect("minimal text turn");
    }

    #[test]
    fn pi_tool_turn_fixture() {
        let events = vec![
            AgentEvent::TurnStart { turn: 2 },
            AgentEvent::MessageStart {
                role: crate::message::Role::Assistant,
                message_id: "m2".into(),
            },
            AgentEvent::ToolCallStart {
                tool_id: "t1".into(),
                tool_name: "read".into(),
                arguments_summary: "{}".into(),
            },
            AgentEvent::ToolCallProgress {
                tool_id: "t1".into(),
                progress_type: ProgressType::Spinner,
                detail: "".into(),
            },
            AgentEvent::ToolCallEnd {
                data: Box::new(ToolCallEndEventData {
                    tool_id: "t1".into(),
                    tool_name: "read".into(),
                    arguments: "{}".into(),
                    status: ToolCallStatus::Success,
                    duration_ms: 1,
                    output_size: None,
                    result_summary: None,
                    is_error: false,
                }),
            },
            AgentEvent::MessageEnd {
                role: crate::message::Role::Assistant,
                message_id: "m2".into(),
            },
            AgentEvent::TurnEnd {
                turn: 2,
                usage: UsageInfo::default(),
            },
        ];
        validate_pi_turn_lifecycle_order(&events).expect("tool turn");
    }

    #[test]
    fn pi_turn_order_rejects_inverted_end() {
        let events = vec![
            AgentEvent::TurnEnd {
                turn: 1,
                usage: UsageInfo::default(),
            },
            AgentEvent::TurnStart { turn: 1 },
        ];
        assert!(validate_pi_turn_lifecycle_order(&events).is_err());
    }
}
