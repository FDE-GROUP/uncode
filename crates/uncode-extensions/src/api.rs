use std::sync::Arc;

use crate::command::{CommandRegistration, ShortcutRegistration};
use crate::event_bus::{EventBus, EventHandler, SubscriptionId};
use crate::header_footer::{FooterConfig, HeaderConfig, WorkingIndicatorConfig};
use crate::hooks::{Extension, HookRegistry, LifecycleHook};
use crate::message_renderer::MessageRenderConfig;
use crate::provider::ProviderRegistration;
use crate::renderer::ToolRenderConfig;
use crate::resource::ResourcePathConfig;
use crate::session::{SessionAction, SessionResponse};
use crate::theme_control::{ThemeControlConfig, ThinkingLabelConfig};
use crate::tool::ExtensionTool;

use uncode_core::dialog::{DialogRequest, DialogResponse};
use uncode_core::overlay::{OverlayAction, OverlayConfig, OverlayContent};
use uncode_core::ui_action::{NotifyType, UiAction, WidgetConfig};

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

/// Callback type for tool renderer registration. Injected by `uncode-cli`.
pub type RendererRegistrationCallback =
    Arc<dyn Fn(ToolRenderConfig) -> Result<(), String> + Send + Sync>;

/// Callback type for dialog interaction. Injected by `uncode-cli`.
/// Blocks until the user responds.
pub type DialogCallback =
    Arc<dyn Fn(DialogRequest) -> Result<DialogResponse, String> + Send + Sync>;

/// Callback type for agent abort. Injected by `uncode-cli`.
pub type AbortCallback = Arc<dyn Fn() + Send + Sync>;

/// Callback type for triggering context compaction. Injected by `uncode-cli`.
pub type CompactCallback = Arc<dyn Fn() + Send + Sync>;

/// Callback type for checking if the agent is idle. Injected by `uncode-cli`.
pub type IdleCheckCallback = Arc<dyn Fn() -> bool + Send + Sync>;

/// Callback type for overlay actions. Injected by `uncode-cli`.
/// Blocks until the TUI processes the action.
pub type OverlayCallback = Arc<dyn Fn(OverlayAction) -> Result<(), String> + Send + Sync>;

/// Callback type for UI actions (widget/status). Injected by `uncode-cli`.
/// Blocks until the TUI processes the action.
pub type UiCallback = Arc<dyn Fn(UiAction) -> Result<(), String> + Send + Sync>;

/// Callback type for system notifications. Injected by `uncode-cli`.
/// Direct I/O — no TUI channel needed.
pub type NotifyCallback = Arc<dyn Fn(String, NotifyType) -> Result<(), String> + Send + Sync>;

/// Callback type for message renderer registration. Injected by `uncode-cli`.
pub type MessageRendererCallback =
    Arc<dyn Fn(MessageRenderConfig) -> Result<(), String> + Send + Sync>;

/// Callback type for header replacement. Injected by `uncode-cli`.
pub type HeaderCallback = Arc<dyn Fn(Option<HeaderConfig>) -> Result<(), String> + Send + Sync>;

/// Callback type for footer replacement. Injected by `uncode-cli`.
pub type FooterCallback = Arc<dyn Fn(Option<FooterConfig>) -> Result<(), String> + Send + Sync>;

/// Callback type for working indicator replacement. Injected by `uncode-cli`.
pub type WorkingIndicatorCallback =
    Arc<dyn Fn(Option<WorkingIndicatorConfig>) -> Result<(), String> + Send + Sync>;

/// Callback type for theme switching. Injected by `uncode-cli`.
pub type ThemeCallback = Arc<dyn Fn(ThemeControlConfig) -> Result<(), String> + Send + Sync>;

/// Callback type for thinking label customization. Injected by `uncode-cli`.
pub type ThinkingLabelCallback =
    Arc<dyn Fn(ThinkingLabelConfig) -> Result<(), String> + Send + Sync>;

/// Callback type for resource path registration. Injected by `uncode-cli`.
pub type ResourcePathCallback =
    Arc<dyn Fn(crate::resource::ResourcePathConfig) -> Result<(), String> + Send + Sync>;

/// Callback type for session tree operations. Injected by `uncode-cli`.
pub type SessionCallback =
    Arc<dyn Fn(SessionAction) -> Result<SessionResponse, String> + Send + Sync>;

/// 扩展开发者的注册 API 入口。
///
/// **Pi:** 对照扩展安装/注册门面；无同名 TS 类型。
pub struct ExtensionApi {
    registry: Arc<HookRegistry>,
    event_bus: Arc<EventBus>,
    tool_callback: Option<ToolRegistrationCallback>,
    tool_unregister_callback: Option<ToolUnregisterCallback>,
    command_callback: Option<CommandRegistrationCallback>,
    command_unregister_callback: Option<CommandUnregisterCallback>,
    shortcut_callback: Option<ShortcutRegistrationCallback>,
    provider_callback: Option<ProviderRegistrationCallback>,
    provider_unregister_callback: Option<ProviderUnregisterCallback>,
    renderer_callback: Option<RendererRegistrationCallback>,
    dialog_callback: Option<DialogCallback>,
    abort_callback: Option<AbortCallback>,
    compact_callback: Option<CompactCallback>,
    idle_check_callback: Option<IdleCheckCallback>,
    overlay_callback: Option<OverlayCallback>,
    ui_callback: Option<UiCallback>,
    notify_callback: Option<NotifyCallback>,
    message_renderer_callback: Option<MessageRendererCallback>,
    header_callback: Option<HeaderCallback>,
    footer_callback: Option<FooterCallback>,
    working_indicator_callback: Option<WorkingIndicatorCallback>,
    theme_callback: Option<ThemeCallback>,
    thinking_label_callback: Option<ThinkingLabelCallback>,
    resource_path_callback: Option<ResourcePathCallback>,
    session_callback: Option<SessionCallback>,
}

impl ExtensionApi {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self {
            registry,
            event_bus: Arc::new(EventBus::new()),
            tool_callback: None,
            tool_unregister_callback: None,
            command_callback: None,
            command_unregister_callback: None,
            shortcut_callback: None,
            provider_callback: None,
            provider_unregister_callback: None,
            renderer_callback: None,
            dialog_callback: None,
            abort_callback: None,
            compact_callback: None,
            idle_check_callback: None,
            overlay_callback: None,
            ui_callback: None,
            notify_callback: None,
            message_renderer_callback: None,
            header_callback: None,
            footer_callback: None,
            working_indicator_callback: None,
            theme_callback: None,
            thinking_label_callback: None,
            resource_path_callback: None,
            session_callback: None,
        }
    }

    /// Create with all optional callbacks (called by uncode-cli).
    #[allow(clippy::too_many_arguments)]
    pub fn with_callbacks(
        registry: Arc<HookRegistry>,
        event_bus: Arc<EventBus>,
        tool_callback: Option<ToolRegistrationCallback>,
        tool_unregister_callback: Option<ToolUnregisterCallback>,
        command_callback: Option<CommandRegistrationCallback>,
        command_unregister_callback: Option<CommandUnregisterCallback>,
        shortcut_callback: Option<ShortcutRegistrationCallback>,
        provider_callback: Option<ProviderRegistrationCallback>,
        provider_unregister_callback: Option<ProviderUnregisterCallback>,
        renderer_callback: Option<RendererRegistrationCallback>,
        dialog_callback: Option<DialogCallback>,
        abort_callback: Option<AbortCallback>,
        compact_callback: Option<CompactCallback>,
        idle_check_callback: Option<IdleCheckCallback>,
        overlay_callback: Option<OverlayCallback>,
        ui_callback: Option<UiCallback>,
        notify_callback: Option<NotifyCallback>,
        message_renderer_callback: Option<MessageRendererCallback>,
        header_callback: Option<HeaderCallback>,
        footer_callback: Option<FooterCallback>,
        working_indicator_callback: Option<WorkingIndicatorCallback>,
        theme_callback: Option<ThemeCallback>,
        thinking_label_callback: Option<ThinkingLabelCallback>,
        resource_path_callback: Option<ResourcePathCallback>,
        session_callback: Option<SessionCallback>,
    ) -> Self {
        Self {
            registry,
            event_bus,
            tool_callback,
            tool_unregister_callback,
            command_callback,
            command_unregister_callback,
            shortcut_callback,
            provider_callback,
            provider_unregister_callback,
            renderer_callback,
            dialog_callback,
            abort_callback,
            compact_callback,
            idle_check_callback,
            overlay_callback,
            ui_callback,
            notify_callback,
            message_renderer_callback,
            header_callback,
            footer_callback,
            working_indicator_callback,
            theme_callback,
            thinking_label_callback,
            resource_path_callback,
            session_callback,
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

    /// Register a custom tool renderer from an extension.
    pub fn register_renderer(&self, config: ToolRenderConfig) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .renderer_callback
            .as_ref()
            .ok_or("renderer registration not available: no callback configured")?;
        callback(config)
    }

    /// Show an interactive dialog and block until the user responds.
    pub fn show_dialog(&self, request: DialogRequest) -> Result<DialogResponse, String> {
        let callback = self
            .dialog_callback
            .as_ref()
            .ok_or("dialog not available: no callback configured")?;
        callback(request)
    }

    /// Abort the current agent run.
    ///
    /// **Pi:** `ctx.abort()`.
    pub fn abort(&self) {
        if let Some(callback) = &self.abort_callback {
            callback();
        }
    }

    /// Trigger context compaction.
    ///
    /// **Pi:** `ctx.compact()`.
    pub fn compact(&self) {
        if let Some(callback) = &self.compact_callback {
            callback();
        }
    }

    /// Check if the agent is currently idle.
    ///
    /// **Pi:** `ctx.isIdle()`.
    pub fn is_idle(&self) -> bool {
        if let Some(callback) = &self.idle_check_callback {
            callback()
        } else {
            true
        }
    }

    /// Show an overlay in the TUI. Blocks until the TUI processes the action.
    pub fn show_overlay(
        &self,
        config: OverlayConfig,
        content: OverlayContent,
    ) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .overlay_callback
            .as_ref()
            .ok_or("overlay not available: no callback configured")?;
        callback(OverlayAction::Show { config, content })
    }

    /// Hide an overlay by key. Blocks until the TUI processes the action.
    pub fn hide_overlay(&self, key: &str) -> Result<(), String> {
        let callback = self
            .overlay_callback
            .as_ref()
            .ok_or("overlay not available: no callback configured")?;
        callback(OverlayAction::Hide { key: key.into() })
    }

    /// Update an overlay's content by key. Blocks until the TUI processes the action.
    pub fn update_overlay(&self, key: &str, content: OverlayContent) -> Result<(), String> {
        let callback = self
            .overlay_callback
            .as_ref()
            .ok_or("overlay not available: no callback configured")?;
        callback(OverlayAction::Update {
            key: key.into(),
            content,
        })
    }

    /// Place a widget above or below the input editor.
    pub fn set_widget(&self, config: WidgetConfig) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .ui_callback
            .as_ref()
            .ok_or("UI not available: no callback configured")?;
        callback(UiAction::SetWidget { config })
    }

    /// Remove a previously placed widget by key.
    pub fn remove_widget(&self, key: &str) -> Result<(), String> {
        let callback = self
            .ui_callback
            .as_ref()
            .ok_or("UI not available: no callback configured")?;
        callback(UiAction::RemoveWidget { key: key.into() })
    }

    /// Set or clear status text displayed in the footer.
    pub fn set_status(&self, key: &str, text: Option<String>) -> Result<(), String> {
        let callback = self
            .ui_callback
            .as_ref()
            .ok_or("UI not available: no callback configured")?;
        callback(UiAction::SetStatus {
            key: key.into(),
            text,
        })
    }

    /// Send a system notification.
    pub fn notify(&self, message: &str, notify_type: NotifyType) -> Result<(), String> {
        let callback = self
            .notify_callback
            .as_ref()
            .ok_or("notify not available: no callback configured")?;
        callback(message.into(), notify_type)
    }

    /// Register a custom message renderer by message type.
    pub fn register_message_renderer(&self, config: MessageRenderConfig) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .message_renderer_callback
            .as_ref()
            .ok_or("message renderer registration not available: no callback configured")?;
        callback(config)
    }

    /// Replace the built-in header with custom content. Pass `None` to restore defaults.
    pub fn set_header(&self, config: Option<HeaderConfig>) -> Result<(), String> {
        if let Some(ref c) = config {
            c.validate()?;
        }
        let callback = self
            .header_callback
            .as_ref()
            .ok_or("header not available: no callback configured")?;
        callback(config)
    }

    /// Replace the built-in footer with custom content. Pass `None` to restore defaults.
    pub fn set_footer(&self, config: Option<FooterConfig>) -> Result<(), String> {
        if let Some(ref c) = config {
            c.validate()?;
        }
        let callback = self
            .footer_callback
            .as_ref()
            .ok_or("footer not available: no callback configured")?;
        callback(config)
    }

    /// Replace the built-in working indicator animation. Pass `None` to restore defaults.
    pub fn set_working_indicator(
        &self,
        config: Option<WorkingIndicatorConfig>,
    ) -> Result<(), String> {
        if let Some(ref c) = config {
            c.validate()?;
        }
        let callback = self
            .working_indicator_callback
            .as_ref()
            .ok_or("working indicator not available: no callback configured")?;
        callback(config)
    }

    /// Switch the active TUI theme by name.
    pub fn set_theme(&self, config: ThemeControlConfig) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .theme_callback
            .as_ref()
            .ok_or("theme not available: no callback configured")?;
        callback(config)
    }

    /// Customize thinking level labels.
    pub fn set_thinking_labels(&self, config: ThinkingLabelConfig) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .thinking_label_callback
            .as_ref()
            .ok_or("thinking labels not available: no callback configured")?;
        callback(config)
    }

    /// Register an additional resource path that the agent's file tools can access.
    pub fn register_resource_path(&self, config: ResourcePathConfig) -> Result<(), String> {
        config.validate()?;
        let callback = self
            .resource_path_callback
            .as_ref()
            .ok_or("resource path registration not available: no callback configured")?;
        callback(config)
    }

    /// Fork a new session branch from the specified entry.
    ///
    /// **Pi:** `ctx.fork()`.
    pub fn session_fork(&self, entry_id: &str) -> Result<String, String> {
        let action = SessionAction::Fork {
            entry_id: entry_id.into(),
        };
        action.validate()?;
        let callback = self
            .session_callback
            .as_ref()
            .ok_or("session operations not available: no callback configured")?;
        match callback(action)? {
            SessionResponse::Forked { session_id } => Ok(session_id),
            SessionResponse::Ok => Err("unexpected response for fork".into()),
        }
    }

    /// Navigate the session tree to the specified entry.
    ///
    /// **Pi:** `ctx.navigateTree()`.
    pub fn session_navigate(&self, entry_id: &str) -> Result<(), String> {
        let action = SessionAction::Navigate {
            entry_id: entry_id.into(),
        };
        action.validate()?;
        let callback = self
            .session_callback
            .as_ref()
            .ok_or("session operations not available: no callback configured")?;
        match callback(action)? {
            SessionResponse::Ok => Ok(()),
            other => Err(format!("unexpected response for navigate: {other:?}")),
        }
    }

    /// Switch to a different session entirely.
    ///
    /// **Pi:** `ctx.switchSession()`.
    pub fn session_switch(&self, session_id: &str) -> Result<(), String> {
        let action = SessionAction::Switch {
            session_id: session_id.into(),
        };
        action.validate()?;
        let callback = self
            .session_callback
            .as_ref()
            .ok_or("session operations not available: no callback configured")?;
        match callback(action)? {
            SessionResponse::Ok => Ok(()),
            other => Err(format!("unexpected response for switch: {other:?}")),
        }
    }

    /// Set the display name of the current session.
    pub fn session_set_name(&self, name: &str) -> Result<(), String> {
        let action = SessionAction::SetName { name: name.into() };
        action.validate()?;
        let callback = self
            .session_callback
            .as_ref()
            .ok_or("session operations not available: no callback configured")?;
        match callback(action)? {
            SessionResponse::Ok => Ok(()),
            other => Err(format!("unexpected response for set_name: {other:?}")),
        }
    }

    // ── 跨扩展事件总线 (#393) ──

    /// 发布自定义事件到指定 channel。
    ///
    /// **Pi:** `pi.events.emit(channel, data)`。
    pub fn emit_event(&self, channel: &str, data: serde_json::Value) {
        self.event_bus.emit(channel, data);
    }

    /// 订阅指定 channel 的自定义事件。
    ///
    /// **Pi:** `pi.events.on(channel, handler)`。
    /// 返回的 `SubscriptionId` 可用于 `unsubscribe_event()` 取消订阅。
    pub fn subscribe_event(&self, channel: &str, handler: EventHandler) -> SubscriptionId {
        self.event_bus.subscribe(channel, handler)
    }

    /// 取消事件订阅。
    pub fn unsubscribe_event(&self, id: SubscriptionId) -> bool {
        self.event_bus.unsubscribe(id)
    }
}
