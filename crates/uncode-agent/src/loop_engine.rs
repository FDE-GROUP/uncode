use futures::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use uncode_core::error::UncodeError;
use uncode_core::event::AgentEvent;
use uncode_core::message::{ContentBlock, Message, Role, UsageInfo};
use uncode_core::session::SessionEntry;
use uncode_llm::driver::{CompletionRequest, LlmDriver, StreamEvent};
use uncode_session::store::SessionStore;
use uncode_tools::registry::ToolRegistry;

const MAX_TURNS: u64 = 50;
const DEFAULT_MAX_TOKENS: u64 = 128_000;

pub struct AgentLoop {
    driver: Arc<dyn LlmDriver>,
    tool_registry: Arc<ToolRegistry>,
    session_store: Arc<SessionStore>,
    system_prompt: String,
    model: String,
    model_max_tokens: u64,
    session_id: Option<String>,
    event_tx: broadcast::Sender<AgentEvent>,
    cancel_token: CancellationToken,
}

impl AgentLoop {
    pub fn new(
        driver: Arc<dyn LlmDriver>,
        tool_registry: Arc<ToolRegistry>,
        session_store: Arc<SessionStore>,
        system_prompt: String,
        model: String,
    ) -> Self {
        Self::with_max_tokens(
            driver,
            tool_registry,
            session_store,
            system_prompt,
            model,
            DEFAULT_MAX_TOKENS,
        )
    }

    pub fn with_max_tokens(
        driver: Arc<dyn LlmDriver>,
        tool_registry: Arc<ToolRegistry>,
        session_store: Arc<SessionStore>,
        system_prompt: String,
        model: String,
        model_max_tokens: u64,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            driver,
            tool_registry,
            session_store,
            system_prompt,
            model,
            model_max_tokens,
            session_id: None,
            event_tx,
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn with_event_sender(
        driver: Arc<dyn LlmDriver>,
        tool_registry: Arc<ToolRegistry>,
        session_store: Arc<SessionStore>,
        system_prompt: String,
        model: String,
        event_tx: broadcast::Sender<AgentEvent>,
    ) -> Self {
        Self {
            driver,
            tool_registry,
            session_store,
            system_prompt,
            model,
            model_max_tokens: DEFAULT_MAX_TOKENS,
            session_id: None,
            event_tx,
            cancel_token: CancellationToken::new(),
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

    pub fn set_cancel_token(&mut self, token: CancellationToken) {
        self.cancel_token = token;
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    fn emit(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }

    pub async fn run(&self, user_message: Message) -> Result<Vec<Message>, UncodeError> {
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
                        .init_session_with_title(&id, &self.model, &cwd, None)
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

        self.emit(AgentEvent::SessionStart {
            session_id: session_id.clone(),
            timestamp: chrono::Utc::now(),
        });

        let mut messages: Vec<Message> = vec![Message::system(self.system_prompt.clone())];
        messages.push(user_message);

        let tools = self.tool_registry.definitions();
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut turn = 0;

        loop {
            if self.cancel_token.is_cancelled() {
                info!("agent interrupted at turn {turn}");
                self.emit(AgentEvent::AgentInterrupted {
                    turn,
                    partial_response: false,
                });
                break;
            }

            if turn >= MAX_TURNS {
                info!("max turns ({MAX_TURNS}) reached");
                break;
            }

            turn += 1;
            debug!("turn {turn}/{}", MAX_TURNS);

            // Compaction check before building request
            if crate::compaction::should_compact(&messages, self.model_max_tokens) {
                if let Err(e) = crate::compaction::compact_messages(
                    &mut messages,
                    &self.driver,
                    &self.model,
                    self.model_max_tokens,
                )
                .await
                {
                    tracing::warn!("compaction failed: {e}");
                }
            }

            let request = CompletionRequest {
                model: self.model.clone(),
                messages: messages.clone(),
                system: Some(self.system_prompt.clone()),
                max_tokens: Some(8192),
                temperature: Some(0.7),
                tools: tools.clone(),
            };

            let mut stream = tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!("agent interrupted before stream at turn {turn}");
                    self.emit(AgentEvent::AgentInterrupted { turn, partial_response: false });
                    break;
                }
                result = self.driver.complete(request) => result?,
            };
            let mut current_text = String::with_capacity(2048);
            let mut current_thinking = String::with_capacity(1024);
            let mut pending_tool_calls: Vec<(String, String, String)> = Vec::with_capacity(4);
            let mut tool_results: Vec<ContentBlock> = Vec::with_capacity(4);
            let mut args_pushed: HashSet<String> = HashSet::new();
            let mut turn_input_tokens: u64 = 0;
            let mut turn_output_tokens: u64 = 0;

            loop {
                // Check cancellation before waiting for next stream event
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
                    break;
                }

                let event = tokio::select! {
                    _ = self.cancel_token.cancelled() => {
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
                        break;
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
                        if let Some(tc) = pending_tool_calls.iter_mut().find(|(tid, ..)| tid == &id)
                        {
                            tc.2.push_str(&arguments);

                            // Early path display: push accumulated args to TUI
                            // as soon as we can extract a meaningful path/command.
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
                        // Fallback: push arguments if not already sent during deltas
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

                        let tool = self.tool_registry.get(&name);
                        let (result, is_error) = match tool {
                            Some(executor) => {
                                let start = std::time::Instant::now();
                                let exec_result = tokio::select! {
                                    _ = self.cancel_token.cancelled() => {
                                        info!("agent interrupted during tool execution: {name}");
                                        // Persist already-collected tool results before breaking
                                        for tr in tool_results.drain(..) {
                                            let tool_msg = Message::new(Role::Tool, vec![tr]);
                                            messages.push(tool_msg);
                                        }
                                        self.emit(AgentEvent::AgentInterrupted {
                                            turn,
                                            partial_response: !current_text.is_empty(),
                                        });
                                        break;
                                    }
                                    r = executor.execute(arguments.clone()) => r,
                                };
                                match exec_result {
                                    Ok(output) => {
                                        let duration = start.elapsed();
                                        self.emit(AgentEvent::ToolCallProgress {
                                            tool_id: id.clone(),
                                            progress_type: uncode_core::event::ProgressType::Stdout,
                                            detail: output.clone(),
                                        });
                                        self.emit(AgentEvent::ToolCallEnd {
                                            tool_id: id.clone(),
                                            tool_name: name.clone(),
                                            arguments: arguments.to_string(),
                                            status: uncode_core::event::ToolCallStatus::Success,
                                            duration_ms: duration.as_millis() as u64,
                                            output_size: Some(output.len()),
                                        });
                                        (output, false)
                                    }
                                    Err(e) => {
                                        let duration = start.elapsed();
                                        error!("tool {name} failed: {e}");
                                        self.emit(AgentEvent::ToolCallEnd {
                                            tool_id: id.clone(),
                                            tool_name: name.clone(),
                                            arguments: arguments.to_string(),
                                            status: uncode_core::event::ToolCallStatus::Failed,
                                            duration_ms: duration.as_millis() as u64,
                                            output_size: None,
                                        });
                                        (format!("error: {e}"), true)
                                    }
                                }
                            }
                            None => {
                                let msg = format!("tool '{name}' not found");
                                self.emit(AgentEvent::ToolCallEnd {
                                    tool_id: id.clone(),
                                    tool_name: name.clone(),
                                    arguments: arguments.to_string(),
                                    status: uncode_core::event::ToolCallStatus::Failed,
                                    duration_ms: 0,
                                    output_size: None,
                                });
                                (msg, true)
                            }
                        };

                        tool_results.push(ContentBlock::ToolResult(
                            uncode_core::message::ToolResult {
                                tool_call_id: id,
                                content: result,
                                is_error,
                            },
                        ));
                    }
                    StreamEvent::Usage(usage) => {
                        turn_input_tokens = usage.input_tokens;
                        turn_output_tokens = usage.output_tokens;
                        total_input_tokens += usage.input_tokens;
                        total_output_tokens += usage.output_tokens;
                    }
                    StreamEvent::Error(e) => {
                        error!("stream error: {e}");
                        self.emit(AgentEvent::Error {
                            category: uncode_core::event::ErrorCategory::Llm,
                            message: e.clone(),
                            recoverable: true,
                        });
                    }
                    StreamEvent::Done => {
                        tracing::debug!(
                            "Done event: thinking={} text={} tool_calls={} tool_results={}",
                            !current_thinking.is_empty(),
                            !current_text.is_empty(),
                            pending_tool_calls.len(),
                            tool_results.len()
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
                            msg.usage = Some(UsageInfo {
                                input_tokens: turn_input_tokens,
                                output_tokens: turn_output_tokens,
                                cost: None,
                            });

                            // Persist assistant message
                            if let Err(e) = self.session_store.append_entry(
                                &session_id,
                                &SessionEntry::Message(msg.clone().into()),
                            ) {
                                debug!("persist assistant message skipped: {e}");
                            }

                            messages.push(msg);
                        }

                        for result in tool_results.drain(..) {
                            // Persist tool result
                            let tool_msg = Message::new(Role::Tool, vec![result]);
                            if let Err(e) = self.session_store.append_entry(
                                &session_id,
                                &SessionEntry::Message(tool_msg.clone().into()),
                            ) {
                                debug!("persist tool result skipped: {e}");
                            }
                            messages.push(tool_msg);
                        }
                    }
                }
            }

            let was_interrupted = self.cancel_token.is_cancelled();
            if !was_interrupted {
                self.emit(AgentEvent::TurnEnd {
                    turn,
                    usage: UsageInfo {
                        input_tokens: turn_input_tokens,
                        output_tokens: turn_output_tokens,
                        cost: None,
                    },
                });
            }

            if pending_tool_calls.is_empty() || was_interrupted {
                break;
            }
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
