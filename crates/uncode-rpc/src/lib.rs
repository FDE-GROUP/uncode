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

pub type RpcHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

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
        handler: impl Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
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
                let responses: Vec<JsonRpcResponse> = requests
                    .into_iter()
                    .map(|r| self.handle_request(r, &handlers))
                    .collect();
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
                    self.handle_request(request, &handlers);
                    continue;
                }
                let handlers = self.handlers.lock().await;
                let resp = self.handle_request(request, &handlers);
                drop(handlers);
                let json = serde_json::to_string(&resp)?;
                let mut w = writer.lock().await;
                let _ = w.write_all(format!("{json}\n").as_bytes()).await;
            }
        }

        Ok(())
    }

    fn handle_request(
        &self,
        request: JsonRpcRequest,
        handlers: &HashMap<String, RpcHandler>,
    ) -> JsonRpcResponse {
        match handlers.get(&request.method) {
            Some(handler) => match handler(request.params.unwrap_or(Value::Null)) {
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
    session_store: Arc<uncode_session::store::SessionStore>,
    provider_registry: Arc<uncode_llm::registry::ProviderRegistry>,
) {
    // session.create
    let ss = session_store.clone();
    server
        .register("session.create", move |params| {
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
            .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({"session_id": id}))
        })
        .await;

    // session.list
    let ss = session_store.clone();
    server
        .register("session.list", move |_| {
            let sessions = ss.list_sessions().map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(sessions).map_err(|e| e.to_string())?)
        })
        .await;

    // session.get
    let ss = session_store.clone();
    server
        .register("session.get", move |params| {
            let id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or("missing session_id")?;
            let header = ss.read_header(id).map_err(|e| e.to_string())?;
            let entries = ss.load_entries(id).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "header": header,
                "entries": entries,
            }))
        })
        .await;

    // model.list
    let pr = provider_registry.clone();
    server
        .register("model.list", move |_| {
            let models: Vec<serde_json::Value> = pr
                .all_models()
                .into_iter()
                .map(|(info, configured)| {
                    serde_json::json!({
                        "id": info.id,
                        "display_name": info.display_name,
                        "provider": info.provider,
                        "max_tokens": info.max_tokens,
                        "configured": configured,
                    })
                })
                .collect();
            Ok(serde_json::to_value(models).map_err(|e| e.to_string())?)
        })
        .await;

    // model.switch
    let pr = provider_registry.clone();
    server
        .register("model.switch", move |params| {
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .ok_or("missing model name")?;
            if !pr.has(model) {
                return Err(format!("model not found: {model}"));
            }
            Ok(serde_json::json!({"switched": model}))
        })
        .await;

    // tool.list
    server
        .register("tool.list", move |_| {
            let tools = vec![
                "read", "write", "edit", "grep", "bash", "find", "ls", "github",
            ];
            Ok(serde_json::to_value(tools).map_err(|e| e.to_string())?)
        })
        .await;

    // message.send
    server
        .register("message.send", move |params| {
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
        .await;

    // message.stream
    server
        .register("message.stream", move |params| {
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
            },
        };
        assert_eq!(event_name(&event), "turn_end");
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

    #[tokio::test]
    async fn test_register_and_call_handler() {
        let server = RpcServer::new();
        server.register("test.echo", |params| Ok(params)).await;

        let handlers = server.handlers.lock().await;
        let handler = handlers.get("test.echo").unwrap();
        let result = handler(serde_json::json!({"hello": "world"})).unwrap();
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
        let resp = server.handle_request(request, &handlers);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
