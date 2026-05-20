//! Async permission gate — blocks tool execution until TUI user confirms.
//!
//! Wired via [`PermissionToolHooks`](crate::hooks::PermissionToolHooks) and TUI `resolve()`.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::{Mutex, broadcast, oneshot};

use uncode_core::event::AgentEvent;
use uncode_core::tool::BeforeToolCallContext;

use crate::tool_permission;

/// User decision on a pending tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Allow,
    Deny,
}

/// Shared gate between `AgentLoop` hooks and TUI confirmation UI.
pub struct PermissionGate {
    auto_allow_readonly: bool,
    auto_allow_bash_safe: bool,
    waiters: Mutex<HashMap<String, oneshot::Sender<Approval>>>,
    early: Mutex<HashMap<String, Approval>>,
    event_tx: Option<broadcast::Sender<AgentEvent>>,
}

impl PermissionGate {
    pub fn new(event_tx: broadcast::Sender<AgentEvent>) -> Self {
        Self {
            auto_allow_readonly: true,
            auto_allow_bash_safe: true,
            waiters: Mutex::new(HashMap::new()),
            early: Mutex::new(HashMap::new()),
            event_tx: Some(event_tx),
        }
    }

    /// For tests: no UI events.
    pub fn new_without_events() -> Self {
        Self {
            auto_allow_readonly: true,
            auto_allow_bash_safe: true,
            waiters: Mutex::new(HashMap::new()),
            early: Mutex::new(HashMap::new()),
            event_tx: None,
        }
    }

    pub fn needs_confirmation(&self, tool_name: &str, arguments: &str) -> bool {
        tool_permission::needs_confirmation(
            tool_name,
            arguments,
            self.auto_allow_readonly,
            self.auto_allow_bash_safe,
        )
    }

    /// Called from TUI when user allows or denies (may arrive before or after the waiter registers).
    pub async fn resolve(&self, tool_call_id: &str, approval: Approval) {
        if let Some(tx) = self.waiters.lock().await.remove(tool_call_id) {
            let _ = tx.send(approval);
            return;
        }
        self.early
            .lock()
            .await
            .insert(tool_call_id.to_string(), approval);
    }

    /// Block until allowed or denied; emits [`AgentEvent::ToolCallAwaitingApproval`] when waiting.
    pub async fn wait_for_approval(&self, ctx: &BeforeToolCallContext) -> Option<String> {
        let args_str = serde_json::to_string(&ctx.args).unwrap_or_default();
        if !self.needs_confirmation(&ctx.tool_name, &args_str) {
            return None;
        }

        if let Some(approval) = self.early.lock().await.remove(&ctx.tool_call_id) {
            return block_reason(approval);
        }

        let (tx, rx) = oneshot::channel();
        self.waiters
            .lock()
            .await
            .insert(ctx.tool_call_id.clone(), tx);

        if let Some(ref event_tx) = self.event_tx {
            let _ = event_tx.send(AgentEvent::ToolCallAwaitingApproval {
                tool_id: ctx.tool_call_id.clone(),
                tool_name: ctx.tool_name.clone(),
                arguments_summary: args_str,
            });
        }

        match tokio::time::timeout(Duration::from_secs(600), rx).await {
            Ok(Ok(approval)) => block_reason(approval),
            Ok(Err(_)) => Some("confirmation channel closed".into()),
            Err(_) => Some("confirmation timed out".into()),
        }
    }
}

fn block_reason(approval: Approval) -> Option<String> {
    match approval {
        Approval::Allow => None,
        Approval::Deny => Some("denied by user".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_early_allow_before_wait() {
        let gate = Arc::new(PermissionGate::new_without_events());
        gate.resolve("tc1", Approval::Allow).await;

        let ctx = BeforeToolCallContext {
            tool_call_id: "tc1".into(),
            tool_name: "write".into(),
            args: json!({"path": "a.rs", "content": "x"}),
        };
        assert!(gate.wait_for_approval(&ctx).await.is_none());
    }

    #[tokio::test]
    async fn test_wait_then_deny() {
        let gate = Arc::new(PermissionGate::new_without_events());
        let g = gate.clone();
        let ctx = BeforeToolCallContext {
            tool_call_id: "tc2".into(),
            tool_name: "write".into(),
            args: json!({"path": "b.rs"}),
        };
        let wait = tokio::spawn(async move { g.wait_for_approval(&ctx).await });
        tokio::task::yield_now().await;
        gate.resolve("tc2", Approval::Deny).await;
        let reason = wait.await.unwrap();
        assert_eq!(reason.as_deref(), Some("denied by user"));
    }
}
