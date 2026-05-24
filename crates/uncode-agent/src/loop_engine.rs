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
const MAX_INJECTED_MESSAGES: usize = 10;

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
        allowed_paths: Vec::new(),
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

/// Unified cognition memory — working memory (turn scratchpad) + episode memory (importance-ranked).
struct CognitionMemory {
    working: crate::cognition::working_memory::WorkingMemory,
    episode: crate::cognition::episode::EpisodeMemory,
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
    session_id: std::sync::Mutex<Option<String>>,
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
    idle_notify: tokio::sync::Notify,
    graph_cache: Option<Arc<crate::workspace_graph::WorkspaceGraphCache>>,
    compaction_config: CompactionConfig,
    skill_registry: Option<uncode_core::skill::SkillRegistry>,
    /// 决策层提案累积器 — 认知显化与决策驱动设计 Phase 1 连线 (#339)
    proposal_acc: std::sync::Mutex<crate::decision::proposal::ProposalAccumulator>,
    /// 语义防火墙 — 认知显化与决策驱动设计 原则2 (#339 强制执行)
    firewall: std::sync::Mutex<Option<crate::decision::firewall::SemanticFirewall>>,
    /// 演化引擎 — 认知显化与决策驱动设计 自适应进化 (#342)
    evolution: std::sync::Mutex<uncode_shared::evolution::EvolutionEngine>,
    /// 认知记忆 — 工作记忆 + 情景记忆统一管理 (#385)
    cognition_memory: std::sync::Mutex<CognitionMemory>,
    /// 认知记忆管理器 — 压缩决策 + 摘要注入 (#386)
    memory_manager: crate::cognition::memory::MemoryManager,
    /// 裁决器 — 决策层合法性判定 (#385)
    adjudicator: std::sync::Mutex<Option<crate::decision::adjudication::Adjudicator>>,
    /// 扩展生命周期桥接 — Extension Runtime Phase 1 (#344)
    extension_bridge: Option<crate::hooks::ExtensionLifecycleBridge>,
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
        Self::with_event_sender(
            api_registry,
            model_registry,
            api_keys,
            tool_registry,
            session_store,
            system_prompt,
            model_id,
            event_tx,
        )
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
            session_id: std::sync::Mutex::new(None),
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
            idle_notify: tokio::sync::Notify::new(),
            graph_cache: None,
            compaction_config: CompactionConfig::default(),
            skill_registry: None,
            proposal_acc: std::sync::Mutex::new(
                crate::decision::proposal::ProposalAccumulator::new(),
            ),
            firewall: std::sync::Mutex::new(None),
            evolution: std::sync::Mutex::new(uncode_shared::evolution::EvolutionEngine::new(3)),
            cognition_memory: std::sync::Mutex::new(CognitionMemory {
                working: crate::cognition::working_memory::WorkingMemory::new(0),
                episode: crate::cognition::episode::EpisodeMemory::new(100),
            }),
            memory_manager: crate::cognition::memory::MemoryManager::new(
                crate::cognition::memory::MemoryConfig::default(),
            ),
            adjudicator: std::sync::Mutex::new(None),
            extension_bridge: None,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    pub fn event_sender(&self) -> broadcast::Sender<AgentEvent> {
        self.event_tx.clone()
    }

    pub fn set_session_id(&mut self, session_id: String) {
        *self.session_id.lock().unwrap() = Some(session_id);
    }

    pub fn session_id(&self) -> Option<String> {
        self.session_id.lock().unwrap().clone()
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

    pub fn set_extension_bridge(&mut self, bridge: crate::hooks::ExtensionLifecycleBridge) {
        self.extension_bridge = Some(bridge);
    }

    /// 注入裁决器 — 决策层合法性判定 (#385)
    pub fn set_adjudicator(&mut self, adj: crate::decision::adjudication::Adjudicator) {
        *self.adjudicator.lock().unwrap() = Some(adj);
    }

    /// Fire `SessionShutdown` lifecycle hook (called from harness abort).
    pub async fn fire_session_shutdown(&self, reason: &str) {
        if let Some(ref bridge) = self.extension_bridge {
            if let Some(sid) = self.session_id() {
                bridge.fire_session_shutdown(&sid, reason).await;
            }
        }
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
        let text = msg.content.first().and_then(|b| match b {
            uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        });
        let mq = self.message_queue.lock().await;
        mq.steer(msg).await;
        if let Some(text) = text {
            self.emit(AgentEvent::MessageQueued { text });
        }
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
        let text = msg.content.first().and_then(|b| match b {
            uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        });
        let mq = self.message_queue.lock().await;
        mq.follow_up(msg).await;
        if let Some(text) = text {
            self.emit(AgentEvent::MessageQueued { text });
        }
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
        mq.next_turn(msg).await;
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
        if let Err(e) = self.event_tx.send(event) {
            debug!("broadcast send failed (no receivers): {e}");
        }
    }

    /// 持久化决策审计记录到 SessionStore (#387)
    async fn persist_decision_audit(
        &self,
        session_id: &str,
        turn_id: String,
        tool_name: &str,
        allowed: bool,
        reason: Option<&str>,
        adjudication_duration_ms: u64,
    ) {
        let entry =
            SessionEntry::DecisionAudit(Box::new(uncode_core::session::DecisionAuditEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                turn_id,
                tool_name: tool_name.to_string(),
                allowed,
                reason: reason.map(|s| s.to_string()),
                adjudication_duration_ms,
            }));
        if let Err(e) = self.session_store.append_entry(session_id, &entry).await {
            debug!("decision audit persist skipped: {e}");
        }
    }

    /// 构建认知层上下文摘要 (#386)
    ///
    /// 使用 PromptManager.with_cognition_context() 将 WorkingMemory 和
    /// EpisodeMemory 的摘要合并为结构化文本。
    fn build_cognition_context(&self) -> Option<String> {
        let (wm_hint, ep_summary) = {
            let cm = self.cognition_memory.lock().unwrap();
            (
                cm.working.to_context_hint(),
                cm.episode.build_context_summary(),
            )
        };
        if wm_hint.is_none() && ep_summary.is_none() {
            return None;
        }
        let prompt = crate::cognition::prompt_manager::PromptManager::new()
            .with_cognition_context(wm_hint, ep_summary)
            .build();
        if prompt.is_empty() {
            None
        } else {
            Some(prompt)
        }
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

    /// Get a shared handle to the active-run flag (for extension idle-check callbacks).
    pub fn active_run_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.active_run)
    }

    /// Wait until no run is active (for external synchronization)
    pub async fn wait_for_idle(&self) {
        while self.is_run_active() {
            self.idle_notify.notified().await;
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

        // Always clear active_run flag and notify waiters
        self.active_run.store(false, Ordering::Release);
        self.idle_notify.notify_waiters();

        result
    }

    /// Build context from session store and inject workspace graph + system prompt + cognition.
    async fn rebuild_context_with_injections(
        &self,
        session_id: &str,
        cwd: &std::path::Path,
    ) -> Result<crate::context_builder::BuiltContext, UncodeError> {
        let mut built = crate::context_builder::build_context(&self.session_store, session_id)
            .await
            .map_err(|e| {
                UncodeError::Harness(uncode_core::error::HarnessError::Other {
                    message: e.to_string(),
                    code: 5099,
                })
            })?;
        if let Some(ref cache) = self.graph_cache {
            let graph = cache.get_or_build(cwd).await;
            if !graph.nodes.is_empty() {
                let bundle = crate::workspace_graph::select_bundle(&graph, &[], "", cache.config());
                if !bundle.is_empty() {
                    built.messages.insert(0, bundle.to_system_message());
                }
            }
        }
        built
            .messages
            .insert(0, Message::system(self.system_prompt.clone()));
        if let Some(ctx) = self.build_cognition_context() {
            built.messages.insert(1, Message::system(ctx));
        }
        Ok(built)
    }

    /// Check whether compaction should run (threshold + MemoryManager).
    async fn should_compact(&self, session_id: &str, context_window: u64) -> bool {
        if crate::compaction::should_compact_session(
            &self.session_store,
            session_id,
            context_window,
            &self.compaction_config,
        )
        .await
        {
            return true;
        }
        let Ok(entries) = self.session_store.load_entries(session_id).await else {
            return false;
        };
        let estimated = crate::compaction::estimate_entry_tokens(&entries);
        matches!(
            self.memory_manager.evaluate(estimated, context_window),
            crate::cognition::memory::CompactionDecision::ShouldCompact { .. }
                | crate::cognition::memory::CompactionDecision::ForceCompact { .. }
        )
    }

    /// Record turn feedback into WorkingMemory and EpisodeMemory.
    fn record_feedback(&self, turn: u64, feedback: &crate::decision::feedback::TurnFeedback) {
        let mut cm = self.cognition_memory.lock().unwrap();
        let wm_entries = feedback.to_working_memory_entries();
        for (content, important) in wm_entries {
            if important {
                cm.working.observe_important(content);
            } else {
                cm.working.observe(content);
            }
        }
        for obs in &feedback.observations {
            let event_type = if obs.starts_with('❌') {
                "tool_result_failure"
            } else {
                "tool_result_success"
            };
            cm.episode.record(event_type, obs, turn);
        }
    }

    /// Emit session-end lifecycle events (SessionEnd + AgentSettled).
    async fn emit_session_end(
        &self,
        session_id: &str,
        total_turns: u64,
        total_input_tokens: u64,
        total_output_tokens: u64,
    ) {
        let total_usage = UsageInfo {
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            cost: None,
        };
        self.emit(AgentEvent::SessionEnd {
            data: Box::new(SessionEndData {
                session_id: session_id.into(),
                total_turns,
                total_tokens: total_usage,
                exit_reason: (if self.cancel_token.is_cancelled() {
                    "interrupted"
                } else if total_turns >= MAX_TURNS {
                    "max_turns"
                } else {
                    "completed"
                })
                .into(),
            }),
        });
        if let Some(ref bridge) = self.extension_bridge {
            bridge.fire_session_end(session_id).await;
        }
        self.emit(AgentEvent::AgentSettled {
            session_id: session_id.into(),
        });
    }

    async fn run_inner(&self, mut user_message: Message) -> Result<Vec<Message>, UncodeError> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let existing_id = self.session_id.lock().unwrap().clone();
        let session_id = match existing_id {
            Some(id) => id,
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
                *self.session_id.lock().unwrap() = Some(id.clone());
                id
            }
        };

        // Fire Input hook — extensions can transform or handle user input
        let input_text: String = user_message
            .content
            .iter()
            .filter_map(|b| match b {
                uncode_ai::message::ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<&str>>()
            .join("\n");
        if let Some(ref bridge) = self.extension_bridge {
            let hook_result = bridge
                .fire_input(
                    &session_id,
                    uncode_extensions::hooks::InputSource::Interactive,
                    &input_text,
                    &[],
                )
                .await;
            match &hook_result {
                uncode_extensions::hooks::HookResult::Modify(modification) => {
                    if let Some(ref action) = modification.input_action {
                        match action {
                            uncode_extensions::hooks::InputAction::Handled => {
                                tracing::info!("input handled by extension, skipping normal flow");
                                return Ok(vec![]);
                            }
                            uncode_extensions::hooks::InputAction::Transform { text, .. } => {
                                if let Some(new_text) = text {
                                    for block in &mut user_message.content {
                                        if let ContentBlock::Text { text } = block {
                                            *text = new_text.clone();
                                            break;
                                        }
                                    }
                                }
                            }
                            uncode_extensions::hooks::InputAction::Continue => {}
                        }
                    }
                }
                uncode_extensions::hooks::HookResult::Block { reason } => {
                    tracing::info!("input blocked by extension: {reason}");
                    return Ok(vec![]);
                }
                uncode_extensions::hooks::HookResult::Continue => {}
            }
        }

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

        if let Some(ref bridge) = self.extension_bridge {
            bridge
                .fire_before_agent_start(&session_id, &input_text)
                .await;
        }

        self.emit(AgentEvent::SessionStart {
            session_id: session_id.clone(),
            timestamp: chrono::Utc::now(),
        });

        if let Some(ref bridge) = self.extension_bridge {
            bridge.fire_session_start(&session_id, "new").await;
            bridge
                .fire_resources_discover(&session_id, &cwd.to_string_lossy())
                .await;
        }

        // Build context from session store with workspace + cognition injections
        let built = self
            .rebuild_context_with_injections(&session_id, &cwd)
            .await?;
        let mut messages = built.messages;
        let mut effective_thinking_level = built.effective_thinking_level;

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

                // Session-aware compaction check (MemoryManager + should_compact_session)
                if self
                    .should_compact(&session_id, model.context_window as u64)
                    .await
                {
                    if let Some(ref bridge) = self.extension_bridge {
                        bridge.fire_session_before_compact(&session_id).await;
                    }
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
                        &model,
                        &self.api_keys,
                        &self.compaction_config,
                    )
                    .await
                    {
                        Ok(Some(summary)) => {
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
                            let rebuilt = self
                                .rebuild_context_with_injections(&session_id, &cwd)
                                .await?;
                            messages = rebuilt.messages;
                            effective_thinking_level = rebuilt.effective_thinking_level;

                            self.emit(AgentEvent::CompactionComplete {
                                messages_replaced: entries_before,
                                tokens_before,
                                tokens_after: 0,
                                summary_text,
                                reason: CompactionReason::Threshold,
                            });
                            if let Some(ref bridge) = self.extension_bridge {
                                bridge.fire_session_compact(&session_id).await;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            warn!("compaction failed: {e} — continuing with uncompressed context");
                        }
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

                self.emit(AgentEvent::TurnStart { turn });

                if let Some(ref bridge) = self.extension_bridge {
                    bridge.fire_turn_start(&session_id, turn).await;
                }

                if let Some(ref bridge) = self.extension_bridge {
                    bridge.fire_agent_start(&session_id).await;
                    let ctx_result = bridge.fire_context(&session_id, &messages).await;
                    match ctx_result {
                        uncode_extensions::hooks::HookResult::Modify(modification) => {
                            if let Some(additional) = modification.additional_messages {
                                let count = additional.len().min(MAX_INJECTED_MESSAGES);
                                for msg in additional.into_iter().take(MAX_INJECTED_MESSAGES) {
                                    messages.push(msg);
                                }
                                self.emit(AgentEvent::ContextInjected {
                                    extension_name: "extension".into(),
                                    count,
                                });
                            }
                        }
                        uncode_extensions::hooks::HookResult::Block { reason } => {
                            tracing::warn!("extension blocked context hook: {reason}");
                        }
                        uncode_extensions::hooks::HookResult::Continue => {}
                    }
                }

                let context = Context {
                    system_prompt: Some(self.system_prompt.clone()),
                    messages: messages.clone(),
                    tools: tools.clone(),
                };

                // Build on_payload callback that bridges to extension hooks.
                let ext_registry = self.extension_bridge.as_ref().map(|b| b.registry().clone());
                let session_id_for_payload = Some(session_id.clone());
                let existing_on_payload = self.on_payload.clone();
                let on_payload_cb: Option<PayloadCallback> =
                    if ext_registry.is_some() && session_id_for_payload.is_some() {
                        Some(Arc::new(move |body: &mut serde_json::Value| {
                            if let Some(ref cb) = existing_on_payload {
                                cb(body);
                            }
                            if let Some(ref registry) = ext_registry {
                                if let Some(ref sid) = session_id_for_payload {
                                    let ctx = uncode_extensions::hooks::HookContext {
                                        session_id: Some(sid.clone()),
                                        event: uncode_extensions::hooks::HookEvent::ProviderRequest(
                                            body.clone(),
                                        ),
                                    };
                                    let reg = registry.clone();
                                    let hook =
                                    uncode_extensions::hooks::LifecycleHook::BeforeProviderRequest;
                                    // Fire the extension hook asynchronously without blocking the stream.
                                    tokio::spawn(async move {
                                        let _ = reg.fire(hook, &ctx).await;
                                    });
                                }
                            }
                        }))
                    } else {
                        existing_on_payload
                    };

                let options = StreamOptions {
                    api_key,
                    temperature: Some(0.7),
                    max_tokens: Some(8192),
                    thinking_level,
                    session_id: Some(session_id.clone()),
                    on_payload: on_payload_cb,
                    on_response: self.on_response.clone(),
                    ..StreamOptions::default()
                };

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
                    result = self.stream_with_retry(&model, &context, &options) => {
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
                                    &model,
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
                                        match self
                                            .rebuild_context_with_injections(
                                                &session_id,
                                                &cwd,
                                            )
                                            .await
                                        {
                                            Ok(rebuilt) => {
                                                messages = rebuilt.messages;
                                                let compacted_ctx = Context {
                                                    system_prompt: Some(
                                                        self.system_prompt.clone(),
                                                    ),
                                                    messages: messages.clone(),
                                                    tools: tools.clone(),
                                                };
                                                tokio::select! {
                                                    _ = self.cancel_token.cancelled() => {
                                                        break 'outer;
                                                    }
                                                    r = self.stream_with_retry(
                                                        &model,
                                                        &compacted_ctx,
                                                        &options,
                                                    ) => r?,
                                                }
                                            }
                                            Err(e) => return Err(e),
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

                // ── 认知层连线 (#385): 初始化 turn 反馈累积器 ──
                let mut turn_feedback = crate::decision::feedback::TurnFeedback::new(turn as u32);
                // 重置工作记忆的 turn 编号，flush 返回的低重要性条目注入情景记忆
                {
                    let mut cm = self.cognition_memory.lock().unwrap();
                    let flushed = cm.working.flush(turn);
                    for entry in &flushed {
                        cm.episode
                            .record("working_memory_flush", entry.one_liner(), turn);
                    }
                }

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
                            if let Some(ref bridge) = self.extension_bridge {
                                bridge.fire_message_update(&session_id).await;
                            }
                        }
                        StreamEvent::ToolCallStart { id, name } => {
                            info!("tool call start: {name} ({id})");
                            self.emit(AgentEvent::ToolCallStart {
                                tool_id: id.clone(),
                                tool_name: name.clone(),
                                arguments_summary: String::new(),
                            });
                            if let Some(ref bridge) = self.extension_bridge {
                                bridge
                                    .fire_tool_execution_start(&session_id, &id, &name)
                                    .await;
                            }
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
                            if let Some(ref bridge) = self.extension_bridge {
                                bridge.fire_tool_execution_update(&session_id).await;
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
                            turn_input_tokens += usage.input_tokens;
                            turn_output_tokens += usage.output_tokens;
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
                            // Warn on orphaned tool calls (stream ended without ToolCallEnd)
                            if !pending_tool_calls.is_empty() {
                                warn!(
                                    "stream ended with {} pending tool calls (missing ToolCallEnd)",
                                    pending_tool_calls.len()
                                );
                                // Do NOT clear — let the merge below attempt to parse delta args.
                            }
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

                            // ── 决策层防火墙验证 + 裁决器 + 审计 (#339, #385, #387) ──
                            let mut denied_results: Vec<(String, String, ToolResult)> = Vec::new();
                            struct PendingAudit {
                                turn_id: String,
                                tool_name: String,
                                allowed: bool,
                                reason: Option<String>,
                                duration_ms: u64,
                            }
                            let mut pending_audits: Vec<PendingAudit> = Vec::new();
                            let denied_tool_names: HashSet<String> = {
                                let acc = self.proposal_acc.lock().unwrap();
                                let proposals = acc.completed();
                                if proposals.is_empty() {
                                    HashSet::new()
                                } else {
                                    let mut fw = self.firewall.lock().unwrap();
                                    if fw.is_none() {
                                        *fw = Some(crate::decision::firewall::build_default_firewall(
                                            std::sync::Arc::new(crate::tool_permission::PermissionPolicy::default_policy()),
                                            Arc::clone(&self.tool_registry),
                                            std::env::current_dir().unwrap_or_default(),
                                        ));
                                    }
                                    let mut denied = HashSet::new();
                                    if let Some(ref firewall) = *fw {
                                        let adj = self.adjudicator.lock().unwrap();
                                        let decision_ctx =
                                            crate::decision::types::DecisionContext {
                                                turn_number: turn as u32,
                                                max_turns: crate::loop_engine::MAX_TURNS as u32,
                                                active_tools: self
                                                    .tool_registry
                                                    .active_tool_names()
                                                    .unwrap_or_default(),
                                            };
                                        for proposal in proposals {
                                            let started_at = std::time::Instant::now();
                                            let tool_name = proposal.tool_name.clone();
                                            match firewall.process(proposal) {
                                                Ok(normalized) => {
                                                    let (allowed, reason) =
                                                        if let Some(ref adjudicator) = *adj {
                                                            match adjudicator.adjudicate(
                                                                &normalized,
                                                                &decision_ctx,
                                                            ) {
                                                                Ok(_approved) => {
                                                                    debug!(
                                                                        "adjudicator approved: {}",
                                                                        normalized.tool_name
                                                                    );
                                                                    (true, None)
                                                                }
                                                                Err(e) => {
                                                                    warn!(
                                                                        "adjudicator denied: {e}"
                                                                    );
                                                                    (false, Some(e.to_string()))
                                                                }
                                                            }
                                                        } else {
                                                            (true, None)
                                                        };

                                                    let duration_ms =
                                                        started_at.elapsed().as_millis() as u64;
                                                    self.emit(AgentEvent::DecisionMade {
                                                        turn_id: format!("turn-{turn}"),
                                                        tool_name: normalized.tool_name.clone(),
                                                        allowed,
                                                        reason: reason.clone(),
                                                        duration_ms: Some(duration_ms),
                                                    });
                                                    pending_audits.push(PendingAudit {
                                                        turn_id: format!("turn-{turn}"),
                                                        tool_name: normalized.tool_name.clone(),
                                                        allowed,
                                                        reason,
                                                        duration_ms,
                                                    });
                                                    if !allowed {
                                                        denied.insert(tool_name);
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::debug!(
                                                        "firewall flagged proposal {}: {e}",
                                                        tool_name
                                                    );
                                                    let duration_ms =
                                                        started_at.elapsed().as_millis() as u64;
                                                    self.emit(AgentEvent::DecisionMade {
                                                        turn_id: format!("turn-{turn}"),
                                                        tool_name: tool_name.clone(),
                                                        allowed: false,
                                                        reason: Some(e.to_string()),
                                                        duration_ms: Some(duration_ms),
                                                    });
                                                    pending_audits.push(PendingAudit {
                                                        turn_id: format!("turn-{turn}"),
                                                        tool_name: tool_name.clone(),
                                                        allowed: false,
                                                        reason: Some(e.to_string()),
                                                        duration_ms,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    denied
                                }
                            };

                            // 持久化审计记录（锁已释放，可安全 await）
                            for audit in pending_audits {
                                self.persist_decision_audit(
                                    &session_id,
                                    audit.turn_id,
                                    &audit.tool_name,
                                    audit.allowed,
                                    audit.reason.as_deref(),
                                    audit.duration_ms,
                                )
                                .await;
                            }

                            // 从执行列表中移除被拒绝的工具，并生成错误 tool result
                            if !denied_tool_names.is_empty() {
                                let denied_executions: Vec<(String, String, serde_json::Value)> =
                                    executions
                                        .drain(..)
                                        .filter(|(id, name, _args)| {
                                            if denied_tool_names.contains(name) {
                                                // 为被拒绝的工具生成错误结果
                                                denied_results.push((
                                                    id.clone(),
                                                    name.clone(),
                                                    ToolResult::err(format!(
                                                        "denied by decision policy: {}",
                                                        name
                                                    )),
                                                ));
                                                false
                                            } else {
                                                true
                                            }
                                        })
                                        .collect();
                                executions = denied_executions;
                            }

                            // Batch-execute buffered tool calls (same `executions` as assistant ToolCalls)
                            let has_denied = !denied_results.is_empty();
                            if !executions.is_empty() || has_denied {
                                // Pi strategy: if ANY tool is sequential, run ALL sequentially
                                let has_sequential = executions.iter().any(|(_, name, _)| {
                                    self.tool_registry.execution_mode(name)
                                        == ExecutionMode::Sequential
                                });

                                let mut all_outcomes: Vec<(String, String, ToolResult)> =
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

                                // 将被决策层拒绝的工具结果合并
                                if !denied_results.is_empty() {
                                    all_outcomes.extend(denied_results);
                                }

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
                                    if let Some(ref bridge) = self.extension_bridge {
                                        bridge
                                            .fire_tool_execution_end(
                                                &session_id,
                                                id,
                                                name,
                                                is_error,
                                            )
                                            .await;
                                    }

                                    // ── 决策层评估 + 反馈闭环 (原则5: 事件流双向通道, #385) ──
                                    {
                                        let result = crate::decision::types::ExecutionResult {
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

                                        let active_tools: Vec<String> = self
                                            .tool_registry
                                            .active_tool_names()
                                            .unwrap_or_default();
                                        let test_output = if content_text.contains("test result:") {
                                            Some(content_text.as_str())
                                        } else {
                                            None
                                        };
                                        turn_feedback.record(
                                            &result,
                                            &active_tools,
                                            turn_input_tokens as usize,
                                            test_output,
                                        );

                                        // 发出评估事件（兼容现有 UI）
                                        if let Some(ref eval) = turn_feedback.evaluation {
                                            if let Some(last_score) = eval.scores.last() {
                                                let level_name = match last_score.level {
                                                    crate::decision::evaluator::AssessmentLevel::RawOutput => "H0",
                                                    crate::decision::evaluator::AssessmentLevel::Basic => "H1",
                                                    crate::decision::evaluator::AssessmentLevel::Verified => "H2",
                                                    crate::decision::evaluator::AssessmentLevel::Reproducible => "H3",
                                                };
                                                self.emit(AgentEvent::EvaluationScore {
                                                    turn_id: format!("turn-{turn}"),
                                                    level: level_name.to_string(),
                                                    quality_score: last_score.quality_score,
                                                    summary: Some(format!(
                                                        "{}: {:.0}%",
                                                        name,
                                                        last_score.quality_score * 100.0
                                                    )),
                                                });
                                            }
                                        }
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
                                        // ── 认知层: 不确定性分类 (#386) ──
                                        let uc = crate::cognition::uncertainty::UncertaintyClass::from_error_category("tool", &content_text);
                                        let uc_summary = match &uc {
                                            crate::cognition::uncertainty::UncertaintyClass::Generative(_) => "generative_uncertainty",
                                            crate::cognition::uncertainty::UncertaintyClass::Cognitive(_) => "cognitive_gap",
                                            crate::cognition::uncertainty::UncertaintyClass::Executional(e) => {
                                                self.cognition_memory.lock().unwrap().working.observe_important(
                                                    format!("[uncertainty] executional: {} — strategy: {:?}", e.error, e.strategy),
                                                );
                                                "executional_uncertainty"
                                            }
                                        };
                                        debug!("tool failure classified as {uc_summary}: {name}");
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

                if let Some(ref bridge) = self.extension_bridge {
                    bridge.fire_after_provider_response(&session_id, 200).await;
                }

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
                    if let Some(ref bridge) = self.extension_bridge {
                        bridge.fire_turn_end(&session_id, turn).await;
                        bridge.fire_agent_end(&session_id).await;
                    }

                    // ── 认知层反馈闭环 (#385): TurnFeedback → WorkingMemory → EpisodeMemory ──
                    self.record_feedback(turn, &turn_feedback);

                    // agent_steps 可观测性（面向离线训练管道）
                    if !turn_feedback.agent_steps.is_empty() {
                        debug!(
                            "turn {turn}: collected {} agent steps",
                            turn_feedback.agent_steps.len()
                        );
                    }

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
                                model: &model,
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
                                if let Some(ref bridge) = self.extension_bridge {
                                    let previous_model = self.model_id.as_str();
                                    bridge
                                        .fire_model_select(
                                            &session_id,
                                            new_model,
                                            Some(previous_model),
                                        )
                                        .await;
                                }
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
                                if let Some(ref bridge) = self.extension_bridge {
                                    bridge
                                        .fire_thinking_level_select(
                                            &session_id,
                                            &format!("{new_tl:?}"),
                                            effective_thinking_level
                                                .as_ref()
                                                .map(|tl| format!("{tl:?}"))
                                                .as_deref(),
                                        )
                                        .await;
                                }
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

        self.emit_session_end(&session_id, turn, total_input_tokens, total_output_tokens)
            .await;

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
