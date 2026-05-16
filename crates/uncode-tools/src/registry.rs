use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use uncode_core::tool::{ToolDefinition, ToolExecutor};

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
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
