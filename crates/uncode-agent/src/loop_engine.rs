//! Agent 主循环：双层 `while`、三通道队列、ReAct 工具链。
//!
//! **L1（Pi）：** 与 `agentLoop` 同构——外层 `follow_up`、内层 tool-call + `steering` drain、
//! `next_turn` 预排队、`terminate` 批次 AND 语义。见 `docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md`。

use futures::StreamExt;
use futures::stream::BoxStream;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::phase_summary::{
    PhaseSummaryLlmInput, assistant_snippet_for_phase, build_phase_summary_heuristic,
    format_tool_phase_label, summarize_tool_args, try_llm_phase_summary,
};
use crate::session::store::SessionStore;
use crate::steering::MessageQueue;
use crate::tools::local_env::LocalExecutionEnv;
use crate::tools::registry::ToolRegistry;
use uncode_ai::StreamEvent;
use uncode_ai::{ApiRegistry, ModelRegistry};
use uncode_core::api_types::{
    Context, PayloadCallback, ResponseCallback, StreamOptions, ThinkingLevel,
    TransformContextCallback,
};
use uncode_core::config::CompactionConfig;
use uncode_core::error::HarnessError;
use uncode_core::error::UncodeError;
use uncode_core::event::AgentEvent;
use uncode_core::event::{
    CompactionReason, CompactionStartData, ContextThresholdData, LlmRequestEndData,
    LlmRequestStartData, LlmRequestStatus, ModelChangeSource, ModelChangedData, QueueUpdateData,
    RetryAttemptData, SessionEndData, ThinkingLevelChangedData, ToolCallEndEventData,
    ToolCallStatus,
};
use uncode_core::message::{ContentBlock, Message, Role, UsageInfo};
use uncode_core::session::{SessionEntry, ThinkingLevelChangeEntry, generate_entry_id};
use uncode_core::tool::ExecutionEnv;
use uncode_core::tool::{
    AfterToolCallContext, BeforeToolCallContext, ExecutionMode, ToolContext, ToolHooks,
    ToolProgress, ToolResult,
};

pub const MAX_TURNS: u64 = 50;
const MAX_LLM_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 1000;

/// Turn 边界回调可返回的模型变更决策。
///
/// **Pi:** 对照 `prepareNextTurn` 返回的 model/thinking 变更。
#[derive(Debug, Default, Clone)]
pub struct NextTurnDecision {
    pub model_id: Option<String>,
    pub thinking_level: Option<ThinkingLevel>,
}

/// Execute + after-hook for parallel batch phase (shared state cloned per future).
async fn execute_prepared_tool_shared(
    registry: Arc<ToolRegistry>,
    cancel_token: CancellationToken,
    event_tx: broadcast::Sender<AgentEvent>,
    hooks: Option<Arc<dyn ToolHooks>>,
    execution_env: Arc<dyn ExecutionEnv>,
    id: String,
    name: String,
    prepared_args: serde_json::Value,
    raw_args: serde_json::Value,
) -> ToolResult {
    let started = std::time::Instant::now();
    let executor = registry.get(&name);
    let child = cancel_token.child_token();
    let ctx = ToolContext {
        cancel_token: child.clone(),
        execution_env: Some(execution_env),
        on_progress: Some(Box::new({
            let etx = event_tx.clone();
            let tid = id.clone();
            move |p: ToolProgress| {
                let detail = match &p {
                    ToolProgress::Spinner(s) => s.clone(),
                    ToolProgress::Percentage { detail, .. } => detail.clone(),
                    ToolProgress::LogLine(l) => l.clone(),
                };
                let _ = etx.send(AgentEvent::ToolCallProgress {
                    tool_id: tid.clone(),
                    progress_type: uncode_core::event::ProgressType::Spinner,
                    detail,
                });
            }
        })),
        tool_call_id: id.clone(),
    };

    let mut tool_result = if let Some(exec) = executor {
        tokio::select! {
            _ = child.cancelled() => ToolResult::err("cancelled"),
            r = exec.execute_with_context(prepared_args, ctx) => {
                match r {
                    Ok(tr) => tr,
                    Err(e) => ToolResult::err(format!("error: {e}")),
                }
            }
        }
    } else {
        ToolResult::err(format!("tool not found: {name}"))
    };

    tool_result = tool_result.with_duration_ms(started.elapsed().as_millis() as u64);

    if let Some(ref h) = hooks {
        let after_ctx = AfterToolCallContext {
            tool_call_id: id.clone(),
            tool_name: name.clone(),
            args: raw_args,
        };
        let patch = h.after_tool_call(&after_ctx, &mut tool_result).await;
        if let Some(new_content) = patch.content {
            tool_result.content = new_content;
        }
        if let Some(new_details) = patch.details {
            tool_result.details = Some(new_details);
        }
        if let Some(new_is_error) = patch.is_error {
            tool_result.is_error = new_is_error;
        }
        if let Some(new_terminate) = patch.terminate {
            tool_result.terminate = new_terminate;
        }
    }

    tool_result
}

/// Extract the first text block from a message, or "" if none.
fn first_text(msg: &Message) -> &str {
    msg.content
        .first()
        .and_then(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("")
}

/// 双层循环执行引擎（文档亦称 LoopEngine）。
///
/// **Pi:** 对应 `agentLoop` 核心；公开入口为 `run` / `run_inner`。
pub struct AgentLoop {
    api_registry: Arc<ApiRegistry>,
    model_registry: Arc<ModelRegistry>,
    api_keys: Arc<HashMap<String, String>>,
    tool_registry: Arc<ToolRegistry>,
    session_store: Arc<SessionStore>,
    system_prompt: String,
    model_id: String,
    session_id: Option<String>,
    event_tx: broadcast::Sender<AgentEvent>,
    cancel_token: CancellationToken,
    tool_hooks: Option<Arc<dyn ToolHooks>>,
    execution_env: Arc<dyn ExecutionEnv>,
    message_queue: tokio::sync::Mutex<MessageQueue>,
    should_stop_after_turn: Option<Arc<dyn Fn(u64) -> bool + Send + Sync>>,
    prepare_next_turn: Option<Arc<dyn Fn() -> Option<NextTurnDecision> + Send + Sync>>,
    transform_context: Option<TransformContextCallback>,
    on_payload: Option<PayloadCallback>,
    on_response: Option<ResponseCallback>,
    active_run: Arc<AtomicBool>,
    graph_cache: Option<Arc<crate::workspace_graph::WorkspaceGraphCache>>,
    compaction_config: CompactionConfig,
    skill_registry: Option<uncode_core::skill::SkillRegistry>,
    /// 决策层提案累积器 — 认知显化与决策驱动设计 Phase 1 连线 (#339)
    proposal_acc: std::sync::Mutex<crate::decision::proposal::ProposalAccumulator>,
    /// 语义防火墙 — 认知显化与决策驱动设计 原则2 (#339 强制执行)
    firewall: std::sync::Mutex<Option<crate::decision::firewall::SemanticFirewall>>,
    /// 演化引擎 — 认知显化与决策驱动设计 自适应进化 (#342)
    evolution: std::sync::Mutex<uncode_shared::evolution::EvolutionEngine>,
}

impl AgentLoop {
    pub fn new(
        api_registry: Arc<ApiRegistry>,
        model_registry: Arc<ModelRegistry>,
        api_keys: HashMap<String, String>,
        tool_registry: Arc<ToolRegistry>,
        session_store: Arc<SessionStore>,
        system_prompt: String,
        model_id: String,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            api_registry,
            model_registry,
            api_keys: Arc::new(api_keys),
            tool_registry,
            session_store,
            system_prompt,
            model_id,
            session_id: None,
            event_tx,
            cancel_token: CancellationToken::new(),
            tool_hooks: None,
            execution_env: Arc::new(LocalExecutionEnv::new()),
            message_queue: tokio::sync::Mutex::new(MessageQueue::new()),
            should_stop_after_turn: None,
            prepare_next_turn: None,
            transform_context: None,
            on_payload: None,
            on_response: None,
            active_run: Arc::new(AtomicBool::new(false)),
            graph_cache: None,
            compaction_config: CompactionConfig::default(),
            skill_registry: None,
            proposal_acc: std::sync::Mutex::new(
                crate::decision::proposal::ProposalAccumulator::new(),
            ),
            firewall: std::sync::Mutex::new(None),
            evolution: std::sync::Mutex::new(uncode_shared::evolution::EvolutionEngine::new(3)),
        }
    }

    /// Set compaction configuration.
    pub fn with_compaction_config(mut self, config: CompactionConfig) -> Self {
        self.compaction_config = config;
        self
    }

    /// Set skill registry for expanding `/skill-name` commands in steering messages.
    pub fn with_skill_registry(mut self, registry: uncode_core::skill::SkillRegistry) -> Self {
        self.skill_registry = Some(registry);
        self
    }

    /// Expand `/skill-name` in message text using the skill registry.
    fn expand_skill_in_message(&self, mut msg: Message) -> Message {
        let Some(ref registry) = self.skill_registry else {
            return msg;
        };
        for block in &mut msg.content {
            if let uncode_core::message::ContentBlock::Text { text } = block {
                if let Some(rest) = text.strip_prefix('/') {
                    let name = rest.split_whitespace().next().unwrap_or(rest);
                    if let Some(expanded) = registry.render(name, &std::collections::HashMap::new())
                    {
                        *text = expanded;
                    }
                }
            }
        }
        msg
    }

    /// Override the runtime used by file/shell tools (tests, remote sandbox).
    pub fn with_execution_env(mut self, env: Arc<dyn ExecutionEnv>) -> Self {
        self.execution_env = env;
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_event_sender(
        api_registry: Arc<ApiRegistry>,
        model_registry: Arc<ModelRegistry>,
        api_keys: HashMap<String, String>,
        tool_registry: Arc<ToolRegistry>,
        session_store: Arc<SessionStore>,
        system_prompt: String,
        model_id: String,
        event_tx: broadcast::Sender<AgentEvent>,
    ) -> Self {
        Self {
            api_registry,
            model_registry,
            api_keys: Arc::new(api_keys),
            tool_registry,
            session_store,
            system_prompt,
            model_id,
            session_id: None,
            event_tx,
            cancel_token: CancellationToken::new(),
            tool_hooks: None,
            execution_env: Arc::new(LocalExecutionEnv::new()),
            message_queue: tokio::sync::Mutex::new(MessageQueue::new()),
            should_stop_after_turn: None,
            prepare_next_turn: None,
            transform_context: None,
            on_payload: None,
            on_response: None,
            active_run: Arc::new(AtomicBool::new(false)),
            graph_cache: None,
            compaction_config: CompactionConfig::default(),
            skill_registry: None,
            proposal_acc: std::sync::Mutex::new(
                crate::decision::proposal::ProposalAccumulator::new(),
            ),
            firewall: std::sync::Mutex::new(None),
            evolution: std::sync::Mutex::new(uncode_shared::evolution::EvolutionEngine::new(3)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    pub fn event_sender(&self) -> broadcast::Sender<AgentEvent> {
        self.event_tx.clone()
    }

    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn set_model_id(&mut self, model_id: String) {
        self.model_id = model_id;
    }

    pub fn set_cancel_token(&mut self, token: CancellationToken) {
        self.cancel_token = token;
    }

    pub fn set_graph_cache(&mut self, cache: Arc<crate::workspace_graph::WorkspaceGraphCache>) {
        self.graph_cache = Some(cache);
    }

    pub fn set_tool_hooks(&mut self, hooks: Arc<dyn ToolHooks>) {
        self.tool_hooks = Some(hooks);
    }

    /// Restrict tools visible to the LLM and executable in this loop.
    ///
    /// **Pi:** `setActiveTools`.
    pub fn set_active_tools(&self, names: &[impl AsRef<str>]) -> Result<(), String> {
        self.tool_registry.set_active_tools(names)
    }

    /// Clear active-tool filter (all registered tools are active).
    pub fn clear_active_tools(&self) {
        self.tool_registry.clear_active_tools();
    }

    pub fn set_should_stop_after_turn(&mut self, cb: Arc<dyn Fn(u64) -> bool + Send + Sync>) {
        self.should_stop_after_turn = Some(cb);
    }

    pub fn set_prepare_next_turn(
        &mut self,
        cb: Arc<dyn Fn() -> Option<NextTurnDecision> + Send + Sync>,
    ) {
        self.prepare_next_turn = Some(cb);
    }

    pub fn set_transform_context(&mut self, cb: TransformContextCallback) {
        self.transform_context = Some(cb);
    }

    /// 观测即将发往 LLM 的 JSON 请求体（对齐 Pi `on_payload`）。
    pub fn set_on_payload(&mut self, cb: PayloadCallback) {
        self.on_payload = Some(cb);
    }

    /// 观测 LLM HTTP 响应头（对齐 Pi `on_response`）。
    pub fn set_on_response(&mut self, cb: ResponseCallback) {
        self.on_response = Some(cb);
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    pub async fn steer(&self, msg: Message) {
        let msg = self.expand_skill_in_message(msg);
        if let Some(text) = msg.content.first().and_then(|b| match b {
            uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }) {
            self.emit(AgentEvent::MessageQueued { text });
        }
        let mq = self.message_queue.lock().await;
        let _ = mq.steer(msg).await;
        let (s, f, n) = mq.queue_counts();
        self.emit(AgentEvent::QueueUpdate {
            data: Box::new(QueueUpdateData {
                steering_count: s,
                follow_up_count: f,
                next_turn_count: n,
            }),
        });
    }

    pub async fn follow_up(&self, msg: Message) {
        let msg = self.expand_skill_in_message(msg);
        if let Some(text) = msg.content.first().and_then(|b| match b {
            uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }) {
            self.emit(AgentEvent::MessageQueued { text });
        }
        let mq = self.message_queue.lock().await;
        let _ = mq.follow_up(msg).await;
        let (s, f, n) = mq.queue_counts();
        self.emit(AgentEvent::QueueUpdate {
            data: Box::new(QueueUpdateData {
                steering_count: s,
                follow_up_count: f,
                next_turn_count: n,
            }),
        });
    }

    pub async fn next_turn(&self, msg: Message) {
        let mq = self.message_queue.lock().await;
        let _ = mq.next_turn(msg).await;
        let (s, f, n) = mq.queue_counts();
        self.emit(AgentEvent::QueueUpdate {
            data: Box::new(QueueUpdateData {
                steering_count: s,
                follow_up_count: f,
                next_turn_count: n,
            }),
        });
    }

    /// Cancel and clear all queues, returning cleared messages
    pub async fn cancel_and_clear(&self) -> (Vec<Message>, Vec<Message>) {
        self.cancel_token.cancel();
        let mut mq = self.message_queue.lock().await;
        mq.clear_all()
    }

    pub(crate) fn emit(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Pi parallel batch: prepare → validate → before (serial per tool).
    async fn prepare_tool_call(
        &self,
        id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ToolResult> {
        if self.tool_registry.get(name).is_none() {
            return Err(ToolResult::err(format!("tool not found: {name}")));
        }
        if !self.tool_registry.is_active(name) {
            return Err(ToolResult::err(format!("tool not active: {name}")));
        }

        let prepared_args = match self.tool_registry.prepare_and_validate(name, args) {
            Ok(a) => a,
            Err(e) => return Err(ToolResult::err(e)),
        };

        if let Some(ref hooks) = self.tool_hooks {
            let ctx = BeforeToolCallContext {
                tool_call_id: id.to_string(),
                tool_name: name.to_string(),
                args: prepared_args.clone(),
            };
            if let Some(reason) = hooks.before_tool_call(&ctx).await {
                return Err(ToolResult::err(reason));
            }
        }

        Ok(prepared_args)
    }

    /// Run execute + after hook for already-prepared arguments.
    async fn execute_prepared_tool(
        &self,
        id: &str,
        name: &str,
        prepared_args: serde_json::Value,
        raw_args: serde_json::Value,
    ) -> ToolResult {
        execute_prepared_tool_shared(
            Arc::clone(&self.tool_registry),
            self.cancel_token.clone(),
            self.event_tx.clone(),
            self.tool_hooks.clone(),
            Arc::clone(&self.execution_env),
            id.to_string(),
            name.to_string(),
            prepared_args,
            raw_args,
        )
        .await
    }

    /// Execute a single tool with full lifecycle: hooks, prepare, execute, finalize
    async fn execute_single_tool(
        &self,
        _session_id: &str,
        id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> ToolResult {
        let prepared = match self.prepare_tool_call(id, name, args.clone()).await {
            Ok(p) => p,
            Err(tr) => return tr,
        };
        self.execute_prepared_tool(id, name, prepared, args).await
    }

    /// LLM stream with exponential-backoff retry for transient errors (429, network).
    ///
    /// **Pi:** 对照 `_isRetryableError()` + `_prepareRetry()`。
    async fn stream_with_retry(
        &self,
        model: &uncode_core::model::Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let max_retries = options.max_retries.unwrap_or(MAX_LLM_RETRIES);
        let base_delay = std::time::Duration::from_millis(
            options.max_retry_delay_ms.unwrap_or(BASE_RETRY_DELAY_MS),
        );

        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            match uncode_ai::stream_simple(model, context, options, &self.api_registry).await {
                Ok(stream) => return Ok(stream),
                Err(e) if e.is_context_overflow() => return Err(e),
                Err(e) if e.is_retryable() && attempt <= max_retries => {
                    let delay = base_delay * 2u32.saturating_pow(attempt - 1);
                    warn!(
                        "LLM error (attempt {attempt}/{max_retries}), retrying in {}ms: {e}",
                        delay.as_millis()
                    );
                    self.emit(AgentEvent::RetryAttempt {
                        data: Box::new(RetryAttemptData {
                            attempt,
                            max_attempts: max_retries,
                            delay_ms: delay.as_millis() as u64,
                            error: e.to_string(),
                            final_success: false,
                        }),
                    });
                    self.emit(AgentEvent::Error {
                        category: uncode_core::event::ErrorCategory::Llm,
                        message: format!("Retryable error, attempt {attempt}/{max_retries}: {e}"),
                        recoverable: true,
                    });
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        () = self.cancel_token.cancelled() => return Err(e),
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Whether an agent `run` is in progress (inner ReAct loop may span multiple Turns).
    pub fn is_run_active(&self) -> bool {
        self.active_run.load(Ordering::Acquire)
    }

    /// Wait until no run is active (for external synchronization)
    pub async fn wait_for_idle(&self) {
        while self.is_run_active() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Reset internal state: clear transcript, runtime state, and queues.
    pub async fn reset(&self) {
        self.cancel_token.cancel();
        let mut mq = self.message_queue.lock().await;
        mq.clear_all();
        self.active_run.store(false, Ordering::Release);
    }

    pub async fn run(&self, user_message: Message) -> Result<Vec<Message>, UncodeError> {
        // ActiveRun guard: reject concurrent runs
        if self
            .active_run
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(UncodeError::Harness(HarnessError::Busy {
                phase: "run".to_string(),
                code: 5001,
            }));
        }

        let result = self.run_inner(user_message).await;

        // Always clear active_run flag
        self.active_run.store(false, Ordering::Release);

        result
    }

    async fn run_inner(&self, user_message: Message) -> Result<Vec<Message>, UncodeError> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let session_id = match &self.session_id {
            Some(id) => id.clone(),
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                let cwd_str = cwd.to_string_lossy().to_string();
                if let Err(e) = self
                    .session_store
                    .init_session_with_title(&id, &self.model_id, &cwd_str, None)
                    .await
                {
                    debug!("session init skipped: {e}");
                }
                id
            }
        };

        // Persist user message
        if let Err(e) = self
            .session_store
            .append_entry(
                &session_id,
                &SessionEntry::Message(user_message.clone().into()),
            )
            .await
        {
            debug!("persist user message skipped: {e}");
        }
        self.emit(AgentEvent::MessageStart {
            role: Role::User,
            message_id: user_message.id.clone(),
        });
        self.emit(AgentEvent::MessageEnd {
            role: Role::User,
            message_id: user_message.id.clone(),
        });

        self.emit(AgentEvent::SessionStart {
            session_id: session_id.clone(),
            timestamp: chrono::Utc::now(),
        });

        // Build context from session store (picks up all previous messages for resume)
        let built = crate::context_builder::build_context(&self.session_store, &session_id)
            .await
            .map_err(|e| {
                UncodeError::Harness(uncode_core::error::HarnessError::Other {
                    message: e.to_string(),
                    code: 5099,
                })
            })?;
        let mut messages = built.messages;
        let mut effective_thinking_level = built.effective_thinking_level;

        // Inject workspace context bundle
        if let Some(ref cache) = self.graph_cache {
            let graph = cache.get_or_build(&cwd).await;
            if !graph.nodes.is_empty() {
                let bundle = crate::workspace_graph::select_bundle(
                    &graph,
                    &[],
                    first_text(&user_message),
                    cache.config(),
                );
                if !bundle.is_empty() {
                    messages.insert(0, bundle.to_system_message());
                }
            }
        }

        messages.insert(0, Message::system(self.system_prompt.clone()));

        let tools = self.tool_registry.definitions();

        // Unified pending messages: nextTurn / steering / followUp all flow through here
        let mut pending_messages: Vec<Message> = {
            let mut mq = self.message_queue.lock().await;
            let msgs = mq.drain_next_turn();
            for msg in &msgs {
                self.emit(AgentEvent::MessageDelivered {
                    text: first_text(msg).to_string(),
                });
            }
            msgs
        };

        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut turn: u64 = 0;

        // ═══════════════════════════════════════════════════════════════
        // Outer loop: follow-up injection (aligned with Pi's outer while(true))
        // ═══════════════════════════════════════════════════════════════
        'outer: loop {
            // Inner loop: turn iteration (aligned with Pi's inner while(hasMoreToolCalls || pendingMessages))
            let mut has_more_tool_calls = true;

            while has_more_tool_calls || !pending_messages.is_empty() {
                if self.cancel_token.is_cancelled() {
                    info!("agent interrupted at turn {turn}");
                    self.emit(AgentEvent::AgentInterrupted {
                        turn,
                        partial_response: false,
                    });
                    break 'outer;
                }

                if turn >= MAX_TURNS {
                    info!("max turns ({MAX_TURNS}) reached");
                    break 'outer;
                }

                // Inject pending messages (steering/nextTurn/followUp unified)
                if !pending_messages.is_empty() {
                    for msg in pending_messages.drain(..) {
                        if let Err(e) = self
                            .session_store
                            .append_entry(&session_id, &SessionEntry::Message(msg.clone().into()))
                            .await
                        {
                            debug!("persist pending message skipped: {e}");
                        }
                        messages.push(msg);
                    }
                }

                has_more_tool_calls = false;
                turn += 1;
                debug!("turn {turn}/{}", MAX_TURNS);

                let model = self.model_registry.get(&self.model_id).ok_or_else(|| {
                    UncodeError::Config(format!("model not found: {}", self.model_id))
                })?;

                // E8: Context usage warning
                if self.compaction_config.enabled {
                    if let Ok(entries) = self.session_store.load_entries(&session_id).await {
                        let estimated = crate::compaction::estimate_entry_tokens(&entries);
                        let ctx_window = model.context_window as u64;
                        if ctx_window > 0 {
                            let usage_ratio = estimated as f64 / ctx_window as f64;
                            let threshold = self.compaction_config.threshold_percent as f64 / 100.0;
                            if usage_ratio >= threshold * 0.6 {
                                self.emit(AgentEvent::ContextThreshold {
                                    data: Box::new(ContextThresholdData {
                                        session_id: session_id.clone(),
                                        usage_ratio,
                                        threshold,
                                        context_window: ctx_window,
                                    }),
                                });
                            }
                        }
                    }
                }

                // Session-aware compaction check
                if crate::compaction::should_compact_session(
                    &self.session_store,
                    &session_id,
                    model.context_window as u64,
                    &self.compaction_config,
                )
                .await
                {
                    self.emit(AgentEvent::CompactionStart {
                        data: Box::new(CompactionStartData {
                            session_id: session_id.clone(),
                            reason: CompactionReason::Threshold,
                            tokens_before: model.context_window as u64,
                        }),
                    });
                    match crate::compaction::compact_session(
                        &self.session_store,
                        &session_id,
                        &self.api_registry,
                        model,
                        &self.api_keys,
                        &self.compaction_config,
                    )
                    .await
                    {
                        Ok(Some(summary)) => {
                            let rebuilt = crate::context_builder::build_context(
                                &self.session_store,
                                &session_id,
                            )
                            .await
                            .map_err(|e| {
                                UncodeError::Harness(uncode_core::error::HarnessError::Other {
                                    message: e.to_string(),
                                    code: 5099,
                                })
                            })?;
                            let tokens_before = summary.tokens_before;
                            let summary_text = summary.summary.clone();
                            let entries_before = {
                                let all = self
                                    .session_store
                                    .load_entries(&session_id)
                                    .await
                                    .unwrap_or_default();
                                all.len()
                            };
                            messages = rebuilt.messages;
                            effective_thinking_level = rebuilt.effective_thinking_level;

                            // Re-inject workspace context bundle after compaction
                            if let Some(ref cache) = self.graph_cache {
                                let graph = cache.get_or_build(&cwd).await;
                                if !graph.nodes.is_empty() {
                                    let bundle = crate::workspace_graph::select_bundle(
                                        &graph,
                                        &[],
                                        "",
                                        cache.config(),
                                    );
                                    if !bundle.is_empty() {
                                        messages.insert(0, bundle.to_system_message());
                                    }
                                }
                            }

                            messages.insert(0, Message::system(self.system_prompt.clone()));
                            self.emit(AgentEvent::CompactionComplete {
                                messages_replaced: entries_before,
                                tokens_before,
                                tokens_after: 0,
                                summary_text,
                                reason: CompactionReason::Threshold,
                            });
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("compaction failed: {e}"),
                    }
                }

                let api_key = self.api_keys.get(&model.provider).cloned();

                // Determine thinking level: effective from session > model default
                let model_thinking = if model.reasoning {
                    Some(ThinkingLevel::High)
                } else {
                    None
                };
                let thinking_level = effective_thinking_level.or(model_thinking);

                // Persist ThinkingLevelChange if it differs from what was last recorded
                if thinking_level != effective_thinking_level {
                    let tl_entry =
                        SessionEntry::ThinkingLevelChange(Box::new(ThinkingLevelChangeEntry {
                            id: generate_entry_id(),
                            parent_id: None,
                            timestamp: chrono::Utc::now(),
                            thinking_level: thinking_level.unwrap_or(ThinkingLevel::High),
                        }));
                    if let Err(e) = self
                        .session_store
                        .append_entry(&session_id, &tl_entry)
                        .await
                    {
                        debug!("persist thinking level change skipped: {e}");
                    }
                    effective_thinking_level = thinking_level;
                }

                // transform_context callback: allow external context transformation
                if let Some(ref transform) = self.transform_context {
                    transform(&mut messages);
                }

                let context = Context {
                    system_prompt: Some(self.system_prompt.clone()),
                    messages: messages.clone(),
                    tools: tools.clone(),
                };
                let options = StreamOptions {
                    api_key,
                    temperature: Some(0.7),
                    max_tokens: Some(8192),
                    thinking_level,
                    session_id: self.session_id.clone(),
                    on_payload: self.on_payload.clone(),
                    on_response: self.on_response.clone(),
                    ..StreamOptions::default()
                };

                self.emit(AgentEvent::TurnStart { turn });

                self.emit(AgentEvent::LlmRequestStart {
                    data: Box::new(LlmRequestStartData {
                        model_id: self.model_id.clone(),
                        message_count: messages.len(),
                    }),
                });
                let llm_start = std::time::Instant::now();

                let mut stream = tokio::select! {
                    _ = self.cancel_token.cancelled() => {
                        info!("agent interrupted before stream at turn {turn}");
                        self.emit(AgentEvent::AgentInterrupted { turn, partial_response: false });
                        break 'outer;
                    }
                    result = self.stream_with_retry(model, &context, &options) => {
                        match result {
                            Ok(s) => s,
                            Err(e) if e.is_context_overflow() => {
                                warn!("context overflow at turn {turn}, triggering compaction");
                                self.emit(AgentEvent::Error {
                                    category: uncode_core::event::ErrorCategory::Llm,
                                    message: "Context overflow, triggering compaction".into(),
                                    recoverable: true,
                                });
                                self.emit(AgentEvent::CompactionStart {
                                    data: Box::new(CompactionStartData {
                                        session_id: session_id.clone(),
                                        reason: CompactionReason::Overflow,
                                        tokens_before: model.context_window as u64,
                                    }),
                                });
                                match crate::compaction::compact_session(
                                    &self.session_store,
                                    &session_id,
                                    &self.api_registry,
                                    model,
                                    self.api_keys.as_ref(),
                                    &self.compaction_config,
                                )
                                .await
                                {
                                    Ok(Some(overflow_summary)) => {
                                        self.emit(AgentEvent::CompactionComplete {
                                            messages_replaced: 0,
                                            tokens_before: model.context_window as u64,
                                            tokens_after: 0,
                                            summary_text: overflow_summary.summary.clone(),
                                            reason: CompactionReason::Overflow,
                                        });
                                        match crate::context_builder::build_context(
                                            &self.session_store,
                                            &session_id,
                                        )
                                        .await
                                        {
                                            Ok(rebuilt) => {
                                                messages = rebuilt.messages;
                                                let compacted_ctx = Context {
                                                    system_prompt: Some(self.system_prompt.clone()),
                                                    messages: messages.clone(),
                                                    tools: tools.clone(),
                                                };
                                                tokio::select! {
                                                    _ = self.cancel_token.cancelled() => {
                                                        break 'outer;
                                                    }
                                                    r = self.stream_with_retry(model, &compacted_ctx, &options) => r?,
                                                }
                                            }
                                            Err(e) => return Err(UncodeError::Harness(
                                                uncode_core::error::HarnessError::Other {
                                                    message: e.to_string(),
                                                    code: 5099,
                                                },
                                            )),
                                        }
                                    }
                                    _ => return Err(e),
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                };

                let mut current_text = String::with_capacity(2048);
                let mut current_thinking = String::with_capacity(1024);
                let mut pending_tool_calls: Vec<(String, String, String)> = Vec::with_capacity(4);
                let mut pending_executions: Vec<(String, String, serde_json::Value)> =
                    Vec::with_capacity(4);
                let mut args_pushed: HashSet<String> = HashSet::new();
                let mut turn_input_tokens: u64 = 0;
                let mut turn_output_tokens: u64 = 0;
                let mut tool_start_times: HashMap<String, std::time::Instant> = HashMap::new();
                let mut turn_phase_completed: Vec<String> = Vec::new();
                let mut turn_phase_issues: Vec<String> = Vec::new();
                let mut turn_assistant_snippet = String::new();

                // ── 决策层连线 (#339): 重置提案累积器 ──
                self.proposal_acc.lock().unwrap().reset();

                // ── Stream processing loop ──
                loop {
                    if self.cancel_token.is_cancelled() {
                        info!("agent interrupted during streaming at turn {turn}");
                        if !current_text.is_empty() || !current_thinking.is_empty() {
                            let mut content: Vec<ContentBlock> = Vec::with_capacity(2);
                            if !current_thinking.is_empty() {
                                content.push(ContentBlock::Thinking {
                                    text: std::mem::take(&mut current_thinking),
                                });
                            }
                            if !current_text.is_empty() {
                                content.push(ContentBlock::Text {
                                    text: std::mem::take(&mut current_text),
                                });
                            }
                            messages.push(Message::new(Role::Assistant, content));
                        }
                        self.emit(AgentEvent::AgentInterrupted {
                            turn,
                            partial_response: !current_text.is_empty(),
                        });
                        break 'outer;
                    }

                    let event = tokio::select! {
                        _ = self.cancel_token.cancelled() => {
                            if !current_text.is_empty() || !current_thinking.is_empty() {
                                let mut content: Vec<ContentBlock> = Vec::with_capacity(2);
                                if !current_thinking.is_empty() {
                                    content.push(ContentBlock::Thinking {
                                        text: std::mem::take(&mut current_thinking),
                                    });
                                }
                                if !current_text.is_empty() {
                                    content.push(ContentBlock::Text {
                                        text: std::mem::take(&mut current_text),
                                    });
                                }
                                messages.push(Message::new(Role::Assistant, content));
                            }
                            self.emit(AgentEvent::AgentInterrupted {
                                turn,
                                partial_response: !current_text.is_empty(),
                            });
                            break 'outer;
                        }
                        event = stream.next() => {
                            match event {
                                Some(e) => e,
                                None => break,
                            }
                        }
                    };

                    // ── 决策层连线 (#339): 喂入提案累积器 ──
                    self.proposal_acc.lock().unwrap().feed(&event);

                    match event {
                        StreamEvent::ThinkingDelta(text) => {
                            if !text.is_empty() {
                                current_thinking.push_str(&text);
                                self.emit(AgentEvent::ContentDelta {
                                    delta_type: uncode_core::event::DeltaType::Thinking,
                                    content: text,
                                    content_index: None,
                                });
                            }
                        }
                        StreamEvent::TextDelta(text) => {
                            if text.is_empty() {
                                continue;
                            }
                            current_text.push_str(&text);
                            self.emit(AgentEvent::ContentDelta {
                                delta_type: uncode_core::event::DeltaType::Text,
                                content: text,
                                content_index: None,
                            });
                        }
                        StreamEvent::ToolCallStart { id, name } => {
                            info!("tool call start: {name} ({id})");
                            self.emit(AgentEvent::ToolCallStart {
                                tool_id: id.clone(),
                                tool_name: name.clone(),
                                arguments_summary: String::new(),
                            });
                            tool_start_times.insert(id.clone(), std::time::Instant::now());
                            pending_tool_calls.push((id, name, String::new()));
                        }
                        StreamEvent::ToolCallDelta { id, arguments } => {
                            if let Some(tc) =
                                pending_tool_calls.iter_mut().find(|(tid, ..)| tid == &id)
                            {
                                tc.2.push_str(&arguments);

                                // Early path display: push accumulated args to TUI
                                if !args_pushed.contains(&id) && has_identifiable_field(&tc.2) {
                                    self.emit(AgentEvent::ToolCallProgress {
                                        tool_id: id.clone(),
                                        progress_type: uncode_core::event::ProgressType::Spinner,
                                        detail: tc.2.clone(),
                                    });
                                    args_pushed.insert(id.clone());
                                }
                            }
                        }
                        StreamEvent::ToolCallEnd(data) => {
                            let id = data.id;
                            let name = data.name;
                            let arguments = data.arguments;
                            if name == "bash" {
                                if let Some(d) = crate::tool_permission::approval_description(
                                    "bash", &arguments, None,
                                ) {
                                    info!("tool call end: {name} ({id}) — {d}");
                                } else {
                                    info!("tool call end: {name} ({id})");
                                }
                            } else {
                                info!("tool call end: {name} ({id})");
                            }
                            if !args_pushed.contains(&id) {
                                let args_detail = pending_tool_calls
                                    .iter()
                                    .find(|(tid, ..)| tid == &id)
                                    .map(|(_, _, a)| a.clone())
                                    .unwrap_or_else(|| arguments.to_string());
                                self.emit(AgentEvent::ToolCallProgress {
                                    tool_id: id.clone(),
                                    progress_type: uncode_core::event::ProgressType::Spinner,
                                    detail: args_detail,
                                });
                                args_pushed.insert(id.clone());
                            }

                            pending_executions.push((id, name, arguments));
                        }
                        StreamEvent::Usage(usage) => {
                            turn_input_tokens = usage.input_tokens;
                            turn_output_tokens = usage.output_tokens;
                            total_input_tokens += usage.input_tokens;
                            total_output_tokens += usage.output_tokens;
                        }
                        StreamEvent::Error { reason, message } => {
                            error!("stream error ({reason:?}): {message}");
                            self.emit(AgentEvent::Error {
                                category: uncode_core::event::ErrorCategory::Llm,
                                message: message.clone(),
                                recoverable: true,
                            });
                        }
                        StreamEvent::Done { reason } => {
                            tracing::debug!(
                                "Done event: reason={:?} thinking={} text={} tool_calls={} pending_executions={}",
                                reason,
                                !current_thinking.is_empty(),
                                !current_text.is_empty(),
                                pending_tool_calls.len(),
                                pending_executions.len()
                            );
                            let mut assistant_content: Vec<ContentBlock> =
                                Vec::with_capacity(pending_tool_calls.len() + 2);

                            if !current_thinking.is_empty() {
                                assistant_content.push(ContentBlock::Thinking {
                                    text: std::mem::take(&mut current_thinking),
                                });
                            }

                            if !current_text.is_empty() {
                                turn_assistant_snippet =
                                    assistant_snippet_for_phase(&current_text, 400);
                                assistant_content.push(ContentBlock::Text {
                                    text: std::mem::take(&mut current_text),
                                });
                            }

                            // Merge stream ToolCallEnd with accumulated deltas; only tool calls
                            // that will receive a tool result message go into the assistant turn
                            // (OpenAI/Anthropic reject assistant+tool_calls without matching tool msgs).
                            let streamed_tool_calls = std::mem::take(&mut pending_tool_calls);
                            let mut executions = std::mem::take(&mut pending_executions);
                            for (id, name, args_str) in streamed_tool_calls {
                                if executions.iter().any(|(eid, _, _)| eid == &id) {
                                    continue;
                                }
                                match serde_json::from_str::<serde_json::Value>(&args_str) {
                                    Ok(args) => executions.push((id, name, args)),
                                    Err(e) => {
                                        error!(
                                            tool = %name,
                                            tool_id = %id,
                                            error = %e,
                                            "tool args JSON parse failed, omitting tool call from assistant message"
                                        );
                                    }
                                }
                            }

                            for (id, name, arguments) in &executions {
                                assistant_content.push(ContentBlock::ToolCall(Box::new(
                                    uncode_core::message::ToolCall {
                                        id: id.clone(),
                                        name: name.clone(),
                                        arguments: arguments.clone(),
                                    },
                                )));
                            }

                            if !assistant_content.is_empty() {
                                let mut msg = Message::new(Role::Assistant, assistant_content);
                                msg.stop_reason = Some(reason);
                                msg.usage = Some(UsageInfo {
                                    input_tokens: turn_input_tokens,
                                    output_tokens: turn_output_tokens,
                                    cost: None,
                                });

                                self.emit(AgentEvent::MessageStart {
                                    role: Role::Assistant,
                                    message_id: msg.id.clone(),
                                });

                                if let Err(e) = self
                                    .session_store
                                    .append_entry(
                                        &session_id,
                                        &SessionEntry::Message(msg.clone().into()),
                                    )
                                    .await
                                {
                                    debug!("persist assistant message skipped: {e}");
                                }

                                self.emit(AgentEvent::MessageEnd {
                                    role: Role::Assistant,
                                    message_id: msg.id.clone(),
                                });

                                messages.push(msg);
                            }

                            let exec_args_by_id: HashMap<String, String> = executions
                                .iter()
                                .map(|(id, _, args)| (id.clone(), args.to_string()))
                                .collect();

                            // ── 决策层防火墙验证 (原则2: 自然语言止于防火墙, #339) ──
                            {
                                let acc = self.proposal_acc.lock().unwrap();
                                let proposals = acc.completed();
                                if !proposals.is_empty() {
                                    let mut fw = self.firewall.lock().unwrap();
                                    if fw.is_none() {
                                        *fw = Some(crate::decision::firewall::build_default_firewall(
                                            std::sync::Arc::new(crate::tool_permission::PermissionPolicy::default_policy()),
                                            Arc::clone(&self.tool_registry),
                                            std::env::current_dir().unwrap_or_default(),
                                        ));
                                    }
                                    if let Some(ref firewall) = *fw {
                                        for proposal in proposals {
                                            match firewall.process(proposal) {
                                                Ok(_normalized) => {
                                                    // 提案通过防火墙——继续执行
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        "firewall blocked proposal {}: {e}",
                                                        proposal.tool_name
                                                    );
                                                    self.emit(AgentEvent::DecisionMade {
                                                        turn_id: format!("turn-{turn}"),
                                                        tool_name: proposal.tool_name.clone(),
                                                        allowed: false,
                                                        reason: Some(e.to_string()),
                                                        duration_ms: None,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Batch-execute buffered tool calls (same `executions` as assistant ToolCalls)
                            if !executions.is_empty() {
                                // Pi strategy: if ANY tool is sequential, run ALL sequentially
                                let has_sequential = executions.iter().any(|(_, name, _)| {
                                    self.tool_registry.execution_mode(name)
                                        == ExecutionMode::Sequential
                                });

                                let all_outcomes: Vec<(String, String, ToolResult)> =
                                    if has_sequential {
                                        let mut outcomes = Vec::with_capacity(executions.len());
                                        for (id, name, args) in executions {
                                            let outcome = self
                                                .execute_single_tool(&session_id, &id, &name, args)
                                                .await;
                                            outcomes.push((id, name, outcome));
                                            if self.cancel_token.is_cancelled() {
                                                break;
                                            }
                                        }
                                        outcomes
                                    } else {
                                        // Pi: prepare → validate → before serial; execute parallel
                                        let batch_len = executions.len();
                                        let mut slot_results: Vec<Option<ToolResult>> =
                                            vec![None; batch_len];
                                        let mut ready = Vec::new();
                                        for (i, (id, name, args)) in executions.iter().enumerate() {
                                            match self
                                                .prepare_tool_call(id, name, args.clone())
                                                .await
                                            {
                                                Ok(prepared) => ready.push((
                                                    i,
                                                    id.clone(),
                                                    name.clone(),
                                                    args.clone(),
                                                    prepared,
                                                )),
                                                Err(tr) => slot_results[i] = Some(tr),
                                            }
                                            if self.cancel_token.is_cancelled() {
                                                break;
                                            }
                                        }

                                        let registry = Arc::clone(&self.tool_registry);
                                        let cancel = self.cancel_token.clone();
                                        let tx = self.event_tx.clone();
                                        let hooks = self.tool_hooks.clone();
                                        let exec_env = Arc::clone(&self.execution_env);

                                        let executed =
                                            futures::future::join_all(ready.into_iter().map(
                                                move |(i, id, name, raw_args, prepared)| {
                                                    let reg = registry.clone();
                                                    let ct = cancel.clone();
                                                    let etx = tx.clone();
                                                    let hk = hooks.clone();
                                                    let env = exec_env.clone();
                                                    async move {
                                                        let tr = execute_prepared_tool_shared(
                                                            reg, ct, etx, hk, env, id, name,
                                                            prepared, raw_args,
                                                        )
                                                        .await;
                                                        (i, tr)
                                                    }
                                                },
                                            ))
                                            .await;

                                        for (i, tr) in executed {
                                            slot_results[i] = Some(tr);
                                        }

                                        executions
                                            .into_iter()
                                            .enumerate()
                                            .map(|(i, (id, name, _args))| {
                                                let tr =
                                                    slot_results[i].take().unwrap_or_else(|| {
                                                        ToolResult::err("cancelled")
                                                    });
                                                (id, name, tr)
                                            })
                                            .collect()
                                    };

                                // Persist results, emit events, check terminate
                                let mut should_terminate = !all_outcomes.is_empty();
                                for (id, name, tool_result) in &all_outcomes {
                                    // Invalidate workspace graph cache after file edits
                                    if (name == "write" || name == "edit")
                                        && !tool_result.is_error
                                        && let Some(ref cache) = self.graph_cache
                                    {
                                        cache.invalidate();
                                    }
                                    let content_text = tool_result.text_content();
                                    let is_error = tool_result.is_error;
                                    let duration_ms = tool_start_times
                                        .remove(id)
                                        .map(|t| t.elapsed().as_millis() as u64)
                                        .unwrap_or(0);

                                    self.emit(AgentEvent::ToolCallEnd {
                                        data: Box::new(ToolCallEndEventData {
                                            tool_id: id.clone(),
                                            tool_name: name.clone(),
                                            arguments: String::new(),
                                            status: if is_error {
                                                ToolCallStatus::Failed
                                            } else {
                                                ToolCallStatus::Success
                                            },
                                            duration_ms,
                                            output_size: Some(content_text.len()),
                                            result_summary: Some(content_text.clone()),
                                            is_error,
                                        }),
                                    });

                                    // ── 决策层评估 + 反馈 (原则5: 事件流双向通道, #340) ──
                                    {
                                        use crate::decision::evaluator::Evaluator;
                                        use crate::decision::feedback::FeedbackBridge;
                                        let result = crate::decision::execution::ExecutionResult {
                                            tool_id: id.clone(),
                                            tool_name: name.clone(),
                                            success: !is_error,
                                            duration_ms,
                                            output: Some(content_text.clone()),
                                            error: if is_error {
                                                Some(content_text.clone())
                                            } else {
                                                None
                                            },
                                            terminate: tool_result.terminate,
                                        };
                                        let evaluator: &dyn Evaluator = if result
                                            .output
                                            .as_ref()
                                            .map_or(false, |o| o.contains("test result:"))
                                        {
                                            &crate::decision::evaluator::VerifiedEvaluator
                                        } else {
                                            &crate::decision::evaluator::BasicEvaluator
                                        };
                                        let ctx = crate::decision::evaluator::EvaluationContext {
                                            turn_number: turn as u32,
                                            tool_name: name.clone(),
                                            test_output: if content_text.contains("test result:") {
                                                Some(content_text.clone())
                                            } else {
                                                None
                                            },
                                            lint_output: None,
                                        };
                                        let score = evaluator.evaluate(&result, &ctx);
                                        let level_name = match score.level {
                                            crate::decision::evaluator::AssessmentLevel::RawOutput => "H0",
                                            crate::decision::evaluator::AssessmentLevel::Basic => "H1",
                                            crate::decision::evaluator::AssessmentLevel::Verified => "H2",
                                            crate::decision::evaluator::AssessmentLevel::Reproducible => "H3",
                                        };
                                        self.emit(AgentEvent::EvaluationScore {
                                            turn_id: format!("turn-{turn}"),
                                            level: level_name.to_string(),
                                            quality_score: score.quality_score,
                                            summary: Some(format!(
                                                "{}: {:.0}%",
                                                name,
                                                score.quality_score * 100.0
                                            )),
                                        });
                                        let _feedback = FeedbackBridge::infer_feedback(&result);
                                    }

                                    let args_short = summarize_tool_args(
                                        exec_args_by_id.get(id).map(|s| s.as_str()).unwrap_or(""),
                                    );
                                    let label = format_tool_phase_label(name, &args_short);
                                    if is_error {
                                        turn_phase_issues.push(label);
                                        // ── 演化引擎: 记录失败 ──
                                        self.evolution.lock().unwrap().record_failure(
                                            turn as u32,
                                            name.clone(),
                                            content_text.clone(),
                                        );
                                    } else {
                                        turn_phase_completed.push(label);
                                    }

                                    let result_block = ContentBlock::ToolResult(Box::new(
                                        uncode_core::message::ToolResult {
                                            tool_call_id: id.clone(),
                                            content: content_text,
                                            is_error,
                                        },
                                    ));
                                    let tool_msg = Message::new(Role::Tool, vec![result_block]);
                                    self.emit(AgentEvent::MessageStart {
                                        role: Role::Tool,
                                        message_id: tool_msg.id.clone(),
                                    });
                                    if let Err(e) = self
                                        .session_store
                                        .append_entry(
                                            &session_id,
                                            &SessionEntry::Message(tool_msg.clone().into()),
                                        )
                                        .await
                                    {
                                        debug!("persist tool result skipped: {e}");
                                    }
                                    self.emit(AgentEvent::MessageEnd {
                                        role: Role::Tool,
                                        message_id: tool_msg.id.clone(),
                                    });
                                    messages.push(tool_msg);

                                    // Pi: terminate only if ALL results request it
                                    if !tool_result.terminate {
                                        should_terminate = false;
                                    }
                                }

                                // Set has_more_tool_calls for inner loop control
                                if !should_terminate {
                                    has_more_tool_calls = true;
                                } else {
                                    info!("all tools requested terminate, ending agent loop");
                                }
                            }

                            break; // Done is the last stream event
                        }
                    }
                }

                // ── Post-turn processing ──

                // E4: LLM request end
                self.emit(AgentEvent::LlmRequestEnd {
                    data: Box::new(LlmRequestEndData {
                        model_id: self.model_id.clone(),
                        duration_ms: llm_start.elapsed().as_millis() as u64,
                        input_tokens: turn_input_tokens,
                        output_tokens: turn_output_tokens,
                        status: if self.cancel_token.is_cancelled() {
                            LlmRequestStatus::Cancelled
                        } else {
                            LlmRequestStatus::Success
                        },
                    }),
                });

                // TurnEnd
                if !self.cancel_token.is_cancelled() {
                    let turn_usage = UsageInfo {
                        input_tokens: turn_input_tokens,
                        output_tokens: turn_output_tokens,
                        cost: None,
                    };
                    // ── 演化引擎: 模式检测 ──
                    {
                        let evolution = self.evolution.lock().unwrap();
                        let mutations = evolution.analyze();
                        if !mutations.is_empty() {
                            tracing::info!(
                                "evolution engine detected {} mutation suggestion(s)",
                                mutations.len()
                            );
                            for m in &mutations {
                                tracing::debug!("  suggested: {m:?}");
                            }
                        }
                    }

                    self.emit(AgentEvent::TurnEnd {
                        turn,
                        usage: turn_usage.clone(),
                    });
                    if !turn_phase_completed.is_empty() || !turn_phase_issues.is_empty() {
                        let heuristic = build_phase_summary_heuristic(
                            turn,
                            turn_phase_completed.clone(),
                            turn_phase_issues.clone(),
                            has_more_tool_calls,
                            turn_usage.clone(),
                        );
                        let summary_data = if self.cancel_token.is_cancelled() {
                            heuristic
                        } else {
                            try_llm_phase_summary(PhaseSummaryLlmInput {
                                turn,
                                completed_labels: &turn_phase_completed,
                                issue_labels: &turn_phase_issues,
                                assistant_snippet: &turn_assistant_snippet,
                                has_more_tool_calls,
                                token_usage: turn_usage,
                                api_registry: &self.api_registry,
                                model,
                                api_keys: self.api_keys.as_ref(),
                                cancel_token: self.cancel_token.clone(),
                            })
                            .await
                            .unwrap_or(heuristic)
                        };
                        self.emit(AgentEvent::PhaseSummary {
                            data: Box::new(summary_data),
                        });
                    }
                }

                // prepare_next_turn callback — may return model/thinking changes
                if let Some(ref cb) = self.prepare_next_turn {
                    if let Some(decision) = cb() {
                        if let Some(new_model) = &decision.model_id {
                            if *new_model != self.model_id {
                                self.emit(AgentEvent::ModelChanged {
                                    data: Box::new(ModelChangedData {
                                        from: Some(self.model_id.clone()),
                                        to: new_model.clone(),
                                        source: ModelChangeSource::Auto,
                                    }),
                                });
                            }
                        }
                        if let Some(new_tl) = &decision.thinking_level {
                            if Some(new_tl) != effective_thinking_level.as_ref() {
                                self.emit(AgentEvent::ThinkingLevelChanged {
                                    data: Box::new(ThinkingLevelChangedData {
                                        from: effective_thinking_level,
                                        to: *new_tl,
                                    }),
                                });
                            }
                        }
                    }
                }

                // should_stop_after_turn callback
                if let Some(ref cb) = self.should_stop_after_turn
                    && cb(turn)
                {
                    break 'outer;
                }

                // Drain steering messages → pending_messages (feeds inner loop condition)
                let steering_msgs = {
                    let mut mq = self.message_queue.lock().await;
                    mq.drain_steering()
                };
                for msg in &steering_msgs {
                    self.emit(AgentEvent::MessageDelivered {
                        text: first_text(msg).to_string(),
                    });
                }
                pending_messages = steering_msgs;
            }
            // ── Inner turn loop exited ──

            // Outer loop: drain follow-up messages
            let follow_up_msgs = {
                let mut mq = self.message_queue.lock().await;
                mq.drain_follow_up()
            };
            if !follow_up_msgs.is_empty() {
                for msg in &follow_up_msgs {
                    self.emit(AgentEvent::MessageDelivered {
                        text: first_text(msg).to_string(),
                    });
                }
                pending_messages = follow_up_msgs;
                continue 'outer;
            }
            break 'outer;
        }

        let total_usage = UsageInfo {
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            cost: None,
        };

        self.emit(AgentEvent::SessionEnd {
            data: Box::new(SessionEndData {
                session_id: session_id.clone(),
                total_turns: turn,
                total_tokens: total_usage,
                exit_reason: (if self.cancel_token.is_cancelled() {
                    "interrupted"
                } else if turn >= MAX_TURNS {
                    "max_turns"
                } else {
                    "completed"
                })
                .into(),
            }),
        });

        self.emit(AgentEvent::AgentSettled {
            session_id: session_id.clone(),
        });

        Ok(messages)
    }
}

/// Check if a partial JSON string contains a recognizable identifier field
/// (path, file_path, or command) with a non-empty quoted value.
fn has_identifiable_field(partial: &str) -> bool {
    for key in ["\"path\"", "\"file_path\"", "\"command\""] {
        if let Some(pos) = partial.find(key) {
            let rest = &partial[pos + key.len()..];
            if let Some(colon) = rest.find(':') {
                let after_colon = rest.get(colon + 1..).unwrap_or("").trim_start();
                if after_colon.starts_with('"') {
                    let inner = after_colon.get(1..).unwrap_or("");
                    if let Some(end) = inner.find('"')
                        && end > 0
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod phase_summary_tests {
    use super::*;
    use crate::phase_summary::build_phase_summary_heuristic;

    #[test]
    fn summarize_tool_args_truncates_long_json() {
        let long = "a".repeat(60);
        let s = summarize_tool_args(&long);
        assert!(s.ends_with('…'));
        assert!(s.chars().count() <= 49);
    }

    #[test]
    fn build_phase_summary_includes_next_steps_when_more_tools() {
        let data = build_phase_summary_heuristic(
            2,
            vec!["read(src/main.rs)".into()],
            vec![],
            true,
            UsageInfo::default(),
        );
        assert_eq!(data.phase, 2);
        assert_eq!(data.completed.len(), 1);
        assert!(!data.next_steps.is_empty());
    }

    #[test]
    fn build_phase_summary_splits_failures_into_issues() {
        let data = build_phase_summary_heuristic(
            1,
            vec!["grep(foo)".into()],
            vec!["bash(cargo test)".into()],
            false,
            UsageInfo::default(),
        );
        assert_eq!(data.completed, vec!["grep(foo)"]);
        assert_eq!(data.issues, vec!["bash(cargo test)"]);
        assert!(data.next_steps.is_empty());
    }
}
