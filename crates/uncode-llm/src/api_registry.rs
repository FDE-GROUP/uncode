use std::collections::HashMap;
use std::sync::Arc;

use crate::api::Api;

/// API 注册表——启动时构建，之后只读
pub struct ApiRegistry {
    apis: HashMap<String, Arc<dyn Api>>,
}

impl ApiRegistry {
    pub fn new() -> Self {
        Self {
            apis: HashMap::new(),
        }
    }

    pub fn register(&mut self, api: Arc<dyn Api>) {
        self.apis.insert(api.api_name().to_string(), api);
    }

    pub fn get(&self, api_name: &str) -> Option<Arc<dyn Api>> {
        self.apis.get(api_name).cloned()
    }

    pub fn has(&self, api_name: &str) -> bool {
        self.apis.contains_key(api_name)
    }

    pub fn names(&self) -> Vec<String> {
        self.apis.keys().cloned().collect()
    }
}

impl Default for ApiRegistry {
    fn default() -> Self {
        Self::new()
    }
}
