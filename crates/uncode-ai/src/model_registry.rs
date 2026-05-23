use parking_lot::RwLock;
use std::collections::HashMap;

use crate::model::{Model, builtin_models};
use crate::provider_preset::apply_provider_preset;

/// 模型注册表——按 id 查找 Model 数据。
///
/// **Pi:** 对应内置模型表 + 用户覆盖；API-first，不按厂商单独驱动 crate。
///
/// Uses interior mutability (`RwLock`) so that dynamic provider registration
/// from extensions can add models without requiring `&mut self`.
pub struct ModelRegistry {
    models: RwLock<HashMap<String, Model>>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: RwLock::new(HashMap::new()),
        }
    }

    /// 加载内置模型数据集
    pub fn from_builtin() -> Self {
        let models = builtin_models()
            .into_iter()
            .map(apply_provider_preset)
            .map(|m| (m.id.clone(), m))
            .collect();
        Self {
            models: RwLock::new(models),
        }
    }

    pub fn register(&self, model: Model) {
        self.models.write().insert(model.id.clone(), model);
    }

    /// Remove all models whose `provider` field equals the given name.
    /// Returns the number of models removed.
    pub fn unregister_by_provider(&self, provider: &str) -> usize {
        let mut models = self.models.write();
        let ids_to_remove: Vec<String> = models
            .iter()
            .filter(|(_, m)| m.provider == provider)
            .map(|(id, _)| id.clone())
            .collect();
        let count = ids_to_remove.len();
        for id in ids_to_remove {
            models.remove(&id);
        }
        count
    }

    pub fn get(&self, id: &str) -> Option<Model> {
        self.models.read().get(id).cloned()
    }

    pub fn has(&self, id: &str) -> bool {
        self.models.read().contains_key(id)
    }

    pub fn all_models(&self) -> Vec<Model> {
        self.models.read().values().cloned().collect()
    }

    /// 合并用户自定义模型——同 id 覆盖，新 id 追加
    pub fn merge_user_models(&self, user_models: Vec<Model>) {
        let mut models = self.models.write();
        for m in user_models {
            let m = apply_provider_preset(m);
            models.insert(m.id.clone(), m);
        }
    }

    pub fn from_models(models: Vec<Model>) -> Self {
        let models = models.into_iter().map(|m| (m.id.clone(), m)).collect();
        Self {
            models: RwLock::new(models),
        }
    }

    pub fn override_base_url(&self, provider: &str, base_url: &str) {
        let mut models = self.models.write();
        for model in models.values_mut() {
            if model.provider == provider {
                model.base_url = base_url.to_string();
            }
        }
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}
