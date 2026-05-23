//! Host function imports — functions the WASM module can call.

use wasmtime::{Caller, Linker};

use super::memory::HostState;

/// Populate the wasmtime Linker with host import functions.
pub fn setup_linker(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
    linker.func_wrap("uncode", "__uncode_host_register_hook", host_register_hook)?;

    linker.func_wrap("uncode", "__uncode_host_register_tool", host_register_tool)?;

    linker.func_wrap(
        "uncode",
        "__uncode_host_register_command",
        host_register_command,
    )?;

    linker.func_wrap(
        "uncode",
        "__uncode_host_register_shortcut",
        host_register_shortcut,
    )?;

    linker.func_wrap("uncode", "__uncode_host_log", host_log)?;

    linker.func_wrap("uncode", "__uncode_host_get_cwd", host_get_cwd)?;

    Ok(())
}

/// Read a byte slice from WASM memory at (ptr, len). Returns None if out of bounds.
fn read_memory_bytes(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    if len <= 0 {
        return None;
    }
    let memory = caller.get_export("memory")?.into_memory()?;
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start + len as usize;
    if end > data.len() {
        return None;
    }
    Some(data[start..end].to_vec())
}

fn host_register_hook(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return,
    };

    match std::str::from_utf8(&bytes) {
        Ok(hook_name) => {
            let ext_name = caller.data().extension_name.clone();
            tracing::debug!("extension {ext_name} registers hook: {hook_name}");
            caller
                .data_mut()
                .registered_hooks
                .push(hook_name.to_string());
        }
        Err(_) => {
            let ext_name = caller.data().extension_name.clone();
            tracing::warn!("extension {ext_name} sent invalid utf-8 hook name");
        }
    }
}

fn host_register_tool(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };

    let ext_name = caller.data().extension_name.clone();
    match std::str::from_utf8(&bytes) {
        Ok(json_str) => {
            match serde_json::from_str::<crate::tool::ExtensionToolMetadata>(json_str) {
                Ok(meta) => {
                    if let Err(e) = meta.validate() {
                        tracing::warn!("extension {ext_name} tool metadata invalid: {e}");
                        return -1;
                    }
                    tracing::debug!(
                        "extension {ext_name} registers tool '{}' via host import",
                        meta.name
                    );
                    caller.data_mut().registered_tools.push(meta);
                    caller.data_mut().registered_tools.len() as i32 - 1 // tool_id
                }
                Err(e) => {
                    tracing::warn!("extension {ext_name} sent invalid tool metadata: {e}");
                    -1
                }
            }
        }
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 tool metadata");
            -1
        }
    }
}

fn host_register_command(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return,
    };

    let ext_name = caller.data().extension_name.clone();
    match std::str::from_utf8(&bytes) {
        Ok(json_str) => {
            match serde_json::from_str::<crate::command::CommandRegistration>(json_str) {
                Ok(_cmd) => {
                    tracing::debug!("extension {ext_name} registers command via host import");
                }
                Err(e) => {
                    tracing::warn!("extension {ext_name} sent invalid command: {e}");
                }
            }
        }
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 command");
        }
    }
}

fn host_register_shortcut(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return,
    };

    let ext_name = caller.data().extension_name.clone();
    match std::str::from_utf8(&bytes) {
        Ok(json_str) => {
            match serde_json::from_str::<crate::command::ShortcutRegistration>(json_str) {
                Ok(_shortcut) => {
                    tracing::debug!("extension {ext_name} registers shortcut via host import");
                }
                Err(e) => {
                    tracing::warn!("extension {ext_name} sent invalid shortcut: {e}");
                }
            }
        }
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 shortcut");
        }
    }
}

fn host_log(mut caller: Caller<'_, HostState>, level: i32, ptr: i32, len: i32) {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return,
    };

    if let Ok(msg) = std::str::from_utf8(&bytes) {
        let ext_name = caller.data().extension_name.clone();
        match level {
            0 => tracing::trace!("[{ext_name}] {msg}"),
            1 => tracing::debug!("[{ext_name}] {msg}"),
            2 => tracing::info!("[{ext_name}] {msg}"),
            3 => tracing::warn!("[{ext_name}] {msg}"),
            _ => tracing::error!("[{ext_name}] {msg}"),
        }
    }
}

fn host_get_cwd(_caller: Caller<'_, HostState>, _out_ptr: i32) -> i32 {
    // Phase 4: return empty — extensions don't need CWD without WASI.
    0
}
