//! Helpers for reading/writing WASM linear memory through wasmtime.

use wasmtime::{Memory, Store};

use super::WasmError;

/// Typed function signatures for WASM exports.
pub struct WasmExports {
    pub init: wasmtime::TypedFunc<(i32,), ()>,
    pub on_hook: wasmtime::TypedFunc<(i32, i32, i32), i32>,
    pub tool_execute: wasmtime::TypedFunc<(i32, i32, i32, i32, i32), i32>,
    pub allocate: wasmtime::TypedFunc<(i32,), i32>,
    pub deallocate: wasmtime::TypedFunc<(i32, i32), ()>,
    pub memory: Memory,
}

impl WasmExports {
    /// Look up all required exports from a wasmtime Instance.
    pub fn from_instance(
        instance: &wasmtime::Instance,
        store: &mut Store<HostState>,
    ) -> Result<Self, WasmError> {
        let missing = |name: &str| WasmError::MissingExport(name.into());

        let init = instance
            .get_typed_func::<(i32,), ()>(&mut *store, "__uncode_init")
            .map_err(|_| missing("__uncode_init"))?;

        let on_hook = instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut *store, "__uncode_on_hook")
            .map_err(|_| missing("__uncode_on_hook"))?;

        let tool_execute = instance
            .get_typed_func::<(i32, i32, i32, i32, i32), i32>(&mut *store, "__uncode_tool_execute")
            .map_err(|_| missing("__uncode_tool_execute"))?;

        let allocate = instance
            .get_typed_func::<(i32,), i32>(&mut *store, "__uncode_allocate")
            .map_err(|_| missing("__uncode_allocate"))?;

        let deallocate = instance
            .get_typed_func::<(i32, i32), ()>(&mut *store, "__uncode_deallocate")
            .map_err(|_| missing("__uncode_deallocate"))?;

        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or_else(|| missing("memory"))?;

        Ok(Self {
            init,
            on_hook,
            tool_execute,
            allocate,
            deallocate,
            memory,
        })
    }
}

/// State stored in the wasmtime Store, accessible from host imports.
pub struct HostState {
    /// The extension name (for logging).
    pub extension_name: String,
    /// Opaque handle passed to `__uncode_init`.
    pub api_handle: u32,
    /// Hooks registered during init.
    pub registered_hooks: Vec<String>,
    /// Tool metadata registered during init.
    pub registered_tools: Vec<crate::tool::ExtensionToolMetadata>,
    /// ExtensionApi reference for host imports.
    pub ext_api: std::sync::Arc<crate::api::ExtensionApi>,
}
