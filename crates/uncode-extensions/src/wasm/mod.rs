//! WASM extension runtime — wasmtime-based sandboxed execution.
//!
//! Loads `.wasm` files from `~/.uncode/extensions/`, instantiates them in
//! isolated wasmtime stores, and bridges the flat ABI to the Extension trait.

mod engine;
mod host_imports;
mod instance;
mod manifest;
mod memory;
mod tool;

pub use engine::WasmEngine;
pub use instance::WasmInstance;
pub use manifest::{ExtensionManifest, ExtensionPermissions};
pub use tool::WasmExtensionTool;

use std::time::Duration;

/// Errors produced by the WASM runtime.
#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    #[error("compilation failed: {0}")]
    Compilation(String),

    #[error("instantiation failed: {0}")]
    Instantiation(String),

    #[error("missing export: {0}")]
    MissingExport(String),

    #[error("ABI violation: {0}")]
    AbiViolation(String),

    #[error("timeout after {0:?}")]
    Timeout(Duration),

    #[error("trap: {0}")]
    Trap(String),

    #[error("manifest error: {0}")]
    Manifest(String),
}

/// Default memory limit in MB.
const DEFAULT_MEMORY_LIMIT_MB: u32 = 64;

/// Default fuel limit per hook call (instruction count).
const DEFAULT_FUEL_LIMIT: u64 = 10_000_000;

/// Default timeout per WASM call.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[track_caller]
pub(crate) fn safe_lock<'a, T>(
    lock: &'a std::sync::Mutex<T>,
    name: &'static str,
) -> std::sync::MutexGuard<'a, T> {
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("mutex '{name}' poisoned, recovering");
            poisoned.into_inner()
        }
    }
}
