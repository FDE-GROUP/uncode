use dashmap::DashMap;
use std::sync::Arc;

use uncode_core::event::AgentEvent;
use uncode_core::message::Message;

/// Agent 生命周期钩子（扩展注入点）。
///
/// **Pi:** 概念上接近 Pi Harness 扩展点；uncode 以 WASM 扩展 + 本枚举分发。
/// **OpenCode:** 无 1:1 钩子名；对照插件/Hook 产品能力即可。
pub enum LifecycleHook {
    SessionStart,
    TurnStart,
    MessageReceived,
    MessageSending,
    ToolCallBefore,
    ToolCallAfter,
    TurnEnd,
    SessionEnd,
}

impl LifecycleHook {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::TurnStart => "turn_start",
            Self::MessageReceived => "message_received",
            Self::MessageSending => "message_sending",
            Self::ToolCallBefore => "tool_call_before",
            Self::ToolCallAfter => "tool_call_after",
            Self::TurnEnd => "turn_end",
            Self::SessionEnd => "session_end",
        }
    }
}

/// 钩子上下文——传递给扩展的数据
#[derive(Debug, Clone)]
pub struct HookContext {
    pub session_id: Option<String>,
    pub event: HookEvent,
}

#[derive(Debug, Clone)]
pub enum HookEvent {
    Event(AgentEvent),
    Message(Message),
    None,
}

/// 扩展 trait — 所有 WASM/内置扩展必须实现。
///
/// **Pi:** 对照 Pi 扩展包生命周期；实现细节不同。
#[async_trait::async_trait]
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;
    async fn on_hook(&self, ctx: &HookContext) -> anyhow::Result<()>;
}

/// 钩子注册表 — 管理扩展实例与 hook 名称映射。
///
/// **Pi:** 无同名类型；对应「按 hook 名调度扩展」的注册中心。
pub struct HookRegistry {
    extensions: DashMap<String, Arc<dyn Extension>>,
    hooks: DashMap<String, Vec<String>>, // hook_name → extension_names
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            extensions: DashMap::new(),
            hooks: DashMap::new(),
        }
    }

    pub fn register(&self, ext: Arc<dyn Extension>, hooks: Vec<LifecycleHook>) {
        let name = ext.name().to_string();
        for hook in hooks {
            self.hooks
                .entry(hook.name().to_string())
                .or_default()
                .push(name.clone());
        }
        self.extensions.insert(name, ext);
    }

    pub async fn fire(&self, hook: LifecycleHook, ctx: &HookContext) {
        let hook_name = hook.name();
        if let Some(ext_names) = self.hooks.get(hook_name) {
            for ext_name in ext_names.value() {
                if let Some(ext) = self.extensions.get(ext_name).as_deref()
                    && let Err(e) = ext.on_hook(ctx).await
                {
                    tracing::warn!("extension {} hook {} failed: {e}", ext.name(), hook_name);
                }
            }
        }
    }

    /// Number of registered extensions (for testing)
    #[cfg(test)]
    pub(crate) fn extension_count(&self) -> usize {
        self.extensions.len()
    }

    /// Number of hook registrations (for testing)
    #[cfg(test)]
    pub(crate) fn hook_count(&self) -> usize {
        self.hooks.len()
    }

    /// Check if a hook has registrations (for testing)
    #[cfg(test)]
    pub(crate) fn has_hook(&self, hook_name: &str) -> bool {
        self.hooks.contains_key(hook_name)
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
