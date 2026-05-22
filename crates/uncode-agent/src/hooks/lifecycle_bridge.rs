use std::sync::Arc;

use uncode_core::message::Message;
use uncode_extensions::hooks::{HookContext, HookEvent, HookRegistry, HookResult, LifecycleHook};

/// Fires extension lifecycle hooks at session/turn/message boundaries.
///
/// Held by `AgentLoop` and called at the appropriate lifecycle points.
pub struct ExtensionLifecycleBridge {
    registry: Arc<HookRegistry>,
}

impl ExtensionLifecycleBridge {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self { registry }
    }

    pub fn registry(&self) -> &Arc<HookRegistry> {
        &self.registry
    }

    pub async fn fire_session_start(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::SessionStart, &ctx).await
    }

    pub async fn fire_session_end(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::SessionEnd, &ctx).await
    }

    pub async fn fire_turn_start(&self, session_id: &str, _turn: u64) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::TurnStart, &ctx).await
    }

    pub async fn fire_turn_end(&self, session_id: &str, _turn: u64) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::TurnEnd, &ctx).await
    }

    pub async fn fire_message_received(&self, session_id: &str, msg: &Message) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::Message(msg.clone()),
        };
        self.registry
            .fire(LifecycleHook::MessageReceived, &ctx)
            .await
    }

    pub async fn fire_message_sending(&self, session_id: &str, msg: &Message) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::Message(msg.clone()),
        };
        self.registry
            .fire(LifecycleHook::MessageSending, &ctx)
            .await
    }
}
