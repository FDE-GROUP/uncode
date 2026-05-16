use std::collections::HashMap;

use crate::model::{Model, builtin_models};

/// 模型注册表——按 id 查找 Model 数据
pub struct ModelRegistry {
    models: HashMap<String, Model>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    /// 加载内置模型数据集
    pub fn from_builtin() -> Self {
        let models = builtin_models()
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect();
        Self { models }
    }

    pub fn register(&mut self, model: Model) {
        self.models.insert(model.id.clone(), model);
    }

    pub fn get(&self, id: &str) -> Option<&Model> {
        self.models.get(id)
    }

    pub fn has(&self, id: &str) -> bool {
        self.models.contains_key(id)
    }

    pub fn all_models(&self) -> Vec<&Model> {
        self.models.values().collect()
    }

    /// 合并用户自定义模型——同 id 覆盖，新 id 追加
    pub fn merge_user_models(&mut self, user_models: Vec<Model>) {
        for m in user_models {
            self.models.insert(m.id.clone(), m);
        }
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}
