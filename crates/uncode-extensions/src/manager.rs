//! ExtensionManager — orchestrates extension lifecycle: load, unload, reload, disable, enable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::api::ExtensionApi;
use crate::hooks::{Extension, HookRegistry, LifecycleHook};
use crate::state::{ExtensionRecord, ExtensionSource, ExtensionState, ExtensionStateTracker};
use crate::tool::ExtensionTool;

/// Summary of a discovery-and-load sweep.
#[derive(Debug)]
pub struct DiscoveryReport {
    pub loaded: Vec<String>,
    pub errors: Vec<(String, String)>,
}

/// Manages the full lifecycle of WASM extensions.
pub struct ExtensionManager {
    registry: Arc<HookRegistry>,
    api: Arc<ExtensionApi>,
    state: ExtensionStateTracker,
    #[cfg(feature = "wasm")]
    engine: Option<super::wasm::WasmEngine>,
    global_dir: PathBuf,
    project_dir: Option<PathBuf>,
}

impl ExtensionManager {
    /// Create a new manager.
    ///
    /// `global_dir`: typically `~/.uncode/extensions/`
    /// `project_dir`: typically `<cwd>/.uncode/extensions/`, may not exist
    pub fn new(
        registry: Arc<HookRegistry>,
        api: Arc<ExtensionApi>,
        global_dir: PathBuf,
        project_dir: Option<PathBuf>,
    ) -> Self {
        #[cfg(feature = "wasm")]
        let engine = match super::wasm::WasmEngine::new() {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::error!("failed to create WASM engine: {e}");
                None
            }
        };
        Self {
            registry,
            api,
            state: ExtensionStateTracker::new(),
            #[cfg(feature = "wasm")]
            engine,
            global_dir,
            project_dir,
        }
    }

    /// Scan both global and project directories, load all discovered extensions.
    ///
    /// Project-level extensions take priority over global ones with the same name.
    #[cfg(feature = "wasm")]
    pub fn discover_and_load_all(&self) -> DiscoveryReport {
        let mut discovered: HashMap<String, (PathBuf, ExtensionSource)> = HashMap::new();

        // Global extensions (lower priority).
        scan_wasm_files(&self.global_dir, ExtensionSource::Global, &mut discovered);

        // Project extensions (higher priority — overwrites global entries).
        if let Some(proj) = &self.project_dir {
            scan_wasm_files(proj, ExtensionSource::Project, &mut discovered);
        }

        let mut loaded = Vec::new();
        let mut errors = Vec::new();

        for (name, (path, source)) in discovered {
            match self.load_single(&path, source) {
                Ok(_) => loaded.push(name),
                Err(e) => errors.push((name, e)),
            }
        }

        if !loaded.is_empty() {
            tracing::info!("extension manager: loaded {} extension(s)", loaded.len());
        }

        DiscoveryReport { loaded, errors }
    }

    /// Load a single extension from a `.wasm` file.
    #[cfg(feature = "wasm")]
    pub fn load_single(&self, wasm_path: &Path, source: ExtensionSource) -> Result<String, String> {
        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| format!("cannot read {}: {e}", wasm_path.display()))?;

        let manifest = super::wasm::ExtensionManifest::load(wasm_path)
            .unwrap_or_else(|_| super::wasm::ExtensionManifest::default_for(wasm_path));

        let engine = self.engine.as_ref().ok_or("WASM engine not available")?;

        let (instance, tools) = engine
            .instantiate(&wasm_bytes, manifest.clone(), self.api.clone())
            .map_err(|e| e.to_string())?;

        let ext_name = instance.instance_name().to_string();

        // Resolve lifecycle hooks.
        let hooks = resolve_hooks(&manifest);

        // Register extension hooks.
        let ext: Arc<dyn Extension> = Arc::new(instance);
        self.registry.register(ext, hooks.clone());

        // Register tools and track names.
        let mut tool_names = Vec::new();
        for tool in tools {
            let meta = tool.metadata();
            tool_names.push(meta.name.clone());
            let tool: Arc<dyn ExtensionTool> = Arc::new(tool);
            if let Err(e) = self.api.register_tool(tool) {
                tracing::warn!("extension {ext_name}: tool registration failed: {e}");
            }
        }

        let hook_names: Vec<String> = hooks.iter().map(|h| h.name().to_string()).collect();

        self.state.insert(ExtensionRecord {
            name: ext_name.clone(),
            state: ExtensionState::Active,
            wasm_path: wasm_path.to_path_buf(),
            source,
            tools: tool_names,
            hooks: hook_names,
        });

        Ok(ext_name)
    }

    /// Unload an extension: unregister all hooks, tools, and providers, remove state record.
    pub fn unload(&self, name: &str) -> Result<(), String> {
        let record = self
            .state
            .get(name)
            .ok_or_else(|| format!("extension '{name}' not found"))?;

        // Unregister tools.
        for tool_name in &record.tools {
            self.api.unregister_tool(tool_name);
        }

        // Unregister provider (best-effort).
        self.api.unregister_provider(name);

        // Unregister hooks.
        self.registry.unregister(name);

        self.state.remove(name);
        tracing::info!("extension '{name}' unloaded");
        Ok(())
    }

    /// Reload an extension: unload the old instance and load fresh from disk.
    #[cfg(feature = "wasm")]
    pub fn reload(&self, name: &str) -> Result<(), String> {
        let record = self
            .state
            .get(name)
            .ok_or_else(|| format!("extension '{name}' not found"))?;

        let wasm_path = record.wasm_path.clone();
        let source = record.source;

        self.state.update_state(name, ExtensionState::Reloading);

        // Unregister hooks and tools, but keep state record.
        for tool_name in &record.tools {
            self.api.unregister_tool(tool_name);
        }
        self.registry.unregister(name);

        // Reload from disk.
        match self.load_single(&wasm_path, source) {
            Ok(new_name) => {
                if new_name != name {
                    // Extension changed its name during reload — remove old record.
                    self.state.remove(name);
                }
                tracing::info!("extension '{name}' reloaded");
                Ok(())
            }
            Err(e) => {
                self.state
                    .update_state(name, ExtensionState::Error(e.clone()));
                tracing::error!("extension '{name}' reload failed: {e}");
                Err(e)
            }
        }
    }

    /// Disable an extension: unregister hooks/tools but keep the record.
    pub fn disable(&self, name: &str) -> Result<(), String> {
        let record = self
            .state
            .get(name)
            .ok_or_else(|| format!("extension '{name}' not found"))?;

        for tool_name in &record.tools {
            self.api.unregister_tool(tool_name);
        }
        self.api.unregister_provider(name);
        self.registry.unregister(name);

        self.state.update_state(name, ExtensionState::Disabled);
        tracing::info!("extension '{name}' disabled");
        Ok(())
    }

    /// Enable a previously disabled extension: reload from disk.
    #[cfg(feature = "wasm")]
    pub fn enable(&self, name: &str) -> Result<(), String> {
        let record = self
            .state
            .get(name)
            .ok_or_else(|| format!("extension '{name}' not found"))?;

        if !matches!(record.state, ExtensionState::Disabled) {
            return Err(format!("extension '{name}' is not disabled"));
        }

        self.load_single(&record.wasm_path, record.source)?;
        tracing::info!("extension '{name}' enabled");
        Ok(())
    }

    /// List all tracked extensions.
    pub fn list(&self) -> Vec<ExtensionRecord> {
        self.state.list()
    }

    /// Reference to the state tracker (for testing).
    pub fn state_tracker(&self) -> &ExtensionStateTracker {
        &self.state
    }
}

/// Scan a directory for `.wasm` files and add to the discovered map.
#[cfg(feature = "wasm")]
fn scan_wasm_files(
    dir: &Path,
    source: ExtensionSource,
    discovered: &mut HashMap<String, (PathBuf, ExtensionSource)>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("wasm") {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        discovered.insert(name, (path, source));
    }
}

/// Resolve lifecycle hooks from manifest.
#[cfg(feature = "wasm")]
fn resolve_hooks(manifest: &super::wasm::ExtensionManifest) -> Vec<LifecycleHook> {
    if manifest.hooks.is_empty() {
        vec![
            LifecycleHook::SessionStart,
            LifecycleHook::TurnStart,
            LifecycleHook::TurnEnd,
            LifecycleHook::SessionEnd,
        ]
    } else {
        manifest
            .hooks
            .iter()
            .filter_map(|h| match h.as_str() {
                "session_start" => Some(LifecycleHook::SessionStart),
                "turn_start" => Some(LifecycleHook::TurnStart),
                "message_received" => Some(LifecycleHook::MessageReceived),
                "message_sending" => Some(LifecycleHook::MessageSending),
                "tool_call_before" => Some(LifecycleHook::ToolCallBefore),
                "tool_call_after" => Some(LifecycleHook::ToolCallAfter),
                "turn_end" => Some(LifecycleHook::TurnEnd),
                "session_end" => Some(LifecycleHook::SessionEnd),
                _ => None,
            })
            .collect()
    }
}
