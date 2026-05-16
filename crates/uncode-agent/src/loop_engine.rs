use futures::StreamExt;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::session::store::SessionStore;
use crate::steering::MessageQueue;
use crate::tools::registry::ToolRegistry;
use uncode_ai::StreamEvent;
use uncode_ai::{ApiRegistry, ModelRegistry};
use uncode_core::api_types::{Context, StreamOptions, ThinkingLevel};
use uncode_core::error::HarnessError;
use uncode_core::error::UncodeError;
use uncode_core::event::AgentEvent;
use uncode_core::event::ToolCallStatus;
use uncode_core::message::{ContentBlock, Message, Role, UsageInfo};
use uncode_core::session::{SessionEntry, ThinkingLevelChangeEntry, generate_entry_id};
use uncode_core::tool::{
    AfterToolCallContext, BeforeToolCallContext, ExecutionMode, ToolContext, ToolHooks,
    ToolProgress, ToolResult,
};

const MAX_TURNS: u64 = 50;

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
    message_queue: tokio::sync::Mutex<MessageQueue>,
    should_stop_after_turn: Option<Arc<dyn Fn(u64) -> bool + Send + Sync>>,
    prepare_next_turn: Option<Arc<dyn Fn() + Send + Sync>>,
    transform_context: Option<Arc<dyn Fn(&mut Vec<Message>) + Send + Sync>>,
    active_run: Arc<AtomicBool>,
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
            message_queue: tokio::sync::Mutex::new(MessageQueue::new()),
            should_stop_after_turn: None,
            prepare_next_turn: None,
            transform_context: None,
            active_run: Arc::new(AtomicBool::new(false)),
        }
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
            message_queue: tokio::sync::Mutex::new(MessageQueue::new()),
            should_stop_after_turn: None,
            prepare_next_turn: None,
            transform_context: None,
            active_run: Arc::new(AtomicBool::new(false)),
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

    pub fn set_tool_hooks(&mut self, hooks: Arc<dyn ToolHooks>) {
        self.tool_hooks = Some(hooks);
    }

    pub fn set_should_stop_after_turn(&mut self, cb: Arc<dyn Fn(u64) -> bool + Send + Sync>) {
        self.should_stop_after_turn = Some(cb);
    }

    pub fn set_prepare_next_turn(&mut self, cb: Arc<dyn Fn() + Send + Sync>) {
        self.prepare_next_turn = Some(cb);
    }

    pub fn set_transform_context(&mut self, cb: Arc<dyn Fn(&mut Vec<Message>) + Send + Sync>) {
        self.transform_context = Some(cb);
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    pub async fn steer(&self, msg: Message) {
        if let Some(text) = msg.content.first().and_then(|b| match b {
            uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }) {
            self.emit(AgentEvent::MessageQueued { text });
        }
        let mq = self.message_queue.lock().await;
        let _ = mq.steer(msg).await;
    }

    pub async fn follow_up(&self, msg: Message) {
        if let Some(text) = msg.content.first().and_then(|b| match b {
            uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }) {
            self.emit(AgentEvent::MessageQueued { text });
        }
        let mq = self.message_queue.lock().await;
        let _ = mq.follow_up(msg).await;
    }

    pub async fn next_turn(&self, msg: Message) {
        let mq = self.message_queue.lock().await;
        let _ = mq.next_turn(msg).await;
    }

    /// Cancel and clear all queues, returning cleared messages
    pub async fn cancel_and_clear(&self) -> (Vec<Message>, Vec<Message>) {
        self.cancel_token.cancel();
        let mut mq = self.message_queue.lock().await;
        mq.clear_all()
    }

    fn emit(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Execute a single tool with full lifecycle: hooks, prepare, execute, finalize
    async fn execute_single_tool(
        &self,
        _session_id: &str,
        id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> ToolResult {
        // before hook
        if let Some(ref hooks) = self.tool_hooks {
            let ctx = BeforeToolCallContext {
                tool_call_id: id.to_string(),
                tool_name: name.to_string(),
                args: args.clone(),
            };
            if let Some(reason) = hooks.before_tool_call(&ctx).await {
                return ToolResult::err(reason);
            }
        }

        // prepare arguments
        let executor = self.tool_registry.get(name);
        let prepared_args = if let Some(ref exec) = executor {
            match exec.prepare_arguments(args.clone()) {
                Ok(a) => a,
                Err(e) => return ToolResult::err(format!("argument error: {e}")),
            }
        } else {
            args.clone()
        };

        let child = self.cancel_token.child_token();
        let ctx = ToolContext {
            cancel_token: child.clone(),
            on_progress: Some(Box::new({
                let etx = self.event_tx.clone();
                let tid = id.to_string();
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
            tool_call_id: id.to_string(),
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

        // after hook
        if let Some(ref hooks) = self.tool_hooks {
            let after_ctx = AfterToolCallContext {
                tool_call_id: id.to_string(),
                tool_name: name.to_string(),
                args,
            };
            let patch = hooks.after_tool_call(&after_ctx, &mut tool_result).await;
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

    /// Wait until no run is active (for external synchronization)
    pub async fn wait_for_idle(&self) {
        while self.active_run.load(Ordering::Acquire) {
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
        let session_id = match &self.session_id {
            Some(id) => id.clone(),
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                let cwd = std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if let Err(e) =
                    self.session_store
                        .init_session_with_title(&id, &self.model_id, &cwd, None)
                {
                    debug!("session init skipped: {e}");
                }
                id
            }
        };

        // Persist user message
        if let Err(e) = self.session_store.append_entry(
            &session_id,
            &SessionEntry::Message(user_message.clone().into()),
        ) {
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
            .map_err(|e| {
                UncodeError::Harness(uncode_core::error::HarnessError::Other {
                    message: e.to_string(),
                    code: 5099,
                })
            })?;
        let mut messages = built.messages;
        let mut effective_thinking_level = built.effective_thinking_level;
        messages.insert(0, Message::system(self.system_prompt.clone()));

        let tools = self.tool_registry.definitions();

        // Unified pending messages: nextTurn / steering / followUp all flow through here
        let mut pending_messages: Vec<Message> = {
            let mut mq = self.message_queue.lock().await;
            let msgs = mq.drain_next_turn();
            for msg in &msgs {
                self.emit(AgentEvent::MessageDelivered {
                    text: msg
                        .content
                        .first()
                        .and_then(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .unwrap_or("")
                        .to_string(),
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

                // Session-aware compaction check
                if crate::compaction::should_compact_session(
                    &self.session_store,
                    &session_id,
                    model.context_window as u64,
                ) {
                    match crate::compaction::compact_session(
                        &self.session_store,
                        &session_id,
                        &self.api_registry,
                        model,
                        &self.api_keys,
                    )
                    .await
                    {
                        Ok(Some(summary)) => {
                            let rebuilt = crate::context_builder::build_context(
                                &self.session_store,
                                &session_id,
                            )
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
                                    .unwrap_or_default();
                                all.len()
                            };
                            messages = rebuilt.messages;
                            effective_thinking_level = rebuilt.effective_thinking_level;
                            messages.insert(0, Message::system(self.system_prompt.clone()));
                            self.emit(AgentEvent::CompactionComplete {
                                messages_replaced: entries_before,
                                tokens_before,
                                tokens_after: 0,
                                summary_text,
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
                    let tl_entry = SessionEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
                        id: generate_entry_id(),
                        parent_id: None,
                        timestamp: chrono::Utc::now(),
                        thinking_level: thinking_level.unwrap_or(ThinkingLevel::High),
                    });
                    if let Err(e) = self.session_store.append_entry(&session_id, &tl_entry) {
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
                    ..StreamOptions::default()
                };

                self.emit(AgentEvent::TurnStart { turn });

                let mut stream = tokio::select! {
                    _ = self.cancel_token.cancelled() => {
                        info!("agent interrupted before stream at turn {turn}");
                        self.emit(AgentEvent::AgentInterrupted { turn, partial_response: false });
                        break 'outer;
                    }
                    result = uncode_ai::stream(model, &context, &options, &self.api_registry) => result?,
                };

                let mut current_text = String::with_capacity(2048);
                let mut current_thinking = String::with_capacity(1024);
                let mut pending_tool_calls: Vec<(String, String, String)> = Vec::with_capacity(4);
                let mut pending_executions: Vec<(String, String, serde_json::Value)> =
                    Vec::with_capacity(4);
                let mut args_pushed: HashSet<String> = HashSet::new();
                let mut turn_input_tokens: u64 = 0;
                let mut turn_output_tokens: u64 = 0;

                // ── Stream processing loop ──
                loop {
                    if self.cancel_token.is_cancelled() {
                        info!("agent interrupted during streaming at turn {turn}");
                        if !current_text.is_empty() || !current_thinking.is_empty() {
                            let mut content: Vec<ContentBlock> = Vec::new();
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
                                let mut content: Vec<ContentBlock> = Vec::new();
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
                        StreamEvent::ToolCallEnd {
                            id,
                            name,
                            arguments,
                        } => {
                            info!("tool call end: {name} ({id})");
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
                            let mut assistant_content: Vec<ContentBlock> = Vec::new();

                            if !current_thinking.is_empty() {
                                assistant_content.push(ContentBlock::Thinking {
                                    text: std::mem::take(&mut current_thinking),
                                });
                            }

                            if !current_text.is_empty() {
                                assistant_content.push(ContentBlock::Text {
                                    text: std::mem::take(&mut current_text),
                                });
                            }

                            for (id, name, arguments) in &pending_tool_calls {
                                let args: serde_json::Value =
                                    serde_json::from_str(arguments).unwrap_or_default();
                                assistant_content.push(ContentBlock::ToolCall(
                                    uncode_core::message::ToolCall {
                                        id: id.clone(),
                                        name: name.clone(),
                                        arguments: args,
                                    },
                                ));
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

                                if let Err(e) = self.session_store.append_entry(
                                    &session_id,
                                    &SessionEntry::Message(msg.clone().into()),
                                ) {
                                    debug!("persist assistant message skipped: {e}");
                                }

                                self.emit(AgentEvent::MessageEnd {
                                    role: Role::Assistant,
                                    message_id: msg.id.clone(),
                                });

                                messages.push(msg);
                            }

                            // Batch-execute buffered tool calls
                            let executions = std::mem::take(&mut pending_executions);
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
                                        let registry = Arc::clone(&self.tool_registry);
                                        let cancel = self.cancel_token.clone();
                                        let tx = self.event_tx.clone();
                                        let hooks = self.tool_hooks.clone();

                                        futures::future::join_all(executions.into_iter().map(
                                            move |(id, name, args)| {
                                                let reg = registry.clone();
                                                let ct = cancel.clone();
                                                let etx = tx.clone();
                                                let hk = hooks.clone();
                                                async move {
                                                    if let Some(ref h) = hk {
                                                        let before_ctx = BeforeToolCallContext {
                                                            tool_call_id: id.clone(),
                                                            tool_name: name.clone(),
                                                            args: args.clone(),
                                                        };
                                                        if let Some(reason) =
                                                            h.before_tool_call(&before_ctx).await
                                                        {
                                                            return (
                                                                id,
                                                                name,
                                                                ToolResult::err(reason),
                                                            );
                                                        }
                                                    }

                                                    let prepared_args = if let Some(exec) =
                                                        reg.get(&name)
                                                    {
                                                        match exec.prepare_arguments(args.clone()) {
                                                            Ok(a) => a,
                                                            Err(e) => {
                                                                return (
                                                                    id,
                                                                    name,
                                                                    ToolResult::err(format!(
                                                                        "argument error: {e}"
                                                                    )),
                                                                )
                                                            }
                                                        }
                                                    } else {
                                                        args.clone()
                                                    };

                                                    let executor = reg.get(&name);
                                                    let child = ct.child_token();
                                                    let tid = id.clone();
                                                    let ctx = ToolContext {
                                                        cancel_token: child.clone(),
                                                        on_progress: Some(Box::new(
                                                            move |p: ToolProgress| {
                                                                let detail = match &p {
                                                                    ToolProgress::Spinner(s) => {
                                                                        s.clone()
                                                                    }
                                                                    ToolProgress::Percentage {
                                                                        detail,
                                                                        ..
                                                                    } => detail.clone(),
                                                                    ToolProgress::LogLine(l) => {
                                                                        l.clone()
                                                                    }
                                                                };
                                                                let _ = etx.send(
                                                                    AgentEvent::ToolCallProgress {
                                                                        tool_id: tid.clone(),
                                                                        progress_type:
                                                                            uncode_core::event::ProgressType::Spinner,
                                                                        detail,
                                                                    },
                                                                );
                                                            },
                                                        )),
                                                        tool_call_id: id.clone(),
                                                    };

                                                    let mut tool_result =
                                                        if let Some(exec) = executor {
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
                                                            ToolResult::err(format!(
                                                                "tool not found: {name}"
                                                            ))
                                                        };

                                                    if let Some(ref h) = hk {
                                                        let after_ctx = AfterToolCallContext {
                                                            tool_call_id: id.clone(),
                                                            tool_name: name.clone(),
                                                            args,
                                                        };
                                                        let patch =
                                                            h.after_tool_call(&after_ctx, &mut tool_result)
                                                                .await;
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

                                                    (id, name, tool_result)
                                                }
                                            },
                                        ))
                                        .await
                                    };

                                // Persist results, emit events, check terminate
                                let mut should_terminate = !all_outcomes.is_empty();
                                for (id, name, tool_result) in &all_outcomes {
                                    let content_text = tool_result.text_content();
                                    let is_error = tool_result.is_error;

                                    self.emit(AgentEvent::ToolCallEnd {
                                        tool_id: id.clone(),
                                        tool_name: name.clone(),
                                        arguments: String::new(),
                                        status: if is_error {
                                            ToolCallStatus::Failed
                                        } else {
                                            ToolCallStatus::Success
                                        },
                                        duration_ms: 0,
                                        output_size: Some(content_text.len()),
                                        result_summary: Some(
                                            content_text.chars().take(200).collect(),
                                        ),
                                        is_error,
                                    });

                                    let result_block = ContentBlock::ToolResult(
                                        uncode_core::message::ToolResult {
                                            tool_call_id: id.clone(),
                                            content: content_text,
                                            is_error,
                                        },
                                    );
                                    let tool_msg = Message::new(Role::Tool, vec![result_block]);
                                    self.emit(AgentEvent::MessageStart {
                                        role: Role::Tool,
                                        message_id: tool_msg.id.clone(),
                                    });
                                    if let Err(e) = self.session_store.append_entry(
                                        &session_id,
                                        &SessionEntry::Message(tool_msg.clone().into()),
                                    ) {
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

                // TurnEnd
                if !self.cancel_token.is_cancelled() {
                    self.emit(AgentEvent::TurnEnd {
                        turn,
                        usage: UsageInfo {
                            input_tokens: turn_input_tokens,
                            output_tokens: turn_output_tokens,
                            cost: None,
                        },
                    });
                }

                // prepare_next_turn callback
                if let Some(ref cb) = self.prepare_next_turn {
                    cb();
                }

                // should_stop_after_turn callback
                if let Some(ref cb) = self.should_stop_after_turn {
                    if cb(turn) {
                        break 'outer;
                    }
                }

                // Drain steering messages → pending_messages (feeds inner loop condition)
                let steering_msgs = {
                    let mut mq = self.message_queue.lock().await;
                    mq.drain_steering()
                };
                for msg in &steering_msgs {
                    self.emit(AgentEvent::MessageDelivered {
                        text: msg
                            .content
                            .first()
                            .and_then(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .unwrap_or("")
                            .to_string(),
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
                        text: msg
                            .content
                            .first()
                            .and_then(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .unwrap_or("")
                            .to_string(),
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
                    if let Some(end) = inner.find('"') {
                        if end > 0 {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
