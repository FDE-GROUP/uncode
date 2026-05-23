//! Extension state tracking — records loaded extensions and their metadata.

use std::path::PathBuf;

use dashmap::DashMap;

/// Runtime state of a loaded extension.
#[derive(Debug, Clone)]
pub enum ExtensionState {
    /// Extension is active and hooks/tools are registered.
    Active,
    /// Extension is being reloaded.
    Reloading,
    /// Extension failed to load/reload. Contains the error message.
    Error(String),
    /// Extension is disabled — hooks/tools unregistered but record kept.
    Disabled,
}

/// Where the extension was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionSource {
    /// `~/.uncode/extensions/`
    Global,
    /// `.uncode/extensions/` (project-local)
    Project,
}

impl std::fmt::Display for ExtensionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "global"),
            Self::Project => write!(f, "project"),
        }
    }
}

/// Metadata record for a loaded extension.
#[derive(Debug, Clone)]
pub struct ExtensionRecord {
    pub name: String,
    pub state: ExtensionState,
    pub wasm_path: PathBuf,
    pub source: ExtensionSource,
    pub tools: Vec<String>,
    pub hooks: Vec<String>,
}

/// Thread-safe tracker for extension state.
pub struct ExtensionStateTracker {
    records: DashMap<String, ExtensionRecord>,
}

impl ExtensionStateTracker {
    pub fn new() -> Self {
        Self {
            records: DashMap::new(),
        }
    }

    pub fn insert(&self, record: ExtensionRecord) {
        self.records.insert(record.name.clone(), record);
    }

    pub fn get(&self, name: &str) -> Option<ExtensionRecord> {
        self.records.get(name).map(|r| r.value().clone())
    }

    pub fn list(&self) -> Vec<ExtensionRecord> {
        self.records.iter().map(|r| r.value().clone()).collect()
    }

    pub fn update_state(&self, name: &str, state: ExtensionState) -> bool {
        if let Some(mut entry) = self.records.get_mut(name) {
            entry.state = state;
            true
        } else {
            false
        }
    }

    pub fn remove(&self, name: &str) -> Option<ExtensionRecord> {
        self.records.remove(name).map(|(_, v)| v)
    }

    /// Find which extension owns a tool by tool name.
    pub fn find_by_tool(&self, tool_name: &str) -> Option<String> {
        self.records
            .iter()
            .find(|r| r.value().tools.iter().any(|t| t == tool_name))
            .map(|r| r.key().clone())
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl Default for ExtensionStateTracker {
    fn default() -> Self {
        Self::new()
    }
}
