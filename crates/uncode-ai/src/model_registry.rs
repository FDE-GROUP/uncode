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
}

impl From<Vec<Model>> for ModelRegistry {
    fn from(models: Vec<Model>) -> Self {
        Self::from_models(models)
    }
}

impl ModelRegistry {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;

    #[test]
    fn new_is_empty() {
        let reg = ModelRegistry::new();
        assert!(reg.all_models().is_empty());
        assert!(!reg.has("anything"));
    }

    #[test]
    fn from_builtin_contains_deepseek_chat() {
        let reg = ModelRegistry::from_builtin();
        assert!(reg.has("deepseek-chat"));
        assert!(reg.all_models().len() > 1);
    }

    #[test]
    fn register_and_get() {
        let reg = ModelRegistry::new();
        let model = Model {
            id: "my-model".into(),
            provider: "test".into(),
            ..Model::default()
        };
        reg.register(model.clone());
        let fetched = reg.get("my-model").unwrap();
        assert_eq!(fetched.id, "my-model");
        assert_eq!(fetched.provider, "test");
    }

    #[test]
    fn register_duplicate_overwrites() {
        let reg = ModelRegistry::new();
        reg.register(Model {
            id: "m".into(),
            name: "first".into(),
            ..Model::default()
        });
        reg.register(Model {
            id: "m".into(),
            name: "second".into(),
            ..Model::default()
        });
        assert_eq!(reg.get("m").unwrap().name, "second");
    }

    #[test]
    fn has_registered_and_unregistered() {
        let reg = ModelRegistry::new();
        reg.register(Model {
            id: "exists".into(),
            ..Model::default()
        });
        assert!(reg.has("exists"));
        assert!(!reg.has("nonexistent"));
    }

    #[test]
    fn unregister_by_provider_removes_only_matching() {
        let reg = ModelRegistry::new();
        reg.register(Model {
            id: "a".into(),
            provider: "provider_a".into(),
            ..Model::default()
        });
        reg.register(Model {
            id: "b".into(),
            provider: "provider_b".into(),
            ..Model::default()
        });
        reg.register(Model {
            id: "c".into(),
            provider: "provider_a".into(),
            ..Model::default()
        });
        assert_eq!(reg.unregister_by_provider("provider_a"), 2);
        assert!(!reg.has("a"));
        assert!(reg.has("b"));
        assert!(!reg.has("c"));
    }

    #[test]
    fn all_models_returns_all() {
        let reg = ModelRegistry::new();
        reg.register(Model {
            id: "x".into(),
            ..Model::default()
        });
        reg.register(Model {
            id: "y".into(),
            ..Model::default()
        });
        let all = reg.all_models();
        assert_eq!(all.len(), 2);
        let ids: Vec<String> = all.into_iter().map(|m| m.id).collect();
        assert!(ids.contains(&"x".into()));
        assert!(ids.contains(&"y".into()));
    }

    #[test]
    fn merge_user_models_adds_new() {
        let reg = ModelRegistry::new();
        reg.merge_user_models(vec![Model {
            id: "new-model".into(),
            provider: "test".into(),
            ..Model::default()
        }]);
        assert!(reg.has("new-model"));
    }

    #[test]
    fn merge_user_models_updates_existing() {
        let reg = ModelRegistry::new();
        reg.register(Model {
            id: "m".into(),
            base_url: "https://original.com".into(),
            ..Model::default()
        });
        reg.merge_user_models(vec![Model {
            id: "m".into(),
            base_url: "https://updated.com".into(),
            ..Model::default()
        }]);
        assert_eq!(reg.get("m").unwrap().base_url, "https://updated.com");
    }

    #[test]
    fn merge_user_models_empty_is_noop() {
        let reg = ModelRegistry::new();
        reg.register(Model {
            id: "m".into(),
            ..Model::default()
        });
        reg.merge_user_models(vec![]);
        assert_eq!(reg.all_models().len(), 1);
    }

    #[test]
    fn from_models_constructor() {
        let models = vec![
            Model {
                id: "a".into(),
                ..Model::default()
            },
            Model {
                id: "b".into(),
                ..Model::default()
            },
        ];
        let reg = ModelRegistry::from_models(models);
        assert!(reg.has("a"));
        assert!(reg.has("b"));
        assert_eq!(reg.all_models().len(), 2);
    }

    #[test]
    fn override_base_url_changes_matching_provider() {
        let reg = ModelRegistry::new();
        reg.register(Model {
            id: "m1".into(),
            provider: "test_provider".into(),
            base_url: "https://old.com".into(),
            ..Model::default()
        });
        reg.register(Model {
            id: "m2".into(),
            provider: "other".into(),
            base_url: "https://other.com".into(),
            ..Model::default()
        });
        reg.override_base_url("test_provider", "https://new.com");
        assert_eq!(reg.get("m1").unwrap().base_url, "https://new.com");
        assert_eq!(reg.get("m2").unwrap().base_url, "https://other.com");
    }

    #[test]
    fn default_delegates_to_new() {
        let reg: ModelRegistry = Default::default();
        assert!(reg.all_models().is_empty());
    }

    #[test]
    fn from_vec_impl() {
        let models = vec![Model {
            id: "from-vec".into(),
            ..Model::default()
        }];
        let reg: ModelRegistry = models.into();
        assert!(reg.has("from-vec"));
    }
}
