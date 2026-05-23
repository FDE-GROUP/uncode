//! WasmEngine — wasmtime Engine + Linker singleton with sandbox config.

use std::sync::Arc;

use crate::api::ExtensionApi;
use crate::hooks::LifecycleHook;

use super::WasmError;
use super::host_imports;
use super::instance::WasmInstance;
use super::manifest::ExtensionManifest;
use super::memory::{HostState, WasmExports};
use super::tool::WasmExtensionTool;

/// Shared WASM engine with pre-configured sandbox and host imports.
pub struct WasmEngine {
    engine: wasmtime::Engine,
    linker: wasmtime::Linker<HostState>,
}

impl WasmEngine {
    /// Create a new engine with sandbox defaults.
    pub fn new() -> Result<Self, WasmError> {
        let mut config = wasmtime::Config::new();

        // Enable fuel for CPU limiting.
        config.consume_fuel(true);

        // Use Cranelift optimizer backend.
        config.strategy(wasmtime::Strategy::Cranelift);

        let engine =
            wasmtime::Engine::new(&config).map_err(|e| WasmError::Compilation(e.to_string()))?;

        let mut linker = wasmtime::Linker::new(&engine);

        // Register host imports.
        host_imports::setup_linker(&mut linker)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;

        Ok(Self { engine, linker })
    }

    /// Compile and instantiate a WASM module.
    ///
    /// Returns `(WasmInstance, Vec<WasmExtensionTool>)` — the instance implements
    /// the `Extension` trait for hooks, and each tool implements `ExtensionTool`.
    pub fn instantiate(
        &self,
        wasm_bytes: &[u8],
        manifest: ExtensionManifest,
        ext_api: Arc<ExtensionApi>,
    ) -> Result<(WasmInstance, Vec<WasmExtensionTool>), WasmError> {
        // Compile module.
        let module = wasmtime::Module::from_binary(&self.engine, wasm_bytes)
            .map_err(|e| WasmError::Compilation(e.to_string()))?;

        // Create store with fuel.
        let mut store = wasmtime::Store::new(
            &self.engine,
            HostState {
                extension_name: manifest.name.clone(),
                api_handle: 1,
                registered_hooks: Vec::new(),
                registered_tools: Vec::new(),
                ext_api,
            },
        );

        // Set fuel limit.
        store
            .set_fuel(manifest.fuel_limit())
            .map_err(|e| WasmError::Instantiation(format!("fuel config: {e}")))?;

        // Instantiate module with linker (host imports auto-wired).
        let instance = self
            .linker
            .instantiate(&mut store, &module)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;

        // Resolve required exports.
        let exports = WasmExports::from_instance(&instance, &mut store)?;

        // Call __uncode_init — the extension registers hooks via host imports.
        let timeout = manifest.timeout();
        exports
            .init
            .call(&mut store, (1,))
            .map_err(|e| WasmError::Trap(e.to_string()))?;

        // Collect hooks registered during init.
        let hooks: Vec<String> = store.data_mut().registered_hooks.drain(..).collect();

        // Resolve hook names to LifecycleHook variants.
        let lifecycle_hooks: Vec<LifecycleHook> = hooks
            .iter()
            .filter_map(|h| parse_lifecycle_hook(h))
            .collect();

        // If manifest declares hooks but none registered via host import, use manifest.
        let lifecycle_hooks = if lifecycle_hooks.is_empty() && !manifest.hooks.is_empty() {
            manifest
                .hooks
                .iter()
                .filter_map(|h| parse_lifecycle_hook(h))
                .collect()
        } else {
            lifecycle_hooks
        };

        // Default: register for common hooks if neither method provided any.
        let lifecycle_hooks = if lifecycle_hooks.is_empty() {
            vec![
                LifecycleHook::SessionStart,
                LifecycleHook::TurnStart,
                LifecycleHook::TurnEnd,
                LifecycleHook::SessionEnd,
            ]
        } else {
            lifecycle_hooks
        };

        tracing::info!(
            "loaded WASM extension: {} (hooks: {:?})",
            manifest.name,
            lifecycle_hooks.iter().map(|h| h.name()).collect::<Vec<_>>()
        );

        // Collect tool metadata registered during __uncode_init.
        let tool_metas: Vec<crate::tool::ExtensionToolMetadata> =
            store.data_mut().registered_tools.drain(..).collect();

        let instance = WasmInstance::new(manifest.name, store, exports, lifecycle_hooks, timeout);

        let inner = instance.inner_clone();
        let tools: Vec<WasmExtensionTool> = tool_metas
            .into_iter()
            .map(|meta| WasmExtensionTool::new(meta, inner.clone()))
            .collect();

        if !tools.is_empty() {
            tracing::info!("WASM extension registered {} tool(s)", tools.len());
        }

        Ok((instance, tools))
    }

    /// Reference to the underlying wasmtime Engine.
    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }
}

/// Parse a lifecycle hook name string into a `LifecycleHook` variant.
fn parse_lifecycle_hook(name: &str) -> Option<LifecycleHook> {
    match name {
        "session_start" => Some(LifecycleHook::SessionStart),
        "turn_start" => Some(LifecycleHook::TurnStart),
        "message_received" => Some(LifecycleHook::MessageReceived),
        "message_sending" => Some(LifecycleHook::MessageSending),
        "tool_call_before" => Some(LifecycleHook::ToolCallBefore),
        "tool_call_after" => Some(LifecycleHook::ToolCallAfter),
        "turn_end" => Some(LifecycleHook::TurnEnd),
        "session_end" => Some(LifecycleHook::SessionEnd),
        _ => {
            tracing::warn!("unknown lifecycle hook: {name}");
            None
        }
    }
}
