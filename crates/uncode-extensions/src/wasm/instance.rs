//! WasmInstance — adapter wrapping a wasmtime Instance as an Extension.

use std::sync::Arc;
use std::time::Duration;

use wasmtime::Store;

use crate::hooks::{HookContext, HookResult};

use super::WasmError;
use super::memory::{HostState, WasmExports};

/// Inner mutable state — shared between WasmInstance and WasmExtensionTool.
pub struct WasmInstanceInner {
    pub store: Store<HostState>,
    pub exports: WasmExports,
    pub disabled: bool,
}

/// A WASM-based extension that implements the `Extension` trait.
///
/// Each `.wasm` file gets its own `WasmInstance` with an isolated store.
pub struct WasmInstance {
    name: String,
    inner: Arc<std::sync::Mutex<WasmInstanceInner>>,
    timeout: Duration,
}

impl WasmInstance {
    pub fn new(
        name: String,
        store: Store<HostState>,
        exports: WasmExports,
        _lifecycle_hooks: Vec<crate::hooks::LifecycleHook>,
        timeout: Duration,
    ) -> Self {
        Self {
            name,
            inner: Arc::new(std::sync::Mutex::new(WasmInstanceInner {
                store,
                exports,
                disabled: false,
            })),
            timeout,
        }
    }

    pub fn instance_name(&self) -> &str {
        &self.name
    }

    pub fn is_disabled(&self) -> bool {
        self.inner.lock().unwrap().disabled
    }

    /// Clone the inner Arc for sharing with WasmExtensionTool instances.
    pub fn inner_clone(&self) -> Arc<std::sync::Mutex<WasmInstanceInner>> {
        self.inner.clone()
    }
}

/// Execute a hook call synchronously. Returns the result or disables the instance.
fn call_on_hook(
    inner: &Arc<std::sync::Mutex<WasmInstanceInner>>,
    ctx_bytes: &[u8],
) -> anyhow::Result<HookResult> {
    let mut guard = inner.lock().unwrap();

    if guard.disabled {
        return Ok(HookResult::Continue);
    }

    // Destructure to avoid simultaneous borrows.
    let WasmInstanceInner {
        ref mut store,
        ref exports,
        ref mut disabled,
    } = *guard;

    let ctx_len = ctx_bytes.len() as i32;

    // Allocate space in WASM memory for context.
    let ctx_ptr = exports
        .allocate
        .call(&mut *store, (ctx_bytes.len() as i32,))
        .map_err(|e| {
            *disabled = true;
            WasmError::Trap(e.to_string())
        })?;

    // Write context bytes.
    {
        let mem = exports.memory.data_mut(&mut *store);
        let start = ctx_ptr as usize;
        let end = start + ctx_bytes.len();
        if end > mem.len() {
            return Err(WasmError::AbiViolation("ctx ptr out of bounds".into()).into());
        }
        mem[start..end].copy_from_slice(ctx_bytes);
    }

    // Allocate output buffer.
    let out_ptr = exports.allocate.call(&mut *store, (1024,)).map_err(|e| {
        *disabled = true;
        WasmError::Trap(e.to_string())
    })?;

    // Reset fuel.
    let _ = store.set_fuel(10_000_000);

    // Call on_hook.
    let call_result = exports
        .on_hook
        .call(&mut *store, (ctx_ptr, ctx_len, out_ptr));

    // Deallocate context buffer.
    let _ = exports.deallocate.call(&mut *store, (ctx_ptr, ctx_len));

    match call_result {
        Ok(0) => Ok(HookResult::Continue),
        Ok(result_len) => {
            let result_json = {
                let mem = exports.memory.data(&*store);
                let start = out_ptr as usize;
                let end = start + result_len as usize;
                if end > mem.len() {
                    return Err(WasmError::AbiViolation("result out of bounds".into()).into());
                }
                std::str::from_utf8(&mem[start..end])
                    .map(|s| s.to_string())
                    .map_err(|e| WasmError::AbiViolation(format!("utf-8: {e}")))?
            };
            parse_hook_result(&result_json)
        }
        Err(e) => {
            let ext_name = store.data().extension_name.clone();
            tracing::warn!("WASM extension {ext_name} trapped: {e}");
            *disabled = true;
            Ok(HookResult::Continue)
        }
    }
}

#[async_trait::async_trait]
impl crate::hooks::Extension for WasmInstance {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_hook(&self, ctx: &HookContext) -> anyhow::Result<HookResult> {
        let inner = self.inner.clone();

        // Serialize full context including event data.
        let ctx_bytes = serde_json::to_vec(ctx)
            .map_err(|e| WasmError::AbiViolation(format!("serialize: {e}")))?;

        // Run in blocking thread since wasmtime is sync.
        tokio::task::spawn_blocking(move || call_on_hook(&inner, &ctx_bytes))
            .await
            .map_err(|_| WasmError::Timeout(self.timeout))?
    }
}

fn parse_hook_result(json: &str) -> anyhow::Result<HookResult> {
    let val: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| WasmError::AbiViolation(format!("invalid hook result json: {e}")))?;

    match val.get("type").and_then(|t| t.as_str()) {
        Some("block") => Ok(HookResult::Block {
            reason: val
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("blocked by extension")
                .into(),
        }),
        Some("modify") => {
            let mut mods = crate::hooks::HookModification::default();
            if let Some(args) = val.get("args_override") {
                mods.args_override = Some(args.clone());
            }
            if let Some(b) = val.get("is_error_override").and_then(|v| v.as_bool()) {
                mods.is_error_override = Some(b);
            }
            if let Some(b) = val.get("terminate_override").and_then(|v| v.as_bool()) {
                mods.terminate_override = Some(b);
            }
            Ok(HookResult::Modify(mods))
        }
        _ => Ok(HookResult::Continue),
    }
}
