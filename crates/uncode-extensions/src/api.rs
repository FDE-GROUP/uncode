use std::sync::Arc;

use crate::hooks::{Extension, HookRegistry, LifecycleHook};
use crate::tool::ExtensionTool;

/// Callback type for tool registration. Injected by `uncode-agent`.
///
/// Receives (tool_name, tool_arc). Returns `Err` on validation failure.
pub type ToolRegistrationCallback =
    Arc<dyn Fn(String, Arc<dyn ExtensionTool>) -> Result<(), String> + Send + Sync>;

/// 扩展开发者的注册 API 入口。
///
/// **Pi:** 对照扩展安装/注册门面；无同名 TS 类型。
pub struct ExtensionApi {
    registry: Arc<HookRegistry>,
    tool_callback: Option<ToolRegistrationCallback>,
}

impl ExtensionApi {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self {
            registry,
            tool_callback: None,
        }
    }

    /// Create with a tool registration callback (called by uncode-agent).
    pub fn with_tool_callback(
        registry: Arc<HookRegistry>,
        callback: ToolRegistrationCallback,
    ) -> Self {
        Self {
            registry,
            tool_callback: Some(callback),
        }
    }

    /// 注册一个扩展及其监听的钩子
    pub fn register_extension(&self, ext: Arc<dyn Extension>, hooks: Vec<LifecycleHook>) {
        self.registry.register(ext, hooks);
    }

    /// 获取 HookRegistry 的引用
    pub fn registry(&self) -> &Arc<HookRegistry> {
        &self.registry
    }

    /// Register an LLM-callable custom tool from an extension.
    ///
    /// Validates the tool metadata, then delegates to the callback injected
    /// by `uncode-agent` which creates the adapter and inserts into `ToolRegistry`.
    pub fn register_tool(&self, tool: Arc<dyn ExtensionTool>) -> Result<(), String> {
        let meta = tool.metadata();
        meta.validate()?;
        let callback = self
            .tool_callback
            .as_ref()
            .ok_or("tool registration not available: no callback configured")?;
        callback(meta.name, tool)
    }
}
