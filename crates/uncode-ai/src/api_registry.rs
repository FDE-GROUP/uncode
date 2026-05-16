use std::collections::HashMap;
use std::sync::Arc;

use crate::api::Api;

/// 延迟加载器类型：首次使用时才构造 API 实例
type LazyLoader = Box<dyn Fn() -> Arc<dyn Api> + Send + Sync>;

/// API 注册表——支持即时注册、延迟加载和动态卸载
pub struct ApiRegistry {
    apis: HashMap<String, Arc<dyn Api>>,
    lazy_loaders: HashMap<String, LazyLoader>,
}

impl ApiRegistry {
    pub fn new() -> Self {
        Self {
            apis: HashMap::new(),
            lazy_loaders: HashMap::new(),
        }
    }

    /// 即时注册 API
    pub fn register(&mut self, api: Arc<dyn Api>) {
        self.apis.insert(api.api_name().to_string(), api);
    }

    /// 延迟注册：首次 get 时才调用 loader 构造实例
    pub fn register_lazy(
        &mut self,
        api_name: &str,
        loader: impl Fn() -> Arc<dyn Api> + Send + Sync + 'static,
    ) {
        self.lazy_loaders
            .insert(api_name.to_string(), Box::new(loader));
    }

    /// 获取 API（自动触发延迟加载）
    pub fn get(&self, api_name: &str) -> Option<Arc<dyn Api>> {
        if let Some(api) = self.apis.get(api_name) {
            return Some(api.clone());
        }
        // 延迟加载需要 &mut self，这里无法触发
        // 延迟加载的 API 需要通过 get_or_init 获取
        None
    }

    /// 获取 API，支持延迟加载
    pub fn get_or_init(&mut self, api_name: &str) -> Option<Arc<dyn Api>> {
        if let Some(api) = self.apis.get(api_name) {
            return Some(api.clone());
        }
        if let Some(loader) = self.lazy_loaders.remove(api_name) {
            let api = loader();
            let name = api.api_name().to_string();
            self.apis.insert(name, api.clone());
            Some(api)
        } else {
            None
        }
    }

    /// 卸载已注册的 API
    pub fn unregister(&mut self, api_name: &str) -> bool {
        self.lazy_loaders.remove(api_name);
        self.apis.remove(api_name).is_some()
    }

    /// 清空所有注册
    pub fn clear(&mut self) {
        self.apis.clear();
        self.lazy_loaders.clear();
    }

    pub fn has(&self, api_name: &str) -> bool {
        self.apis.contains_key(api_name) || self.lazy_loaders.contains_key(api_name)
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.apis.keys().cloned().collect();
        for name in self.lazy_loaders.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }
}

impl Default for ApiRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StreamEvent;
    use crate::api_types::{Context, StreamOptions};
    use crate::model::Model;
    use async_trait::async_trait;
    use futures::Stream;
    use std::pin::Pin;
    use uncode_shared::error::UncodeError;

    type BoxStream = Pin<Box<dyn Stream<Item = StreamEvent> + Send + 'static>>;

    struct MockApi {
        name: &'static str,
    }

    #[async_trait]
    impl Api for MockApi {
        fn api_name(&self) -> &'static str {
            self.name
        }
        async fn stream(
            &self,
            _model: &Model,
            _context: &Context,
            _options: &StreamOptions,
        ) -> Result<BoxStream, UncodeError> {
            Err(UncodeError::Other("mock".into()))
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = ApiRegistry::new();
        reg.register(Arc::new(MockApi { name: "test-api" }));
        assert!(reg.get("test-api").is_some());
        assert!(reg.get("unknown").is_none());
    }

    #[test]
    fn test_register_lazy_and_get_or_init() {
        let mut reg = ApiRegistry::new();
        reg.register_lazy("lazy-api", || Arc::new(MockApi { name: "lazy-api" }));

        // get() 不会触发延迟加载
        assert!(reg.get("lazy-api").is_none());
        assert!(reg.has("lazy-api"));

        // get_or_init 触发加载
        let api = reg.get_or_init("lazy-api");
        assert!(api.is_some());
        assert_eq!(api.unwrap().api_name(), "lazy-api");

        // 第二次 get_or_init 从缓存返回
        let api2 = reg.get_or_init("lazy-api");
        assert!(api2.is_some());

        // loader 已被消费
        assert!(reg.lazy_loaders.is_empty());
    }

    #[test]
    fn test_unregister() {
        let mut reg = ApiRegistry::new();
        reg.register(Arc::new(MockApi { name: "api-1" }));
        assert!(reg.unregister("api-1"));
        assert!(!reg.has("api-1"));
        assert!(!reg.unregister("api-1")); // 已移除
    }

    #[test]
    fn test_unregister_lazy() {
        let mut reg = ApiRegistry::new();
        reg.register_lazy("lazy", || Arc::new(MockApi { name: "lazy" }));
        assert!(!reg.unregister("lazy")); // lazy_loader 不是已注册 API
        assert!(!reg.has("lazy")); // loader 也被移除了
    }

    #[test]
    fn test_clear() {
        let mut reg = ApiRegistry::new();
        reg.register(Arc::new(MockApi { name: "a" }));
        reg.register_lazy("b", || Arc::new(MockApi { name: "b" }));
        reg.clear();
        assert!(reg.names().is_empty());
        assert!(!reg.has("a"));
    }

    #[test]
    fn test_names() {
        let mut reg = ApiRegistry::new();
        reg.register(Arc::new(MockApi { name: "alpha" }));
        reg.register_lazy("beta", || Arc::new(MockApi { name: "beta" }));
        let mut names = reg.names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }
}
