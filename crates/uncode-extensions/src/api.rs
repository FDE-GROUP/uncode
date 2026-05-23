use std::sync::Arc;

use crate::command::{CommandRegistration, ShortcutRegistration};
use crate::hooks::{Extension, HookRegistry, LifecycleHook};
use crate::provider::ProviderRegistration;
use crate::tool::ExtensionTool;

/// Callback type for tool registration. Injected by `uncode-agent`.
///
/// Receives (tool_name, tool_arc). Returns `Err` on validation failure.
pub type ToolRegistrationCallback =
    Arc<dyn Fn(String, Arc<dyn ExtensionTool>) -> Result<(), String> + Send + Sync>;

/// Callback type for tool unregistration. Injected by `uncode-agent`.
pub type ToolUnregisterCallback = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Callback type for slash command registration. Injected by `uncode-cli`.
pub type CommandRegistrationCallback =
    Arc<dyn Fn(CommandRegistration) -> Result<(), String> + Send + Sync>;

/// Callback type for command unregistration. Injected by `uncode-cli`.
pub type CommandUnregisterCallback = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Callback type for keyboard shortcut registration. Injected by `uncode-cli`.
pub type ShortcutRegistrationCallback =
    Arc<dyn Fn(ShortcutRegistration) -> Result<(), String> + Send + Sync>;

/// Callback type for dynamic LLM provider registration. Injected by `uncode-cli`.
pub type ProviderRegistrationCallback =
    Arc<dyn Fn(ProviderRegistration) -> Result<(), String> + Send + Sync>;

/// Callback type for dynamic provider unregistration. Injected by `uncode-cli`.
pub type ProviderUnregisterCallback = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// 扩展开发者的注册 API 入口。
///
/// **Pi:** 对照扩展安装/注册门面；无同名 TS 类型。
pub struct ExtensionApi {
    registry: Arc<HookRegistry>,
    tool_callback: Option<ToolRegistrationCallback>,
    tool_unregister_callback: Option<ToolUnregisterCallback>,
    command_callback: Option<CommandRegistrationCallback>,
    command_unregister_callback: Option<CommandUnregisterCallback>,
    shortcut_callback: Option<ShortcutRegistrationCallback>,
    provider_callback: Option<ProviderRegistrationCallback>,
    provider_unregister_callback: Option<ProviderUnregisterCallback>,
}

impl ExtensionApi {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self {
            registry,
            tool_callback: None,
            tool_unregister_callback: None,
            command_callback: None,
            command_unregister_callback: None,
            shortcut_callback: None,
            provider_callback: None,
            provider_unregister_callback: None,
        }
    }

    /// Create with all optional callbacks (called by uncode-cli).
    #[allow(clippy::too_many_arguments)]
    pub fn with_callbacks(
        registry: Arc<HookRegistry>,
        tool_callback: Option<ToolRegistrationCallback>,
        tool_unregister_callback: Option<ToolUnregisterCallback>,
        command_callback: Option<CommandRegistrationCallback>,
        command_unregister_callback: Option<CommandUnregisterCallback>,
        shortcut_callback: Option<ShortcutRegistrationCallback>,
        provider_callback: Option<ProviderRegistrationCallback>,
        provider_unregister_callback: Option<ProviderUnregisterCallback>,
    ) -> Self {
        Self {
            registry,
            tool_callback,
            tool_unregister_callback,
            command_callback,
            command_unregister_callback,
            shortcut_callback,
            provider_callback,
            provider_unregister_callback,
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
    pub fn register_tool(&self, tool: Arc<dyn ExtensionTool>) -> Result<(), String> {
        let meta = tool.metadata();
        meta.validate()?;
        let callback = self
            .tool_callback
            .as_ref()
            .ok_or("tool registration not available: no callback configured")?;
        callback(meta.name, tool)
    }

    /// Unregister a previously registered tool by name.
    pub fn unregister_tool(&self, name: &str) -> bool {
        if let Some(callback) = &self.tool_unregister_callback {
            callback(name)
        } else {
            false
        }
    }

    /// Register a slash command from an extension.
    pub fn register_command(&self, cmd: CommandRegistration) -> Result<(), String> {
        cmd.validate()?;
        let callback = self
            .command_callback
            .as_ref()
            .ok_or("command registration not available: no callback configured")?;
        callback(cmd)
    }

    /// Unregister a previously registered slash command by name.
    pub fn unregister_command(&self, name: &str) -> bool {
        if let Some(callback) = &self.command_unregister_callback {
            callback(name)
        } else {
            false
        }
    }

    /// Register a keyboard shortcut from an extension.
    pub fn register_shortcut(&self, shortcut: ShortcutRegistration) -> Result<(), String> {
        shortcut.validate()?;
        let callback = self
            .shortcut_callback
            .as_ref()
            .ok_or("shortcut registration not available: no callback configured")?;
        callback(shortcut)
    }

    /// Register a dynamic LLM provider from an extension.
    pub fn register_provider(&self, config: ProviderRegistration) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .provider_callback
            .as_ref()
            .ok_or("provider registration not available: no callback configured")?;
        callback(config)
    }

    /// Unregister a previously registered dynamic provider by name.
    pub fn unregister_provider(&self, name: &str) -> bool {
        if let Some(callback) = &self.provider_unregister_callback {
            callback(name)
        } else {
            false
        }
    }
}
