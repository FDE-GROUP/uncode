use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::driver::LlmDriver;
use uncode_core::config::ModelConfig;
use uncode_core::model::ModelInfo;

#[derive(Default)]
pub struct ProviderRegistry {
    drivers: RwLock<HashMap<String, Arc<dyn LlmDriver>>>,
    models: RwLock<Vec<ModelConfig>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            drivers: RwLock::new(HashMap::with_capacity(8)),
            models: RwLock::new(Vec::new()),
        }
    }

    pub fn with_models(models: Vec<ModelConfig>) -> Self {
        Self {
            drivers: RwLock::new(HashMap::with_capacity(8)),
            models: RwLock::new(models),
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

    /// Return all known models, marking which are configured
    pub fn all_models(&self) -> Vec<(ModelInfo, bool)> {
        let configured: Vec<String> = self.names();
        self.models
            .read()
            .iter()
            .map(|m| {
                let info = ModelInfo {
                    id: m.id.clone(),
                    provider: m.provider.clone(),
                    display_name: m.display_name.clone(),
                    max_tokens: m.max_tokens,
                    supports_vision: m.supports_vision,
                    supports_tools: m.supports_tools,
                    pricing: None,
                };
                let is_configured = configured.contains(&m.provider);
                (info, is_configured)
            })
            .collect()
    }
}
