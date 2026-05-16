use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn ToolExecutor>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::with_capacity(8)),
        }
    }

    pub fn register(&self, name: impl Into<String>, tool: Arc<dyn ToolExecutor>) {
        self.tools.write().insert(name.into(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.read().get(name).cloned()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.read().values().map(|t| t.definition()).collect()
    }

    pub fn list(&self) -> Vec<String> {
        self.tools.read().keys().cloned().collect()
    }

    /// Get the execution mode for a named tool (defaults to Parallel)
    pub fn execution_mode(&self, name: &str) -> ExecutionMode {
        self.tools
            .read()
            .get(name)
            .map(|t| t.definition().execution_mode)
            .unwrap_or_default()
    }

    /// Get a tool's display label (falls back to name)
    pub fn label_for(&self, name: &str) -> String {
        self.tools
            .read()
            .get(name)
            .map(|t| {
                t.definition()
                    .label
                    .clone()
                    .unwrap_or_else(|| t.definition().name.clone())
            })
            .unwrap_or_else(|| name.to_string())
    }

    /// Check if all named tools can run in parallel
    pub fn can_run_parallel(&self, tool_names: &[String]) -> bool {
        tool_names
            .iter()
            .all(|name| self.execution_mode(name) == ExecutionMode::Parallel)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
