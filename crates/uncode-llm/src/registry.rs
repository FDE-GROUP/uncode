use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::driver::LlmDriver;
use uncode_core::model::{ModelInfo, ModelPricing};

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

    /// 返回所有已知模型信息，标记已配置/未配置
    pub fn all_models(&self) -> Vec<(ModelInfo, bool)> {
        let configured: Vec<String> = self.names();
        builtin_models()
            .into_iter()
            .map(|m| {
                let is_configured = configured.contains(&m.id);
                (m, is_configured)
            })
            .collect()
    }
}

fn builtin_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "deepseek-v3".into(),
            provider: "deepseek".into(),
            display_name: "DeepSeek V3".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
            pricing: Some(ModelPricing {
                input_per_1k: 0.003,
                output_per_1k: 0.015,
                cache_read_per_1k: Some(0.001),
            }),
        },
        ModelInfo {
            id: "deepseek-v4-pro".into(),
            provider: "deepseek".into(),
            display_name: "DeepSeek V4 Pro".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
            pricing: Some(ModelPricing {
                input_per_1k: 0.005,
                output_per_1k: 0.025,
                cache_read_per_1k: Some(0.001),
            }),
        },
        ModelInfo {
            id: "glm-5.1".into(),
            provider: "glm".into(),
            display_name: "GLM 5.1".into(),
            max_tokens: 128_000,
            supports_vision: true,
            supports_tools: true,
            pricing: Some(ModelPricing {
                input_per_1k: 0.005,
                output_per_1k: 0.02,
                cache_read_per_1k: None,
            }),
        },
        ModelInfo {
            id: "ollama".into(),
            provider: "ollama".into(),
            display_name: "Ollama (local)".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
            pricing: None,
        },
    ]
}
