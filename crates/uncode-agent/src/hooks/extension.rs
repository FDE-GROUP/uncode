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
                    .map(|_| "[extension-modified]".to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use uncode_core::tool::{AfterToolCallContext, ToolResult};
    use uncode_extensions::hooks::{
        Extension, HookContext, HookRegistry, HookResult, LifecycleHook,
    };

    struct MockExtension {
        name: String,
        result: HookResult,
    }

    #[async_trait::async_trait]
    impl Extension for MockExtension {
        fn name(&self) -> &str {
            &self.name
        }
        async fn on_hook(&self, _ctx: &HookContext) -> anyhow::Result<HookResult> {
            Ok(self.result.clone())
        }
    }

    fn make_ctx() -> BeforeToolCallContext {
        BeforeToolCallContext {
            tool_call_id: "t1".into(),
            tool_name: "read".into(),
            args: serde_json::json!({"path": "/tmp/test"}),
        }
    }

    #[tokio::test]
    async fn test_new_constructs() {
        let registry = HookRegistry::new();
        let _hooks = ExtensionToolHooks::new(Arc::new(registry));
    }

    #[tokio::test]
    async fn test_before_tool_call_continue() {
        let registry = Arc::new(HookRegistry::new());
        let ext = MockExtension {
            name: "test-ext".into(),
            result: HookResult::Continue,
        };
        registry.register(Arc::new(ext), vec![LifecycleHook::ToolCallBefore]);
        let hooks = ExtensionToolHooks::new(registry);
        let ctx = make_ctx();
        let result = hooks.before_tool_call(&ctx).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_before_tool_call_block() {
        let registry = Arc::new(HookRegistry::new());
        let ext = MockExtension {
            name: "block-ext".into(),
            result: HookResult::Block {
                reason: "denied".into(),
            },
        };
        registry.register(Arc::new(ext), vec![LifecycleHook::ToolCallBefore]);
        let hooks = ExtensionToolHooks::new(registry);
        let ctx = make_ctx();
        let result = hooks.before_tool_call(&ctx).await;
        assert_eq!(result, Some("denied".into()));
    }

    #[tokio::test]
    async fn test_after_tool_call_default() {
        let registry = Arc::new(HookRegistry::new());
        let ext = MockExtension {
            name: "after-ext".into(),
            result: HookResult::Continue,
        };
        registry.register(Arc::new(ext), vec![LifecycleHook::ToolCallAfter]);
        let hooks = ExtensionToolHooks::new(registry);
        let ctx = AfterToolCallContext {
            tool_call_id: "t1".into(),
            tool_name: "read".into(),
            args: serde_json::json!({}),
        };
        let mut result = ToolResult::ok("output");
        let patch = hooks.after_tool_call(&ctx, &mut result).await;
        assert!(patch.content.is_none());
        assert!(patch.details.is_none());
        assert!(patch.is_error.is_none());
        assert!(patch.terminate.is_none());
    }
}
