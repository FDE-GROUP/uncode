use std::sync::Arc;

use async_trait::async_trait;
use uncode_core::tool::{
    AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, BeforeToolCallResult,
    ToolHooks,
};
use uncode_extensions::hooks::{HookContext, HookEvent, HookRegistry, HookResult, LifecycleHook};

/// Adapter: bridges extension [`HookRegistry`] into the [`ToolHooks`] infrastructure.
///
/// Tool call interception (block/modify) flows through the existing `ChainedToolHooks`
/// pipeline alongside `PermissionToolHooks`.
pub struct ExtensionToolHooks {
    registry: Arc<HookRegistry>,
}

impl ExtensionToolHooks {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ToolHooks for ExtensionToolHooks {
    async fn before_tool_call(&self, _ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        let hook_ctx = HookContext {
            session_id: None,
            event: HookEvent::None,
        };
        match self
            .registry
            .fire(LifecycleHook::ToolCallBefore, &hook_ctx)
            .await
        {
            HookResult::Continue => None,
            HookResult::Modify(_) => None,
            HookResult::Block { reason } => Some(reason),
        }
    }

    async fn after_tool_call(
        &self,
        _ctx: &AfterToolCallContext,
        _result: &mut uncode_core::tool::ToolResult,
    ) -> AfterToolCallResult {
        let hook_ctx = HookContext {
            session_id: None,
            event: HookEvent::None,
        };
        match self
            .registry
            .fire(LifecycleHook::ToolCallAfter, &hook_ctx)
            .await
        {
            HookResult::Continue => AfterToolCallResult::default(),
            HookResult::Modify(modification) => AfterToolCallResult {
                content: modification.content_override,
                details: modification.details_override,
                is_error: modification.is_error_override,
                terminate: modification.terminate_override,
            },
            HookResult::Block { .. } => AfterToolCallResult::default(),
        }
    }
}
