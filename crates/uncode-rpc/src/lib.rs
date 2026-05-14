//! uncode-rpc — JSON-RPC 2.0 over stdio
//!
//! 提供 JSON-RPC 2.0 协议实现，供 IDE/外部工具通过 stdio 集成。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
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

pub type RpcHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

pub struct RpcServer {
    handlers: Mutex<HashMap<String, RpcHandler>>,
}

impl RpcServer {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
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

    pub async fn serve(&self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();
        let writer = Arc::new(Mutex::new(stdout));

        tracing::info!("JSON-RPC server started");

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let resp = JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {e}"),
                        }),
                    };
                    let mut w = writer.lock().await;
                    let _ = w
                        .write_all(format!("{}\n", serde_json::to_string(&resp)?).as_bytes())
                        .await;
                    continue;
                }
            };

            let handlers = self.handlers.lock().await;
            let response = match handlers.get(&request.method) {
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
            };
            drop(handlers);

            let mut w = writer.lock().await;
            let _ = w
                .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
                .await;
        }

        Ok(())
    }
}

impl Default for RpcServer {
    fn default() -> Self {
        Self::new()
    }
}
