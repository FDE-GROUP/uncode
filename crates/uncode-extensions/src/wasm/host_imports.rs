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

    linker.func_wrap(
        "uncode",
        "__uncode_host_register_provider",
        host_register_provider,
    )?;

    linker.func_wrap(
        "uncode",
        "__uncode_host_register_renderer",
        host_register_renderer,
    )?;

    linker.func_wrap("uncode", "__uncode_host_show_dialog", host_show_dialog)?;

    linker.func_wrap("uncode", "__uncode_host_abort", host_abort)?;
    linker.func_wrap("uncode", "__uncode_host_compact", host_compact)?;
    linker.func_wrap("uncode", "__uncode_host_is_idle", host_is_idle)?;

    linker.func_wrap("uncode", "__uncode_host_show_overlay", host_show_overlay)?;
    linker.func_wrap("uncode", "__uncode_host_hide_overlay", host_hide_overlay)?;
    linker.func_wrap(
        "uncode",
        "__uncode_host_update_overlay",
        host_update_overlay,
    )?;

    linker.func_wrap("uncode", "__uncode_host_set_widget", host_set_widget)?;
    linker.func_wrap("uncode", "__uncode_host_remove_widget", host_remove_widget)?;
    linker.func_wrap("uncode", "__uncode_host_set_status", host_set_status)?;
    linker.func_wrap("uncode", "__uncode_host_notify", host_notify)?;
    linker.func_wrap(
        "uncode",
        "__uncode_host_register_message_renderer",
        host_register_message_renderer,
    )?;

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

fn host_register_provider(
    mut caller: Caller<'_, HostState>,
    _handle: i32,
    ptr: i32,
    len: i32,
) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };

    let ext_name = caller.data().extension_name.clone();
    match std::str::from_utf8(&bytes) {
        Ok(json_str) => {
            match serde_json::from_str::<crate::provider::ProviderRegistration>(json_str) {
                Ok(reg) => {
                    if let Err(e) = reg.validate() {
                        tracing::warn!("extension {ext_name} provider registration invalid: {e}");
                        return -1;
                    }
                    tracing::debug!(
                        "extension {ext_name} registers provider '{}' with {} model(s)",
                        reg.name,
                        reg.models.len()
                    );
                    match caller.data().ext_api.register_provider(reg) {
                        Ok(()) => 0,
                        Err(e) => {
                            tracing::warn!(
                                "extension {ext_name} provider registration failed: {e}"
                            );
                            -1
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("extension {ext_name} sent invalid provider JSON: {e}");
                    -1
                }
            }
        }
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 provider data");
            -1
        }
    }
}

fn host_register_renderer(
    mut caller: Caller<'_, HostState>,
    _handle: i32,
    ptr: i32,
    len: i32,
) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };

    let ext_name = caller.data().extension_name.clone();
    match std::str::from_utf8(&bytes) {
        Ok(json_str) => match serde_json::from_str::<crate::renderer::ToolRenderConfig>(json_str) {
            Ok(config) => match caller.data().ext_api.register_renderer(config) {
                Ok(()) => 0,
                Err(e) => {
                    tracing::warn!("extension {ext_name} renderer registration failed: {e}");
                    -1
                }
            },
            Err(e) => {
                tracing::warn!("extension {ext_name} sent invalid renderer JSON: {e}");
                -1
            }
        },
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 renderer data");
            -1
        }
    }
}

fn host_show_dialog(
    mut caller: Caller<'_, HostState>,
    _handle: i32,
    ptr: i32,
    len: i32,
    out_ptr: i32,
) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };

    let ext_name = caller.data().extension_name.clone();
    let json_str = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 dialog request");
            return -1;
        }
    };

    let request: uncode_core::dialog::DialogRequest = match serde_json::from_str(json_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("extension {ext_name} sent invalid dialog JSON: {e}");
            return -1;
        }
    };

    match caller.data().ext_api.show_dialog(request) {
        Ok(response) => {
            let response_json = match serde_json::to_string(&response) {
                Ok(j) => j,
                Err(e) => {
                    tracing::warn!("extension {ext_name} dialog response serialize failed: {e}");
                    return -1;
                }
            };
            let response_bytes = response_json.as_bytes();
            let response_len = response_bytes.len() as i32;
            let memory = match caller.get_export("memory").and_then(|m| m.into_memory()) {
                Some(m) => m,
                None => return -1,
            };
            let data = memory.data_mut(&mut caller);
            let start = out_ptr as usize;
            let end = start + response_bytes.len();
            if end > data.len() {
                return -1;
            }
            data[start..end].copy_from_slice(response_bytes);
            response_len
        }
        Err(e) => {
            tracing::warn!("extension {ext_name} show_dialog failed: {e}");
            -1
        }
    }
}

fn host_abort(caller: Caller<'_, HostState>, _handle: i32) {
    let ext_name = caller.data().extension_name.clone();
    tracing::info!("extension {ext_name} requested agent abort");
    caller.data().ext_api.abort();
}

fn host_compact(caller: Caller<'_, HostState>, _handle: i32) {
    let ext_name = caller.data().extension_name.clone();
    tracing::info!("extension {ext_name} requested context compaction");
    caller.data().ext_api.compact();
}

fn host_is_idle(caller: Caller<'_, HostState>, _handle: i32) -> i32 {
    if caller.data().ext_api.is_idle() {
        1
    } else {
        0
    }
}

fn host_show_overlay(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };
    let ext_name = caller.data().extension_name.clone();
    let json_str = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 overlay show request");
            return -1;
        }
    };
    let payload: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("extension {ext_name} sent invalid overlay JSON: {e}");
            return -1;
        }
    };
    let config: uncode_core::overlay::OverlayConfig =
        match serde_json::from_value(payload.get("config").cloned().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("extension {ext_name} overlay config invalid: {e}");
                return -1;
            }
        };
    let content: uncode_core::overlay::OverlayContent =
        match serde_json::from_value(payload.get("content").cloned().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("extension {ext_name} overlay content invalid: {e}");
                return -1;
            }
        };
    match caller.data().ext_api.show_overlay(config, content) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} show_overlay failed: {e}");
            -1
        }
    }
}

fn host_hide_overlay(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };
    let ext_name = caller.data().extension_name.clone();
    let key = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 overlay key");
            return -1;
        }
    };
    match caller.data().ext_api.hide_overlay(key) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} hide_overlay failed: {e}");
            -1
        }
    }
}

fn host_update_overlay(
    mut caller: Caller<'_, HostState>,
    _handle: i32,
    key_ptr: i32,
    key_len: i32,
    content_ptr: i32,
    content_len: i32,
) -> i32 {
    let key_bytes = match read_memory_bytes(&mut caller, key_ptr, key_len) {
        Some(b) => b,
        None => return -1,
    };
    let ext_name = caller.data().extension_name.clone();
    let key = match std::str::from_utf8(&key_bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 overlay key");
            return -1;
        }
    };
    let content_bytes = match read_memory_bytes(&mut caller, content_ptr, content_len) {
        Some(b) => b,
        None => return -1,
    };
    let content: uncode_core::overlay::OverlayContent = match serde_json::from_slice(&content_bytes)
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("extension {ext_name} sent invalid overlay content JSON: {e}");
            return -1;
        }
    };
    match caller.data().ext_api.update_overlay(key, content) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} update_overlay failed: {e}");
            -1
        }
    }
}

fn host_set_widget(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };
    let ext_name = caller.data().extension_name.clone();
    let config: uncode_core::ui_action::WidgetConfig = match serde_json::from_slice(&bytes) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("extension {ext_name} sent invalid widget config JSON: {e}");
            return -1;
        }
    };
    match caller.data().ext_api.set_widget(config) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} set_widget failed: {e}");
            -1
        }
    }
}

fn host_remove_widget(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };
    let ext_name = caller.data().extension_name.clone();
    let key = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 widget key");
            return -1;
        }
    };
    match caller.data().ext_api.remove_widget(key) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} remove_widget failed: {e}");
            -1
        }
    }
}

fn host_set_status(mut caller: Caller<'_, HostState>, _handle: i32, ptr: i32, len: i32) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };
    let ext_name = caller.data().extension_name.clone();
    // Expect JSON: { "key": "...", "text": "..." | null }
    let payload: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("extension {ext_name} sent invalid status JSON: {e}");
            return -1;
        }
    };
    let key = match payload.get("key").and_then(|v| v.as_str()) {
        Some(k) => k.to_string(),
        None => {
            tracing::warn!("extension {ext_name} status missing 'key' field");
            return -1;
        }
    };
    let text = payload
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    match caller.data().ext_api.set_status(&key, text) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} set_status failed: {e}");
            -1
        }
    }
}

fn host_notify(
    mut caller: Caller<'_, HostState>,
    _handle: i32,
    level: i32,
    ptr: i32,
    len: i32,
) -> i32 {
    let ext_name = caller.data().extension_name.clone();
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };
    let message = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("extension {ext_name} sent invalid utf-8 notify message");
            return -1;
        }
    };
    let notify_type = match level {
        0 => uncode_core::ui_action::NotifyType::Info,
        1 => uncode_core::ui_action::NotifyType::Warning,
        2 => uncode_core::ui_action::NotifyType::Error,
        _ => uncode_core::ui_action::NotifyType::Info,
    };
    match caller.data().ext_api.notify(message, notify_type) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} notify failed: {e}");
            -1
        }
    }
}

fn host_register_message_renderer(
    mut caller: Caller<'_, HostState>,
    _handle: i32,
    ptr: i32,
    len: i32,
) -> i32 {
    let bytes = match read_memory_bytes(&mut caller, ptr, len) {
        Some(b) => b,
        None => return -1,
    };
    let ext_name = caller.data().extension_name.clone();
    let config: crate::message_renderer::MessageRenderConfig = match serde_json::from_slice(&bytes)
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("extension {ext_name} sent invalid message renderer config JSON: {e}");
            return -1;
        }
    };
    match caller.data().ext_api.register_message_renderer(config) {
        Ok(()) => 0,
        Err(e) => {
            tracing::warn!("extension {ext_name} register_message_renderer failed: {e}");
            -1
        }
    }
}
