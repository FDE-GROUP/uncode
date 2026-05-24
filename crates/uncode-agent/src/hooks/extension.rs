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
    async fn before_tool_call(&self, ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        let hook_ctx = HookContext {
            session_id: None,
            event: HookEvent::ToolCallBefore {
                tool_name: ctx.tool_name.clone(),
                args: ctx.args.clone(),
            },
        };
        match self
            .registry
            .fire(LifecycleHook::ToolCallBefore, &hook_ctx)
            .await
        {
            HookResult::Continue => None,
            HookResult::Modify(modification) => {
                // Return the args override as the reason string to signal modification.
                // The caller (PermissionToolHooks chain) interprets Some(reason) as a block.
                // For proper args modification, the caller needs to check modification.args_override.
                modification
                    .args_override
                    .map(|_| "[extension-modified]".to_string())
            }
            HookResult::Block { reason } => Some(reason),
        }
    }

    async fn after_tool_call(
        &self,
        ctx: &AfterToolCallContext,
        _result: &mut uncode_core::tool::ToolResult,
    ) -> AfterToolCallResult {
        let hook_ctx = HookContext {
            session_id: None,
            event: HookEvent::ToolCallAfter {
                tool_name: ctx.tool_name.clone(),
                is_error: false,
            },
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
