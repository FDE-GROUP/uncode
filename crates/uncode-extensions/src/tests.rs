use std::sync::Arc;

use crate::api::ExtensionApi;
use crate::hooks::{
    Extension, HookContext, HookEvent, HookModification, HookRegistry, HookResult, LifecycleHook,
};

// ── Test extension implementations ──

struct TestExtension {
    name: String,
    call_count: std::sync::atomic::AtomicU32,
}

impl TestExtension {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            call_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn count(&self) -> u32 {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Extension for TestExtension {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_hook(&self, _ctx: &HookContext) -> anyhow::Result<HookResult> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(HookResult::Continue)
    }
}

struct BlockingExtension {
    name: String,
    call_count: std::sync::atomic::AtomicU32,
}

impl BlockingExtension {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            call_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn count(&self) -> u32 {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Extension for BlockingExtension {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_hook(&self, _ctx: &HookContext) -> anyhow::Result<HookResult> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(HookResult::Block {
            reason: "blocked by test".into(),
        })
    }
}

struct ModifyingExtension;

#[async_trait::async_trait]
impl Extension for ModifyingExtension {
    fn name(&self) -> &str {
        "modifying-ext"
    }

    async fn on_hook(&self, _ctx: &HookContext) -> anyhow::Result<HookResult> {
        Ok(HookResult::Modify(HookModification::default()))
    }
}

// ── LifecycleHook tests ──

#[test]
fn test_lifecycle_hook_names() {
    assert_eq!(LifecycleHook::SessionStart.name(), "session_start");
    assert_eq!(LifecycleHook::TurnStart.name(), "turn_start");
    assert_eq!(LifecycleHook::MessageReceived.name(), "message_received");
    assert_eq!(LifecycleHook::MessageSending.name(), "message_sending");
    assert_eq!(LifecycleHook::ToolCallBefore.name(), "tool_call_before");
    assert_eq!(LifecycleHook::ToolCallAfter.name(), "tool_call_after");
    assert_eq!(LifecycleHook::TurnEnd.name(), "turn_end");
    assert_eq!(LifecycleHook::SessionEnd.name(), "session_end");
}

// ── HookRegistry tests ──

#[test]
fn test_hook_registry_new() {
    let registry = HookRegistry::new();
    assert_eq!(registry.extension_count(), 0);
    assert_eq!(registry.hook_count(), 0);
}

#[test]
fn test_hook_registry_default() {
    let registry = HookRegistry::default();
    assert_eq!(registry.extension_count(), 0);
}

#[test]
fn test_register_single_extension() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("test-ext"));

    registry.register(ext, vec![LifecycleHook::SessionStart]);

    assert_eq!(registry.extension_count(), 1);
    assert!(registry.has_hook("session_start"));
}

#[test]
fn test_register_multiple_extensions_same_hook() {
    let registry = HookRegistry::new();
    let ext1 = Arc::new(TestExtension::new("ext1"));
    let ext2 = Arc::new(TestExtension::new("ext2"));

    registry.register(ext1, vec![LifecycleHook::TurnStart]);
    registry.register(ext2, vec![LifecycleHook::TurnStart]);

    assert_eq!(registry.extension_count(), 2);
    assert!(registry.has_hook("turn_start"));
}

#[test]
fn test_register_multiple_hooks_for_one_extension() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("multi-hook"));

    registry.register(
        ext,
        vec![
            LifecycleHook::SessionStart,
            LifecycleHook::SessionEnd,
            LifecycleHook::TurnEnd,
        ],
    );

    assert_eq!(registry.extension_count(), 1);
    assert_eq!(registry.hook_count(), 3);
    assert!(registry.has_hook("session_start"));
    assert!(registry.has_hook("session_end"));
    assert!(registry.has_hook("turn_end"));
}

#[tokio::test]
async fn test_fire_hook_calls_extension() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("counter"));

    registry.register(ext.clone(), vec![LifecycleHook::TurnStart]);

    let ctx = HookContext {
        session_id: Some("test-session".into()),
        event: HookEvent::None,
    };

    let result = registry.fire(LifecycleHook::TurnStart, &ctx).await;

    assert_eq!(ext.count(), 1);
    assert!(matches!(result, HookResult::Continue));
}

#[tokio::test]
async fn test_fire_nonexistent_hook_returns_continue() {
    let registry = HookRegistry::new();
    let ctx = HookContext {
        session_id: None,
        event: HookEvent::None,
    };

    let result = registry.fire(LifecycleHook::ToolCallBefore, &ctx).await;
    assert!(matches!(result, HookResult::Continue));
}

#[tokio::test]
async fn test_fire_multiple_extensions() {
    let registry = HookRegistry::new();
    let ext1 = Arc::new(TestExtension::new("a"));
    let ext2 = Arc::new(TestExtension::new("b"));

    registry.register(ext1.clone(), vec![LifecycleHook::SessionEnd]);
    registry.register(ext2.clone(), vec![LifecycleHook::SessionEnd]);

    let ctx = HookContext {
        session_id: None,
        event: HookEvent::None,
    };

    registry.fire(LifecycleHook::SessionEnd, &ctx).await;

    assert_eq!(ext1.count(), 1);
    assert_eq!(ext2.count(), 1);
}

#[tokio::test]
async fn test_fire_respects_hook_filtering() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("filtered"));

    // Register only for SessionStart
    registry.register(ext.clone(), vec![LifecycleHook::SessionStart]);

    let ctx = HookContext {
        session_id: None,
        event: HookEvent::None,
    };

    // Fire a different hook — extension should NOT be called
    registry.fire(LifecycleHook::SessionEnd, &ctx).await;
    assert_eq!(ext.count(), 0);

    // Fire the registered hook — extension SHOULD be called
    registry.fire(LifecycleHook::SessionStart, &ctx).await;
    assert_eq!(ext.count(), 1);
}

#[tokio::test]
async fn test_fire_returns_block() {
    let registry = HookRegistry::new();
    let ext = Arc::new(BlockingExtension::new("blocker"));

    registry.register(ext.clone(), vec![LifecycleHook::ToolCallBefore]);

    let ctx = HookContext {
        session_id: None,
        event: HookEvent::None,
    };

    let result = registry.fire(LifecycleHook::ToolCallBefore, &ctx).await;

    assert_eq!(ext.count(), 1);
    assert!(matches!(result, HookResult::Block { .. }));
}

#[tokio::test]
async fn test_fire_returns_modify() {
    let registry = HookRegistry::new();
    let ext = Arc::new(ModifyingExtension);

    registry.register(ext, vec![LifecycleHook::ToolCallAfter]);

    let ctx = HookContext {
        session_id: None,
        event: HookEvent::None,
    };

    let result = registry.fire(LifecycleHook::ToolCallAfter, &ctx).await;

    assert!(matches!(result, HookResult::Modify(_)));
}

#[tokio::test]
async fn test_fire_first_block_wins() {
    let registry = HookRegistry::new();
    let blocker = Arc::new(BlockingExtension::new("blocker"));
    let observer = Arc::new(TestExtension::new("observer"));

    // Register blocker first, observer second
    registry.register(blocker.clone(), vec![LifecycleHook::ToolCallBefore]);
    registry.register(observer.clone(), vec![LifecycleHook::ToolCallBefore]);

    let ctx = HookContext {
        session_id: None,
        event: HookEvent::None,
    };

    let result = registry.fire(LifecycleHook::ToolCallBefore, &ctx).await;

    // Blocker ran and returned Block
    assert_eq!(blocker.count(), 1);
    // Observer was never called (first block wins)
    assert_eq!(observer.count(), 0);
    assert!(matches!(result, HookResult::Block { .. }));
}

// ── ExtensionApi tests ──

#[test]
fn test_extension_api_new() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry.clone());
    assert_eq!(api.registry().extension_count(), 0);
}

#[test]
fn test_extension_api_register_extension() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry.clone());
    let ext = Arc::new(TestExtension::new("api-ext"));

    api.register_extension(ext, vec![LifecycleHook::MessageReceived]);

    assert_eq!(registry.extension_count(), 1);
    assert!(registry.has_hook("message_received"));
}

// ── HookContext tests ──

#[test]
fn test_hook_context_with_session() {
    let ctx = HookContext {
        session_id: Some("sess-123".into()),
        event: HookEvent::None,
    };
    assert_eq!(ctx.session_id.as_deref(), Some("sess-123"));
}

#[test]
fn test_hook_context_without_session() {
    let ctx = HookContext {
        session_id: None,
        event: HookEvent::None,
    };
    assert!(ctx.session_id.is_none());
}

// ── ExtensionLoader tests ──

#[test]
fn test_loader_new_and_default() {
    let loader = crate::loader::ExtensionLoader::new();
    let _ = loader;
    let default_loader = crate::loader::ExtensionLoader;
    let _ = default_loader;
}

#[tokio::test]
async fn test_load_from_dir_returns_zero() {
    let loader = crate::loader::ExtensionLoader::new();
    let registry = HookRegistry::new();

    // WASM extension loading not yet implemented, always returns 0
    let count = loader
        .load_from_dir(&registry, std::path::Path::new("/nonexistent"))
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// ── HookResult tests ──

#[test]
fn test_hook_result_default_is_continue() {
    assert!(matches!(HookResult::default(), HookResult::Continue));
}
