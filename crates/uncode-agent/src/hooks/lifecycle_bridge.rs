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

    /// Access the ExtensionApi for loading extensions.
    pub fn api(&self) -> &ExtensionApi {
        &self.api
    }

    pub fn registry(&self) -> &Arc<uncode_extensions::hooks::HookRegistry> {
        &self.registry
    }

    // ── 现有 hook ──

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

    // ── Session 管理 ──

    pub async fn fire_session_shutdown(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry
            .fire(LifecycleHook::SessionShutdown, &ctx)
            .await
    }

    pub async fn fire_session_before_compact(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
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

    // ── Agent 生命周期 ──

    pub async fn fire_before_agent_start(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
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
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::AgentEnd, &ctx).await
    }

    // ── LLM 交互 ──

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

    pub async fn fire_after_provider_response(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry
            .fire(LifecycleHook::AfterProviderResponse, &ctx)
            .await
    }

    // ── 流式更新 ──

    pub async fn fire_message_update(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::MessageUpdate, &ctx).await
    }

    // ── 模型事件 ──

    pub async fn fire_model_select(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry.fire(LifecycleHook::ModelSelect, &ctx).await
    }

    // ── 工具执行细化 ──

    pub async fn fire_tool_execution_start(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
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

    pub async fn fire_tool_execution_end(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry
            .fire(LifecycleHook::ToolExecutionEnd, &ctx)
            .await
    }

    // ── 资源发现 ──

    pub async fn fire_resources_discover(&self, session_id: &str) -> HookResult {
        let ctx = HookContext {
            session_id: Some(session_id.to_string()),
            event: HookEvent::None,
        };
        self.registry
            .fire(LifecycleHook::ResourcesDiscover, &ctx)
            .await
    }

    // ── Session 生命周期拦截 (#395) ──

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

    // ── 用户输入拦截 (#396) ──

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
}
