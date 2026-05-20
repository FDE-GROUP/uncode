use std::sync::Arc;

use crate::hooks::{Extension, HookRegistry, LifecycleHook};

/// 扩展开发者的注册 API 入口。
///
/// **Pi:** 对照扩展安装/注册门面；无同名 TS 类型。
pub struct ExtensionApi {
    registry: Arc<HookRegistry>,
}

impl ExtensionApi {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self { registry }
    }

    /// 注册一个扩展及其监听的钩子
    pub fn register_extension(&self, ext: Arc<dyn Extension>, hooks: Vec<LifecycleHook>) {
        self.registry.register(ext, hooks);
    }

    /// 获取 HookRegistry 的引用
    pub fn registry(&self) -> &Arc<HookRegistry> {
        &self.registry
    }
}
