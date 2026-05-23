use std::path::Path;
use std::sync::Arc;

use crate::api::ExtensionApi;
use crate::hooks::HookRegistry;
use crate::tool::ExtensionTool;

/// WASM extension loader — scans directories and instantiates `.wasm` files.
///
/// **Pi:** 对照 `pi install` / 包路径发现；实现为 `.wasm` 而非 npm/git 包。
pub struct ExtensionLoader;

impl ExtensionLoader {
    pub fn new() -> Self {
        Self
    }

    /// Scan a directory for `.wasm` extension files and load them.
    ///
    /// Each `.wasm` file is compiled and instantiated in a sandboxed wasmtime
    /// store. A companion `.json` manifest file (same name, different extension)
    /// may provide metadata and sandbox configuration.
    ///
    /// Loading failures are logged and do not prevent other extensions from loading.
    /// Returns the count of successfully loaded extensions.
    pub async fn load_from_dir(
        &self,
        registry: &HookRegistry,
        api: &Arc<ExtensionApi>,
        dir: &Path,
    ) -> anyhow::Result<usize> {
        if !dir.exists() {
            tracing::debug!("extension directory does not exist: {}", dir.display());
            return Ok(0);
        }

        let engine = match super::wasm::WasmEngine::new() {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("failed to create WASM engine: {e}");
                return Ok(0);
            }
        };

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("cannot read extension directory: {e}");
                return Ok(0);
            }
        };

        let mut loaded = 0usize;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("error reading directory entry: {e}");
                    continue;
                }
            };

            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("wasm") {
                continue;
            }

            let wasm_bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("cannot read {}: {e}", path.display());
                    continue;
                }
            };

            let manifest = match super::wasm::ExtensionManifest::load(&path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("manifest error for {}: {e}, using defaults", path.display());
                    super::wasm::ExtensionManifest::default_for(&path)
                }
            };

            match engine.instantiate(&wasm_bytes, manifest.clone(), api.clone()) {
                Ok((instance, tools)) => {
                    let ext: Arc<dyn crate::hooks::Extension> = Arc::new(instance);
                    // Resolve lifecycle hooks from manifest.
                    let hooks = if manifest.hooks.is_empty() {
                        // Default: register for common hooks.
                        vec![
                            crate::hooks::LifecycleHook::SessionStart,
                            crate::hooks::LifecycleHook::TurnStart,
                            crate::hooks::LifecycleHook::TurnEnd,
                            crate::hooks::LifecycleHook::SessionEnd,
                        ]
                    } else {
                        manifest
                            .hooks
                            .iter()
                            .filter_map(|h| match h.as_str() {
                                "session_start" => Some(crate::hooks::LifecycleHook::SessionStart),
                                "turn_start" => Some(crate::hooks::LifecycleHook::TurnStart),
                                "message_received" => {
                                    Some(crate::hooks::LifecycleHook::MessageReceived)
                                }
                                "message_sending" => {
                                    Some(crate::hooks::LifecycleHook::MessageSending)
                                }
                                "tool_call_before" => {
                                    Some(crate::hooks::LifecycleHook::ToolCallBefore)
                                }
                                "tool_call_after" => {
                                    Some(crate::hooks::LifecycleHook::ToolCallAfter)
                                }
                                "turn_end" => Some(crate::hooks::LifecycleHook::TurnEnd),
                                "session_end" => Some(crate::hooks::LifecycleHook::SessionEnd),
                                _ => None,
                            })
                            .collect()
                    };
                    registry.register(ext, hooks);

                    // Register WASM tools via ExtensionApi callback.
                    for tool in tools {
                        let tool: Arc<dyn ExtensionTool> = Arc::new(tool);
                        if let Err(e) = api.register_tool(tool) {
                            tracing::warn!("failed to register WASM tool: {e}");
                        }
                    }

                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!("failed to load extension {}: {e}", path.display());
                }
            }
        }

        if loaded > 0 {
            tracing::info!("loaded {loaded} WASM extension(s) from {}", dir.display());
        }

        Ok(loaded)
    }
}

impl Default for ExtensionLoader {
    fn default() -> Self {
        Self::new()
    }
}
