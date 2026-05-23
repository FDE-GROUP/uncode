use dashmap::DashMap;
use std::sync::Arc;

use uncode_core::event::AgentEvent;
use uncode_core::message::Message;

/// Hook result: extensions can allow continuation, modify data, or block.
///
/// **Pi:** 对照 `ToolCallEventResult { block, reason }` 和 `ToolResultEventResult { content, details, isError }`。
#[derive(Debug, Clone)]
pub enum HookResult {
    /// Allow the operation to proceed.
    Continue,
    /// Allow but modify data in transit.
    Modify(HookModification),
    /// Block the operation with a reason.
    Block { reason: String },
}

impl Default for HookResult {
    fn default() -> Self {
        Self::Continue
    }
}

/// Modification payload — what an extension wants to change.
///
/// **Pi:** 对照 `ContextEventResult { messages }`、`ToolResultEventResult { content, details, isError }` 等。
#[derive(Debug, Clone, Default)]
pub struct HookModification {
    /// For ToolCallBefore: replace tool arguments.
    pub args_override: Option<serde_json::Value>,
    /// For ToolCallAfter: replace result content.
    pub content_override: Option<Vec<uncode_core::tool::ToolContent>>,
    /// For ToolCallAfter: replace result details.
    pub details_override: Option<serde_json::Value>,
    /// For ToolCallAfter: override error status.
    pub is_error_override: Option<bool>,
    /// For ToolCallAfter: override terminate flag.
    pub terminate_override: Option<bool>,
}

/// Agent 生命周期钩子（扩展注入点）。
///
/// **Pi:** 概念上接近 Pi Harness 扩展点；uncode 以 WASM 扩展 + 本枚举分发。
/// **OpenCode:** 无 1:1 钩子名；对照插件/Hook 产品能力即可。
#[derive(Clone)]
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

/// 钩子上下文 — 传递给扩展的数据。
///
/// **Pi:** 对照扩展 hook 回调入参；字段集为 uncode 自有。
#[derive(Debug, Clone)]
pub struct HookContext {
    pub session_id: Option<String>,
    pub event: HookEvent,
}

/// 钩子载荷：Agent 事件或消息快照。
///
/// **Pi:** 无同名枚举；概念上包装 Pi 侧 extension 可见的事件子集。
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

    /// Handle a lifecycle hook. Return `Continue` to pass, `Modify` to alter data,
    /// or `Block` to stop the operation. Default: observe-only (`Continue`).
    async fn on_hook(&self, ctx: &HookContext) -> anyhow::Result<HookResult> {
        let _ = ctx;
        Ok(HookResult::Continue)
    }
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

    /// Unregister an extension by name.
    ///
    /// Removes the extension and cleans up all its hook subscriptions.
    /// Empty hook entries are removed. Returns `true` if the extension existed.
    pub fn unregister(&self, name: &str) -> bool {
        let removed = self.extensions.remove(name).is_some();
        if removed {
            // Collect hook keys, then update each entry.
            let hook_keys: Vec<String> = self.hooks.iter().map(|e| e.key().clone()).collect();
            for key in hook_keys {
                if let Some(mut entry) = self.hooks.get_mut(&key) {
                    entry.retain(|n| n != name);
                }
            }
            // Remove empty hook entries.
            let empty_keys: Vec<String> = self
                .hooks
                .iter()
                .filter(|e| e.value().is_empty())
                .map(|e| e.key().clone())
                .collect();
            for key in empty_keys {
                self.hooks.remove(&key);
            }
        }
        removed
    }

    /// Fire a lifecycle hook to all registered extensions.
    ///
    /// Semantics: first `Block` or `Modify` wins; errors are logged as `Continue`.
    pub async fn fire(&self, hook: LifecycleHook, ctx: &HookContext) -> HookResult {
        let hook_name = hook.name();
        if let Some(ext_names) = self.hooks.get(hook_name) {
            for ext_name in ext_names.value() {
                if let Some(ext) = self.extensions.get(ext_name).as_deref() {
                    match ext.on_hook(ctx).await {
                        Ok(HookResult::Continue) => {}
                        Ok(result @ HookResult::Block { .. }) => {
                            tracing::warn!(
                                "extension {} blocked hook {}: {:?}",
                                ext.name(),
                                hook_name,
                                result
                            );
                            return result;
                        }
                        Ok(result @ HookResult::Modify(_)) => {
                            tracing::debug!("extension {} modified hook {}", ext.name(), hook_name,);
                            return result;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "extension {} hook {} failed: {e}",
                                ext.name(),
                                hook_name
                            );
                        }
                    }
                }
            }
        }
        HookResult::Continue
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
