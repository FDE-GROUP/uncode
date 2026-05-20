use std::sync::Arc;

use async_trait::async_trait;
use uncode_core::tool::{
    AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, BeforeToolCallResult,
    ToolHooks, ToolResult,
};

/// Run multiple [`ToolHooks`] in order; first `before_tool_call` block wins.
pub struct ChainedToolHooks {
    hooks: Vec<Arc<dyn ToolHooks>>,
}

impl ChainedToolHooks {
    pub fn new(hooks: Vec<Arc<dyn ToolHooks>>) -> Self {
        Self { hooks }
    }
}

#[async_trait]
impl ToolHooks for ChainedToolHooks {
    async fn before_tool_call(&self, ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        for hook in &self.hooks {
            if let Some(reason) = hook.before_tool_call(ctx).await {
                return Some(reason);
            }
        }
        None
    }

    async fn after_tool_call(
        &self,
        ctx: &AfterToolCallContext,
        result: &mut ToolResult,
    ) -> AfterToolCallResult {
        let mut merged = AfterToolCallResult::default();
        for hook in &self.hooks {
            let patch = hook.after_tool_call(ctx, result).await;
            if patch.content.is_some() {
                merged.content = patch.content;
            }
            if patch.details.is_some() {
                merged.details = patch.details;
            }
            if patch.is_error.is_some() {
                merged.is_error = patch.is_error;
            }
            if patch.terminate.is_some() {
                merged.terminate = patch.terminate;
            }
        }
        merged
    }
}

#[cfg(test)]
struct BlockFirst;

#[cfg(test)]
#[async_trait]
impl ToolHooks for BlockFirst {
    async fn before_tool_call(&self, ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        if ctx.tool_name == "write" {
            Some("blocked by test hook".into())
        } else {
            None
        }
    }

    async fn after_tool_call(
        &self,
        _ctx: &AfterToolCallContext,
        _result: &mut ToolResult,
    ) -> AfterToolCallResult {
        AfterToolCallResult::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chained_before_first_block_wins() {
        let chain = ChainedToolHooks::new(vec![
            Arc::new(BlockFirst),
            Arc::new(crate::hooks::permission::PermissionToolHooks::new(
                Arc::new(crate::permission_gate::PermissionGate::new_without_events()),
            )),
        ]);
        let ctx = BeforeToolCallContext {
            tool_call_id: "t1".into(),
            tool_name: "write".into(),
            args: serde_json::json!({}),
        };
        let reason = chain.before_tool_call(&ctx).await;
        assert_eq!(reason.as_deref(), Some("blocked by test hook"));
    }
}
