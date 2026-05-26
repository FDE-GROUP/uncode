use std::sync::Arc;

use uncode_core::message::Message;
use uncode_extensions::api::ExtensionApi;
use uncode_extensions::hooks::{HookContext, HookEvent, HookResult, LifecycleHook};

/// Fires extension lifecycle hooks at session/turn/message boundaries.
///
/// Held by `AgentLoop` and called at the appropriate lifecycle points.
pub struct ExtensionLifecycleBridge {
    registry: Arc<uncode_extensions::hooks::HookRegistry>,
    api: Arc<ExtensionApi>,
}

impl ExtensionLifecycleBridge {
    pub fn new(api: ExtensionApi) -> Self {
        let registry = api.registry().clone();
        Self {
            registry,
            api: Arc::new(api),
        }
    }

    /// Construct from an `Arc<ExtensionApi>`.
    pub fn from_arc(api: Arc<ExtensionApi>) -> Self {
        let registry = api.registry().clone();
        Self { registry, api }
    }
}

impl From<Arc<ExtensionApi>> for ExtensionLifecycleBridge {
    fn from(api: Arc<ExtensionApi>) -> Self {
        Self::from_arc(api)
    }
}

impl ExtensionLifecycleBridge {
    /// Access the ExtensionApi for loading extensions.
    pub fn api(&self) -> &ExtensionApi {
        &self.api
    }

    pub fn registry(&self) -> &Arc<uncode_extensions::hooks::HookRegistry> {
        &self.registry
    }

    // ── Session lifecycle ──

    pub async fn fire_session_start(&self, session_id: &str, reason: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::SessionStart {
                reason: reason.to_string(),
            },
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

    pub async fn fire_session_shutdown(&self, session_id: &str, reason: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::SessionShutdown {
                reason: reason.to_string(),
            },
        };
        self.registry
            .fire(LifecycleHook::SessionShutdown, &ctx)
            .await
    }

    // ── Turn lifecycle ──

    pub async fn fire_turn_start(&self, session_id: &str, turn_index: u64) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::TurnStart {
                turn_index,
                timestamp: chrono::Utc::now().timestamp(),
            },
        };
        self.registry.fire(LifecycleHook::TurnStart, &ctx).await
    }

    pub async fn fire_turn_end(&self, session_id: &str, turn_index: u64) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::TurnEnd { turn_index },
        };
        self.registry.fire(LifecycleHook::TurnEnd, &ctx).await
    }

    // ── Message lifecycle ──

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

    // ── Session management ──

    pub async fn fire_session_before_compact(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::SessionBeforeCompact,
        };
        self.registry
            .fire(LifecycleHook::SessionBeforeCompact, &ctx)
            .await
    }

    pub async fn fire_session_compact(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry
            .fire(LifecycleHook::SessionCompact, &ctx)
            .await
    }

    // ── Agent lifecycle ──

    pub async fn fire_before_agent_start(&self, session_id: &str, prompt: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::BeforeAgentStart {
                prompt: prompt.to_string(),
            },
        };
        self.registry
            .fire(LifecycleHook::BeforeAgentStart, &ctx)
            .await
    }

    pub async fn fire_agent_start(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::AgentStart, &ctx).await
    }

    pub async fn fire_agent_end(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::AgentEnd,
        };
        self.registry.fire(LifecycleHook::AgentEnd, &ctx).await
    }

    // ── LLM interaction ──

    pub async fn fire_context(&self, session_id: &str, messages: &[Message]) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ContextSnapshot(messages.to_vec()),
        };
        self.registry.fire(LifecycleHook::Context, &ctx).await
    }

    pub async fn fire_before_provider_request(
        &self,
        session_id: &str,
        payload: &serde_json::Value,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ProviderRequest(payload.clone()),
        };
        self.registry
            .fire(LifecycleHook::BeforeProviderRequest, &ctx)
            .await
    }

    pub async fn fire_after_provider_response(&self, session_id: &str, status: u16) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::AfterProviderResponse { status },
        };
        self.registry
            .fire(LifecycleHook::AfterProviderResponse, &ctx)
            .await
    }

    // ── Streaming ──

    pub async fn fire_message_update(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::MessageUpdate, &ctx).await
    }

    pub async fn fire_message_start(
        &self,
        session_id: &str,
        role: &str,
        message_id: &str,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::MessageStart {
                role: role.to_string(),
                message_id: message_id.to_string(),
            },
        };
        self.registry.fire(LifecycleHook::MessageStart, &ctx).await
    }

    pub async fn fire_message_end(
        &self,
        session_id: &str,
        role: &str,
        message_id: &str,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::MessageEnd {
                role: role.to_string(),
                message_id: message_id.to_string(),
            },
        };
        self.registry.fire(LifecycleHook::MessageEnd, &ctx).await
    }

    // ── Model events ──

    pub async fn fire_model_select(
        &self,
        session_id: &str,
        model: &str,
        previous_model: Option<&str>,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ModelSelect {
                model: model.to_string(),
                previous_model: previous_model.map(|s| s.to_string()),
            },
        };
        self.registry.fire(LifecycleHook::ModelSelect, &ctx).await
    }

    // ── Tool execution ──

    pub async fn fire_tool_call_before(
        &self,
        session_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ToolCallBefore {
                tool_name: tool_name.to_string(),
                args: args.clone(),
            },
        };
        self.registry
            .fire(LifecycleHook::ToolCallBefore, &ctx)
            .await
    }

    pub async fn fire_tool_call_after(
        &self,
        session_id: &str,
        tool_name: &str,
        is_error: bool,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ToolCallAfter {
                tool_name: tool_name.to_string(),
                is_error,
            },
        };
        self.registry.fire(LifecycleHook::ToolCallAfter, &ctx).await
    }

    pub async fn fire_tool_execution_start(
        &self,
        session_id: &str,
        tool_call_id: &str,
        tool_name: &str,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ToolExecutionStart {
                tool_call_id: tool_call_id.to_string(),
                tool_name: tool_name.to_string(),
            },
        };
        self.registry
            .fire(LifecycleHook::ToolExecutionStart, &ctx)
            .await
    }

    pub async fn fire_tool_execution_update(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry
            .fire(LifecycleHook::ToolExecutionUpdate, &ctx)
            .await
    }

    pub async fn fire_tool_execution_end(
        &self,
        session_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        is_error: bool,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ToolExecutionEnd {
                tool_call_id: tool_call_id.to_string(),
                tool_name: tool_name.to_string(),
                is_error,
            },
        };
        self.registry
            .fire(LifecycleHook::ToolExecutionEnd, &ctx)
            .await
    }

    // ── Resource discovery ──

    pub async fn fire_resources_discover(&self, session_id: &str, cwd: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ResourcesDiscover {
                cwd: cwd.to_string(),
            },
        };
        self.registry
            .fire(LifecycleHook::ResourcesDiscover, &ctx)
            .await
    }

    // ── Session lifecycle interception (#395) ──

    pub async fn fire_session_before_switch(
        &self,
        session_id: &str,
        target_session_id: &str,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::SessionSwitch {
                session_id: target_session_id.to_string(),
            },
        };
        self.registry
            .fire(LifecycleHook::SessionBeforeSwitch, &ctx)
            .await
    }

    pub async fn fire_session_before_fork(&self, session_id: &str, entry_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::SessionFork {
                entry_id: entry_id.to_string(),
            },
        };
        self.registry
            .fire(LifecycleHook::SessionBeforeFork, &ctx)
            .await
    }

    pub async fn fire_session_before_tree(&self, session_id: &str, entry_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::SessionTreeNav {
                entry_id: entry_id.to_string(),
            },
        };
        self.registry
            .fire(LifecycleHook::SessionBeforeTree, &ctx)
            .await
    }

    pub async fn fire_session_tree(
        &self,
        session_id: &str,
        new_leaf_id: &str,
        old_leaf_id: &str,
        summary: Option<&str>,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::SessionTreeResult {
                new_leaf_id: new_leaf_id.to_string(),
                old_leaf_id: old_leaf_id.to_string(),
                summary: summary.map(|s| s.to_string()),
            },
        };
        self.registry.fire(LifecycleHook::SessionTree, &ctx).await
    }

    // ── User input interception (#396) ──

    pub async fn fire_input(
        &self,
        session_id: &str,
        source: uncode_extensions::hooks::InputSource,
        text: &str,
        images: &[String],
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::Input {
                source,
                text: text.to_string(),
                images: images.to_vec(),
            },
        };
        self.registry.fire(LifecycleHook::Input, &ctx).await
    }

    pub async fn fire_thinking_level_select(
        &self,
        session_id: &str,
        level: &str,
        previous_level: Option<&str>,
    ) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::ThinkingLevelSelect {
                level: level.to_string(),
                previous_level: previous_level.map(|s| s.to_string()),
            },
        };
        self.registry
            .fire(LifecycleHook::ThinkingLevelSelect, &ctx)
            .await
    }

    // ── User bash (#411) ──

    pub async fn fire_user_bash(&self, session_id: &str, command: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::UserBash {
                command: command.to_string(),
            },
        };
        self.registry.fire(LifecycleHook::UserBash, &ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use uncode_core::message::Message;
    use uncode_extensions::hooks::HookRegistry;

    fn make_bridge() -> ExtensionLifecycleBridge {
        let registry = Arc::new(HookRegistry::new());
        let api = ExtensionApi::new(registry);
        ExtensionLifecycleBridge::new(api)
    }

    // ── Construction tests ──

    #[tokio::test]
    async fn test_new_constructs() {
        let _bridge = make_bridge();
    }

    #[tokio::test]
    async fn test_from_arc() {
        let api = Arc::new(ExtensionApi::new(Arc::new(HookRegistry::new())));
        let bridge = ExtensionLifecycleBridge::from_arc(api);
        let _api: &ExtensionApi = bridge.api();
    }

    #[tokio::test]
    async fn test_registry_accessor() {
        let bridge = make_bridge();
        let _registry: &Arc<HookRegistry> = bridge.registry();
    }

    // ── Session lifecycle ──

    #[tokio::test]
    async fn test_fire_session_start() {
        let bridge = make_bridge();
        let result = bridge.fire_session_start("s1", "new conversation").await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_session_end() {
        let bridge = make_bridge();
        let result = bridge.fire_session_end("s1").await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_session_shutdown() {
        let bridge = make_bridge();
        let result = bridge.fire_session_shutdown("s1", "shutting down").await;
        assert!(matches!(result, HookResult::Continue));
    }

    // ── Turn lifecycle ──

    #[tokio::test]
    async fn test_fire_turn_start() {
        let bridge = make_bridge();
        let result = bridge.fire_turn_start("s1", 1).await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_turn_end() {
        let bridge = make_bridge();
        let result = bridge.fire_turn_end("s1", 1).await;
        assert!(matches!(result, HookResult::Continue));
    }

    // ── Message lifecycle ──

    #[tokio::test]
    async fn test_fire_message_received() {
        let bridge = make_bridge();
        let result = bridge
            .fire_message_received("s1", &Message::user("hi"))
            .await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_message_sending() {
        let bridge = make_bridge();
        let result = bridge
            .fire_message_sending("s1", &Message::user("hi"))
            .await;
        assert!(matches!(result, HookResult::Continue));
    }

    // ── Agent lifecycle ──

    #[tokio::test]
    async fn test_fire_before_agent_start() {
        let bridge = make_bridge();
        let result = bridge.fire_before_agent_start("s1", "hello").await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_agent_start() {
        let bridge = make_bridge();
        let result = bridge.fire_agent_start("s1").await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_agent_end() {
        let bridge = make_bridge();
        let result = bridge.fire_agent_end("s1").await;
        assert!(matches!(result, HookResult::Continue));
    }

    // ── Provider/Model ──

    #[tokio::test]
    async fn test_fire_before_provider_request() {
        let bridge = make_bridge();
        let result = bridge
            .fire_before_provider_request("s1", &serde_json::json!({"model": "test"}))
            .await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_model_select() {
        let bridge = make_bridge();
        let result = bridge.fire_model_select("s1", "deepseek-v3", None).await;
        assert!(matches!(result, HookResult::Continue));
    }

    // ── Tool lifecycle ──

    #[tokio::test]
    async fn test_fire_tool_call_before() {
        let bridge = make_bridge();
        let result = bridge
            .fire_tool_call_before("s1", "read", &serde_json::json!({"path": "x"}))
            .await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_tool_call_after() {
        let bridge = make_bridge();
        let result = bridge.fire_tool_call_after("s1", "read", true).await;
        assert!(matches!(result, HookResult::Continue));
    }
}
