use std::path::Path;

use crate::hooks::HookRegistry;

pub struct ExtensionLoader;

impl ExtensionLoader {
    pub fn new() -> Self {
        Self
    }

    /// 扫描目录中的 .wasm 文件并注册到 HookRegistry
    pub async fn load_from_dir(
        &self,
        _registry: &HookRegistry,
        _dir: &Path,
    ) -> anyhow::Result<usize> {
        // WASM 运行时集成将在后续实现
        // 当前返回 0 表示未加载任何扩展
        tracing::info!("WASM extension loading not yet implemented");
        Ok(0)
    }
}

impl Default for ExtensionLoader {
    fn default() -> Self {
        Self::new()
    }
}
