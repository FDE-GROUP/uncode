use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::driver::LlmDriver;

#[derive(Default)]
pub struct ProviderRegistry {
    drivers: RwLock<HashMap<String, Arc<dyn LlmDriver>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            drivers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, name: String, driver: Arc<dyn LlmDriver>) {
        self.drivers.write().insert(name, driver);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn LlmDriver>> {
        self.drivers.read().get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.drivers.read().keys().cloned().collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.drivers.read().contains_key(name)
    }
}
