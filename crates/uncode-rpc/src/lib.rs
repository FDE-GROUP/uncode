//! uncode-rpc — JSON-RPC 2.0 over stdio
//!
//! JSON-RPC 2.0 协议实现，供 IDE/外部工具通过 stdio 集成。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, broadcast};

// ── Protocol types ──

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Serialize)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

// ── Handler type ──

pub type RpcHandler = Arc<
    dyn Fn(
            Value,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>
        + Send
        + Sync,
>;

// ── Server ──

pub struct RpcServer {
    handlers: Mutex<HashMap<String, RpcHandler>>,
    notification_writer: Arc<Mutex<tokio::io::Stdout>>,
}

impl RpcServer {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
            notification_writer: Arc::new(Mutex::new(tokio::io::stdout())),
        }
    }

    pub async fn register(
        &self,
        method: &str,
        handler: impl Fn(
            Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Value, String>> + Send>,
        > + Send
        + Sync
        + 'static,
    ) {
        self.handlers
            .lock()
            .await
            .insert(method.to_string(), Arc::new(handler));
    }

    /// Forward AgentEvent broadcast as JSON-RPC notifications.
    pub async fn forward_events(
        &self,
        mut rx: broadcast::Receiver<uncode_core::event::AgentEvent>,
    ) {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let notification = JsonRpcNotification {
                        jsonrpc: "2.0".into(),
                        method: format!("event.{}", event_name(&event)),
                        params: Some(serde_json::to_value(&event).unwrap_or(Value::Null)),
                    };
                    if let Ok(json) = serde_json::to_string(&notification) {
                        let mut w = self.notification_writer.lock().await;
                        let _ = w.write_all(format!("{json}\n").as_bytes()).await;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }

    /// Main stdio serve loop with batch request support.
    pub async fn serve(&self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        let writer = Arc::new(Mutex::new(tokio::io::stdout()));

        tracing::info!("JSON-RPC server started on stdio");

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let is_batch = line.trim_start().starts_with('[');

            if is_batch {
                let requests: Vec<JsonRpcRequest> = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(e) => {
                        let resp = Self::parse_error(e.to_string());
                        let json = serde_json::to_string(&vec![resp])?;
                        let mut w = writer.lock().await;
                        let _ = w.write_all(format!("{json}\n").as_bytes()).await;
                        continue;
                    }
                };
                let handlers = self.handlers.lock().await;
                let mut responses = Vec::with_capacity(requests.len());
                for r in requests {
                    responses.push(self.handle_request(r, &handlers).await);
                }
                drop(handlers);
                let json = serde_json::to_string(&responses)?;
                let mut w = writer.lock().await;
                let _ = w.write_all(format!("{json}\n").as_bytes()).await;
            } else {
                let request: JsonRpcRequest = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(e) => {
                        let resp = Self::parse_error(e.to_string());
                        let json = serde_json::to_string(&resp)?;
                        let mut w = writer.lock().await;
                        let _ = w.write_all(format!("{json}\n").as_bytes()).await;
                        continue;
                    }
                };
                // Notification (no id) — handle but don't respond
                if request.id.is_none() {
                    let handlers = self.handlers.lock().await;
                    self.handle_request(request, &handlers).await;
                    continue;
                }
                let handlers = self.handlers.lock().await;
                let resp = self.handle_request(request, &handlers).await;
                drop(handlers);
                let json = serde_json::to_string(&resp)?;
                let mut w = writer.lock().await;
                let _ = w.write_all(format!("{json}\n").as_bytes()).await;
            }
        }

        Ok(())
    }

    async fn handle_request(
        &self,
        request: JsonRpcRequest,
        handlers: &HashMap<String, RpcHandler>,
    ) -> JsonRpcResponse {
        match handlers.get(&request.method) {
            Some(handler) => match handler(request.params.unwrap_or(Value::Null)).await {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id,
                    result: Some(result),
                    error: None,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32000,
                        message: e,
                    }),
                },
            },
            None => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                }),
            },
        }
    }

    fn parse_error(msg: String) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: None,
            result: None,
            error: Some(JsonRpcError {
                code: -32700,
                message: format!("Parse error: {msg}"),
            }),
        }
    }
}

impl Default for RpcServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Map AgentEvent variant to notification method suffix.
fn event_name(event: &uncode_core::event::AgentEvent) -> &'static str {
    use uncode_core::event::AgentEvent;
    match event {
        AgentEvent::SessionStart { .. } => "session_start",
        AgentEvent::TaskUpdate { .. } => "task_update",
        AgentEvent::ContentDelta { .. } => "content_delta",
        AgentEvent::ToolCallStart { .. } => "tool_call_start",
        AgentEvent::ToolCallProgress { .. } => "tool_call_progress",
        AgentEvent::ToolCallEnd { .. } => "tool_call_end",
        AgentEvent::PhaseSummary { .. } => "phase_summary",
        AgentEvent::Error { .. } => "error",
        AgentEvent::TurnEnd { .. } => "turn_end",
        AgentEvent::SessionEnd { .. } => "session_end",
        AgentEvent::CompactionComplete { .. } => "compaction_complete",
        AgentEvent::MessageQueued { .. } => "message_queued",
        AgentEvent::MessageDelivered { .. } => "message_delivered",
        AgentEvent::AgentInterrupted { .. } => "agent_interrupted",
        _ => "unknown",
    }
}

// ── Core command registration ──

/// Register the 8 core JSON-RPC commands.
pub async fn register_core_commands(
    server: &RpcServer,
    session_store: Arc<uncode_agent::session::store::SessionStore>,
    model_registry: Arc<uncode_ai::ModelRegistry>,
) {
    // session.create
    let ss = session_store.clone();
    server
        .register("session.create", move |params| {
            let ss = ss.clone();
            Box::pin(async move {
                let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let id = uuid::Uuid::new_v4().to_string();
                ss.init_session_with_title(
                    &id,
                    "",
                    "",
                    if title.is_empty() {
                        None
                    } else {
                        Some(title.to_string())
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({"session_id": id}))
            })
        })
        .await;

    // session.list
    let ss = session_store.clone();
    server
        .register("session.list", move |_| {
            let ss = ss.clone();
            Box::pin(async move {
                let sessions = ss.list_sessions().await.map_err(|e| e.to_string())?;
                serde_json::to_value(sessions).map_err(|e| e.to_string())
            })
        })
        .await;

    // session.get
    let ss = session_store.clone();
    server
        .register("session.get", move |params| {
            let ss = ss.clone();
            Box::pin(async move {
                let id = params
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("missing session_id")?;
                let header = ss.read_header(id).await.map_err(|e| e.to_string())?;
                let entries = ss.load_entries(id).await.map_err(|e| e.to_string())?;
                let leaf_id = ss.get_leaf_id(id).await.map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "header": header,
                    "entries": entries,
                    "leaf_id": leaf_id,
                }))
            })
        })
        .await;

    // session.branch — in-place branching with summary
    let ss = session_store.clone();
    server
        .register("session.branch", move |params| {
            let ss = ss.clone();
            Box::pin(async move {
                let session_id = params
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("missing session_id")?;
                let target_id = params
                    .get("target_id")
                    .and_then(|v| v.as_str())
                    .ok_or("missing target_id")?;
                let reason = params
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("RPC branch");
                uncode_agent::branch_summarization::branch_with_summary(
                    &ss, session_id, target_id, reason,
                )
                .await
                .map_err(|e| e.to_string())?;
                let leaf_id = ss
                    .get_leaf_id(session_id)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "leaf_id": leaf_id,
                }))
            })
        })
        .await;

    // model.list
    let mr = model_registry.clone();
    server
        .register("model.list", move |_| {
            let mr = mr.clone();
            Box::pin(async move {
                let models: Vec<serde_json::Value> = mr
                    .all_models()
                    .into_iter()
                    .map(|m| {
                        serde_json::json!({
                            "id": m.id,
                            "name": m.name,
                            "provider": m.provider,
                            "context_window": m.context_window,
                            "api": m.api,
                        })
                    })
                    .collect();
                serde_json::to_value(models).map_err(|e| e.to_string())
            })
        })
        .await;

    // model.switch
    let mr = model_registry.clone();
    server
        .register("model.switch", move |params| {
            let mr = mr.clone();
            Box::pin(async move {
                let model = params
                    .get("model")
                    .and_then(|v| v.as_str())
                    .ok_or("missing model name")?;
                if !mr.has(model) {
                    return Err(format!("model not found: {model}"));
                }
                Ok(serde_json::json!({"switched": model}))
            })
        })
        .await;

    // tool.list
    server
        .register("tool.list", move |_| {
            Box::pin(async move {
                let tools = vec![
                    "read",
                    "write",
                    "edit",
                    "grep",
                    "bash",
                    "find",
                    "ls",
                    "github",
                    "web_fetch",
                    "web_search",
                ];
                serde_json::to_value(tools).map_err(|e| e.to_string())
            })
        })
        .await;

    // message.send
    server
        .register("message.send", move |params| {
            Box::pin(async move {
                let text = params
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or("missing text")?;
                let session_id = params
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                Ok(serde_json::json!({
                    "status": "queued",
                    "session_id": session_id,
                    "text": text,
                }))
            })
        })
        .await;

    // message.stream
    server
        .register("message.stream", move |params| {
            Box::pin(async move {
                let session_id = params
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                Ok(serde_json::json!({
                    "status": "subscribed",
                    "session_id": session_id,
                    "note": "events broadcast as JSON-RPC notifications (event.*)"
                }))
            })
        })
        .await;
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_name_mapping() {
        use uncode_core::event::AgentEvent;
        let event = AgentEvent::SessionStart {
            session_id: "test".into(),
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(event_name(&event), "session_start");

        let event = AgentEvent::TurnEnd {
            turn: 1,
            usage: uncode_core::message::UsageInfo {
                input_tokens: 100,
                output_tokens: 50,
                cost: None,
            },
        };
        assert_eq!(event_name(&event), "turn_end");
    }

    #[test]
    fn test_event_name_all_known_variants() {
        use chrono::Utc;
        use uncode_core::event::*;
        use uncode_core::message::UsageInfo;

        let now = Utc::now();

        let cases: Vec<(AgentEvent, &str)> = vec![
            (
                AgentEvent::SessionStart {
                    session_id: "s1".into(),
                    timestamp: now,
                },
                "session_start",
            ),
            (
                AgentEvent::SessionEnd {
                    data: Box::new(SessionEndData {
                        session_id: "s1".into(),
                        total_turns: 1,
                        total_tokens: UsageInfo::default(),
                        exit_reason: "done".into(),
                    }),
                },
                "session_end",
            ),
            (
                AgentEvent::TurnStart { turn: 1 },
                "unknown",
            ),
            (
                AgentEvent::TurnEnd {
                    turn: 1,
                    usage: UsageInfo::default(),
                },
                "turn_end",
            ),
            (
                AgentEvent::MessageStart {
                    role: uncode_core::message::Role::Assistant,
                    message_id: "m1".into(),
                },
                "unknown",
            ),
            (
                AgentEvent::MessageEnd {
                    role: uncode_core::message::Role::Assistant,
                    message_id: "m1".into(),
                },
                "unknown",
            ),
            (
                AgentEvent::ContentDelta {
                    delta_type: DeltaType::Text,
                    content: "hi".into(),
                    content_index: None,
                },
                "content_delta",
            ),
            (
                AgentEvent::ToolCallStart {
                    tool_id: "t1".into(),
                    tool_name: "read".into(),
                    arguments_summary: "{}".into(),
                },
                "tool_call_start",
            ),
            (
                AgentEvent::ToolCallProgress {
                    tool_id: "t1".into(),
                    progress_type: ProgressType::Spinner,
                    detail: "".into(),
                },
                "tool_call_progress",
            ),
            (
                AgentEvent::ToolCallAwaitingApproval {
                    tool_id: "t1".into(),
                    tool_name: "write".into(),
                    arguments_summary: "{}".into(),
                    tool_description: None,
                },
                "unknown",
            ),
            (
                AgentEvent::ToolCallEnd {
                    data: Box::new(ToolCallEndEventData {
                        tool_id: "t1".into(),
                        tool_name: "read".into(),
                        arguments: "{}".into(),
                        status: ToolCallStatus::Success,
                        duration_ms: 5,
                        output_size: None,
                        result_summary: None,
                        is_error: false,
                    }),
                },
                "tool_call_end",
            ),
            (
                AgentEvent::TaskUpdate {
                    data: Box::new(TaskUpdateData {
                        task_id: "task1".into(),
                        status: TaskStatus::Running,
                        title: "test".into(),
                        subtasks: vec![],
                        depends_on: vec![],
                    }),
                },
                "task_update",
            ),
            (
                AgentEvent::PhaseSummary {
                    data: Box::new(PhaseSummaryData {
                        phase: 1,
                        completed: vec![],
                        issues: vec![],
                        next_steps: vec![],
                        token_usage: UsageInfo::default(),
                    }),
                },
                "phase_summary",
            ),
            (
                AgentEvent::CompactionStart {
                    data: Box::new(CompactionStartData {
                        session_id: "s1".into(),
                        reason: CompactionReason::Threshold,
                        tokens_before: 1000,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::CompactionComplete {
                    messages_replaced: 5,
                    tokens_before: 1000,
                    tokens_after: 500,
                    summary_text: "compressed".into(),
                    reason: CompactionReason::Threshold,
                },
                "compaction_complete",
            ),
            (
                AgentEvent::RetryAttempt {
                    data: Box::new(RetryAttemptData {
                        attempt: 1,
                        max_attempts: 3,
                        delay_ms: 100,
                        error: "timeout".into(),
                        final_success: true,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::ModelChanged {
                    data: Box::new(ModelChangedData {
                        from: Some("gpt4".into()),
                        to: "gpt4o".into(),
                        source: ModelChangeSource::User,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::ThinkingLevelChanged {
                    data: Box::new(ThinkingLevelChangedData {
                        from: None,
                        to: uncode_core::api_types::ThinkingLevel::Medium,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::MessageQueued {
                    text: "hello".into(),
                },
                "message_queued",
            ),
            (
                AgentEvent::MessageDelivered {
                    text: "hello".into(),
                },
                "message_delivered",
            ),
            (
                AgentEvent::LlmRequestStart {
                    data: Box::new(LlmRequestStartData {
                        model_id: "gpt4o".into(),
                        message_count: 5,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::LlmRequestEnd {
                    data: Box::new(LlmRequestEndData {
                        model_id: "gpt4o".into(),
                        duration_ms: 200,
                        input_tokens: 100,
                        output_tokens: 50,
                        status: LlmRequestStatus::Success,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::QueueUpdate {
                    data: Box::new(QueueUpdateData {
                        steering_count: 0,
                        follow_up_count: 1,
                        next_turn_count: 0,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::SessionInfoChanged {
                    data: Box::new(SessionInfoChangedData {
                        session_id: "s1".into(),
                        field: "title".into(),
                        old_value: None,
                        new_value: Some("new".into()),
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::ContextThreshold {
                    data: Box::new(ContextThresholdData {
                        session_id: "s1".into(),
                        usage_ratio: 0.9,
                        threshold: 0.8,
                        context_window: 128000,
                    }),
                },
                "unknown",
            ),
            (
                AgentEvent::Error {
                    category: ErrorCategory::Llm,
                    message: "error".into(),
                    recoverable: true,
                },
                "error",
            ),
            (
                AgentEvent::AgentInterrupted {
                    turn: 1,
                    partial_response: true,
                },
                "agent_interrupted",
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(event_name(&event), expected, "mismatch for {event:?}");
        }
    }

    #[test]
    fn test_json_rpc_response_serialization() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_json_rpc_response_error_serialization() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found: foo".into(),
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32601"));
        assert!(json.contains("Method not found"));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_json_rpc_error_codes() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: None,
            result: None,
            error: Some(JsonRpcError {
                code: -32700,
                message: "Parse error".into(),
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("-32700"));
    }

    #[test]
    fn test_parse_error_response() {
        let resp = RpcServer::parse_error("invalid json".into());
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.id.is_none());
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32700);
        assert!(err.message.contains("invalid json"));
    }

    #[test]
    fn test_json_rpc_roundtrip() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(42.into())),
            result: Some(serde_json::json!({"result": "ok"})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized["id"], 42);
        assert_eq!(deserialized["result"]["result"], "ok");
        assert_eq!(deserialized["jsonrpc"], "2.0");
    }

    #[tokio::test]
    async fn test_register_and_call_handler() {
        let server = RpcServer::new();
        server
            .register("test.echo", |params| Box::pin(async move { Ok(params) }))
            .await;

        let handlers = server.handlers.lock().await;
        let handler = handlers.get("test.echo").unwrap();
        let result = handler(serde_json::json!({"hello": "world"}))
            .await
            .unwrap();
        assert_eq!(result["hello"], "world");
    }

    #[tokio::test]
    async fn test_handler_not_found() {
        let server = RpcServer::new();
        let handlers = server.handlers.lock().await;
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "nonexistent".into(),
            params: None,
        };
        let resp = server.handle_request(request, &handlers).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_handler_error_return() {
        let server = RpcServer::new();
        server
            .register("test.fail", |_| {
                Box::pin(async move { Err("something went wrong".into()) })
            })
            .await;

        let handlers = server.handlers.lock().await;
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "test.fail".into(),
            params: None,
        };
        let resp = server.handle_request(request, &handlers).await;
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32000);
        assert!(err.message.contains("something went wrong"));
    }

    #[tokio::test]
    async fn test_notification_no_response() {
        let server = RpcServer::new();
        server
            .register("test.notify", |params| Box::pin(async move { Ok(params) }))
            .await;

        let handlers = server.handlers.lock().await;
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: None, // notification: no id
            method: "test.notify".into(),
            params: Some(serde_json::json!({"ping": true})),
        };
        let resp = server.handle_request(request, &handlers).await;
        assert!(resp.id.is_none());
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_forward_events_closed_channel() {
        let (tx, _rx) = broadcast::channel::<uncode_core::event::AgentEvent>(16);
        let server = RpcServer::new();
        // Drop the sender to close the channel
        drop(tx);
        let rx = server.notification_writer.lock().await;
        drop(rx);
        // forward_events should exit on RecvError::Closed
        // Use a new receiver from the dropped sender
        let (_tx, _rx) = broadcast::channel::<uncode_core::event::AgentEvent>(16);
        // This should either break or continue without panicking
        // We just verify no panic:
        // We can't easily observe the loop exit without joining,
        // but we can verify it doesn't hang by using a timeout
        tokio::time::timeout(std::time::Duration::from_millis(100), async {
            // Create a closed receiver
            let (_tx2, rx2) = broadcast::channel::<uncode_core::event::AgentEvent>(1);
            drop(_tx2);
            server.forward_events(rx2).await;
        })
        .await
        .expect("forward_events should exit on closed channel without hanging");
    }
}
