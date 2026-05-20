use std::sync::Arc;

use async_trait::async_trait;
use uncode_core::tool::{
    AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, BeforeToolCallResult,
    ToolHooks, ToolResult,
};

use crate::permission_gate::PermissionGate;

/// Blocks dangerous tool calls until the TUI user confirms via [`PermissionGate::resolve`].
pub struct PermissionToolHooks {
    gate: Arc<PermissionGate>,
}

impl PermissionToolHooks {
    pub fn new(gate: Arc<PermissionGate>) -> Self {
        Self { gate }
    }
}

#[async_trait]
impl ToolHooks for PermissionToolHooks {
    async fn before_tool_call(&self, ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        self.gate.wait_for_approval(ctx).await
    }

    async fn after_tool_call(
        &self,
        _ctx: &AfterToolCallContext,
        _result: &mut ToolResult,
    ) -> AfterToolCallResult {
        AfterToolCallResult::default()
    }
}
