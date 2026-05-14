use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::driver::LlmDriver;

pub struct ProviderRegistry {
    drivers: RwLock<HashMap<String, Arc<dyn LlmDriver>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            drivers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, name: impl Into<String>, driver: Arc<dyn LlmDriver>) {
        self.drivers.write().insert(name.into(), driver);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn LlmDriver>> {
        self.drivers.read().get(name).cloned()
    }

    pub fn list(&self) -> Vec<String> {
        self.drivers.read().keys().cloned().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
