//! WasmExtensionTool — bridges WASM tool execution to the ExtensionTool trait.

use std::sync::Arc;

use crate::tool::{ExtensionTool, ExtensionToolMetadata};

use super::WasmError;
use super::instance::WasmInstanceInner;

/// A tool backed by a WASM module's `__uncode_tool_execute` export.
///
/// Shares the same `WasmInstanceInner` as the parent `WasmInstance`,
/// so tool execution locks the same wasmtime Store.
pub struct WasmExtensionTool {
    metadata: ExtensionToolMetadata,
    inner: Arc<std::sync::Mutex<WasmInstanceInner>>,
}

impl WasmExtensionTool {
    pub fn new(
        metadata: ExtensionToolMetadata,
        inner: Arc<std::sync::Mutex<WasmInstanceInner>>,
    ) -> Self {
        Self { metadata, inner }
    }
}

#[async_trait::async_trait]
impl ExtensionTool for WasmExtensionTool {
    fn metadata(&self) -> ExtensionToolMetadata {
        self.metadata.clone()
    }

    async fn execute(&self, arguments: serde_json::Value) -> anyhow::Result<String> {
        let inner = self.inner.clone();
        let tool_name = self.metadata.name.clone();
        let args_bytes = serde_json::to_vec(&arguments)?;

        tokio::task::spawn_blocking(move || {
            let mut guard = super::safe_lock(&inner, "wasi_tool_execute");

            if guard.disabled {
                return Err(anyhow::anyhow!(
                    "extension disabled, tool '{tool_name}' cannot execute"
                ));
            }

            let WasmInstanceInner {
                ref mut store,
                ref exports,
                ref mut disabled,
            } = *guard;

            // Write tool name into WASM memory.
            let name_ptr = exports
                .allocate
                .call(&mut *store, (tool_name.len() as i32,))
                .map_err(|e| WasmError::Trap(e.to_string()))?;
            {
                let mem = exports.memory.data_mut(&mut *store);
                let start = name_ptr as usize;
                let end = start + tool_name.len();
                if end > mem.len() {
                    return Err(WasmError::AbiViolation("name ptr out of bounds".into()).into());
                }
                mem[start..end].copy_from_slice(tool_name.as_bytes());
            }

            // Write args into WASM memory.
            let args_ptr = exports
                .allocate
                .call(&mut *store, (args_bytes.len() as i32,))
                .map_err(|e| WasmError::Trap(e.to_string()))?;
            {
                let mem = exports.memory.data_mut(&mut *store);
                let start = args_ptr as usize;
                let end = start + args_bytes.len();
                if end > mem.len() {
                    return Err(WasmError::AbiViolation("args ptr out of bounds".into()).into());
                }
                mem[start..end].copy_from_slice(&args_bytes);
            }

            // Allocate output buffer.
            let out_ptr = exports.allocate.call(&mut *store, (4096,)).map_err(|e| {
                *disabled = true;
                WasmError::Trap(e.to_string())
            })?;

            // Reset fuel.
            let _ = store.set_fuel(10_000_000);

            // Call __uncode_tool_execute.
            let result = exports.tool_execute.call(
                &mut *store,
                (
                    name_ptr,
                    tool_name.len() as i32,
                    args_ptr,
                    args_bytes.len() as i32,
                    out_ptr,
                ),
            );

            // Deallocate input buffers.
            let _ = exports
                .deallocate
                .call(&mut *store, (name_ptr, tool_name.len() as i32));
            let _ = exports
                .deallocate
                .call(&mut *store, (args_ptr, args_bytes.len() as i32));

            match result {
                Ok(0) => Ok(String::new()),
                Ok(result_len) => {
                    let mem = exports.memory.data(&*store);
                    let start = out_ptr as usize;
                    let end = start + result_len as usize;
                    if end > mem.len() {
                        return Err(WasmError::AbiViolation("result out of bounds".into()).into());
                    }
                    let s = std::str::from_utf8(&mem[start..end])
                        .map_err(|e| WasmError::AbiViolation(format!("utf-8: {e}")))?;
                    Ok(s.to_string())
                }
                Err(e) => {
                    tracing::warn!("WASM tool '{tool_name}' trapped: {e}");
                    *disabled = true;
                    Err(WasmError::Trap(e.to_string()).into())
                }
            }
        })
        .await
        .map_err(|_| WasmError::Timeout(std::time::Duration::from_secs(5)))?
    }
}
