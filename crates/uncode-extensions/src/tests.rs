use std::sync::Arc;

use crate::api::ExtensionApi;
use crate::command::{
    CommandRegistration, ExtKey, ExtKeyEvent, ExtModifiers, ShortcutRegistration,
};
use crate::hooks::{
    Extension, HookContext, HookEvent, HookModification, HookRegistry, HookResult, LifecycleHook,
};
use crate::tool::{ExtensionTool, ExtensionToolMetadata};

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
    let registry = Arc::new(HookRegistry::new());
    let api = Arc::new(ExtensionApi::new(registry.clone()));

    // Nonexistent directory returns 0 extensions loaded.
    let count = loader
        .load_from_dir(&registry, &api, std::path::Path::new("/nonexistent"))
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// ── HookResult tests ──

#[test]
fn test_hook_result_default_is_continue() {
    assert!(matches!(HookResult::default(), HookResult::Continue));
}

// ── ExtensionToolMetadata tests ──

#[test]
fn test_metadata_validate_ok() {
    let meta = ExtensionToolMetadata {
        name: "hello".into(),
        description: "Says hello".into(),
        parameters: serde_json::json!({"type": "object"}),
        label: None,
        sequential: false,
    };
    assert!(meta.validate().is_ok());
}

#[test]
fn test_metadata_validate_empty_name() {
    let meta = ExtensionToolMetadata {
        name: "".into(),
        description: "desc".into(),
        parameters: serde_json::json!({}),
        label: None,
        sequential: false,
    };
    assert!(meta.validate().unwrap_err().contains("empty"));
}

#[test]
fn test_metadata_validate_name_starts_with_digit() {
    let meta = ExtensionToolMetadata {
        name: "123tool".into(),
        description: "desc".into(),
        parameters: serde_json::json!({}),
        label: None,
        sequential: false,
    };
    assert!(
        meta.validate()
            .unwrap_err()
            .contains("letter or underscore")
    );
}

#[test]
fn test_metadata_validate_name_with_whitespace() {
    let meta = ExtensionToolMetadata {
        name: "hello world".into(),
        description: "desc".into(),
        parameters: serde_json::json!({}),
        label: None,
        sequential: false,
    };
    assert!(meta.validate().unwrap_err().contains("whitespace"));
}

#[test]
fn test_metadata_validate_empty_description() {
    let meta = ExtensionToolMetadata {
        name: "hello".into(),
        description: "".into(),
        parameters: serde_json::json!({}),
        label: None,
        sequential: false,
    };
    assert!(meta.validate().unwrap_err().contains("description"));
}

#[test]
fn test_metadata_validate_underscore_start() {
    let meta = ExtensionToolMetadata {
        name: "_private".into(),
        description: "desc".into(),
        parameters: serde_json::json!({}),
        label: None,
        sequential: false,
    };
    assert!(meta.validate().is_ok());
}

// ── ExtensionTool test helper ──

struct HelloTool;

#[async_trait::async_trait]
impl ExtensionTool for HelloTool {
    fn metadata(&self) -> ExtensionToolMetadata {
        ExtensionToolMetadata {
            name: "hello".into(),
            description: "Says hello".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "name": {"type": "string"} }
            }),
            label: Some("Hello".into()),
            sequential: false,
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> anyhow::Result<String> {
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("world");
        Ok(format!("Hello, {name}!"))
    }
}

// ── register_tool tests ──

#[test]
fn test_register_tool_no_callback_returns_error() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let result = api.register_tool(Arc::new(HelloTool));
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_register_tool_with_callback_delegates() {
    let registry = Arc::new(HookRegistry::new());
    use std::sync::atomic::AtomicUsize;
    let called = Arc::new(AtomicUsize::new(0));
    let called_clone = called.clone();

    let callback = Arc::new(
        move |name: String, _tool: Arc<dyn ExtensionTool>| -> Result<(), String> {
            assert_eq!(name, "hello");
            called_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    );

    let api = ExtensionApi::with_callbacks(
        registry,
        Some(callback),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    api.register_tool(Arc::new(HelloTool)).unwrap();
    assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_register_tool_callback_error_propagates() {
    let registry = Arc::new(HookRegistry::new());
    let callback = Arc::new(
        |_name: String, _tool: Arc<dyn ExtensionTool>| -> Result<(), String> {
            Err("rejected".into())
        },
    );

    let api = ExtensionApi::with_callbacks(
        registry,
        Some(callback),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let result = api.register_tool(Arc::new(HelloTool));
    assert!(result.unwrap_err().contains("rejected"));
}

// ── CommandRegistration tests ──

#[test]
fn test_command_validate_ok() {
    let cmd = CommandRegistration {
        name: "my-cmd".into(),
        description: "A custom command".into(),
    };
    assert!(cmd.validate().is_ok());
}

#[test]
fn test_command_validate_reserved_name() {
    for reserved in crate::command::RESERVED_COMMAND_NAMES {
        let cmd = CommandRegistration {
            name: reserved.to_string(),
            description: "desc".into(),
        };
        assert!(cmd.validate().unwrap_err().contains("reserved"));
    }
}

#[test]
fn test_command_validate_empty_name() {
    let cmd = CommandRegistration {
        name: "".into(),
        description: "desc".into(),
    };
    assert!(cmd.validate().unwrap_err().contains("empty"));
}

#[test]
fn test_command_validate_whitespace_name() {
    let cmd = CommandRegistration {
        name: "my cmd".into(),
        description: "desc".into(),
    };
    assert!(cmd.validate().unwrap_err().contains("whitespace"));
}

#[test]
fn test_command_validate_empty_description() {
    let cmd = CommandRegistration {
        name: "my-cmd".into(),
        description: "".into(),
    };
    assert!(cmd.validate().unwrap_err().contains("description"));
}

#[test]
fn test_register_command_no_callback() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let result = api.register_command(CommandRegistration {
        name: "test".into(),
        description: "test".into(),
    });
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_register_command_with_callback() {
    use std::sync::atomic::AtomicUsize;
    let registry = Arc::new(HookRegistry::new());
    let called = Arc::new(AtomicUsize::new(0));
    let called_clone = called.clone();
    let callback = Arc::new(move |cmd: CommandRegistration| -> Result<(), String> {
        assert_eq!(cmd.name, "ext-cmd");
        called_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    let api = ExtensionApi::with_callbacks(
        registry,
        None,
        None,
        Some(callback),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    api.register_command(CommandRegistration {
        name: "ext-cmd".into(),
        description: "Extension command".into(),
    })
    .unwrap();
    assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
}

// ── ShortcutRegistration tests ──

#[test]
fn test_shortcut_validate_ok() {
    let shortcut = ShortcutRegistration {
        key: ExtKeyEvent {
            key: ExtKey::F(5),
            modifiers: ExtModifiers {
                ctrl: true,
                alt: false,
                shift: false,
            },
        },
        description: "Run extension".into(),
    };
    assert!(shortcut.validate().is_ok());
}

#[test]
fn test_shortcut_validate_reserved() {
    let shortcut = ShortcutRegistration {
        key: ExtKeyEvent {
            key: ExtKey::Char('c'),
            modifiers: ExtModifiers {
                ctrl: true,
                alt: false,
                shift: false,
            },
        },
        description: "Override quit".into(),
    };
    assert!(shortcut.validate().unwrap_err().contains("reserved"));
}

#[test]
fn test_shortcut_validate_empty_description() {
    let shortcut = ShortcutRegistration {
        key: ExtKeyEvent {
            key: ExtKey::F(5),
            modifiers: ExtModifiers::default(),
        },
        description: "".into(),
    };
    assert!(shortcut.validate().unwrap_err().contains("description"));
}

#[test]
fn test_register_shortcut_with_callback() {
    use std::sync::atomic::AtomicUsize;
    let registry = Arc::new(HookRegistry::new());
    let called = Arc::new(AtomicUsize::new(0));
    let called_clone = called.clone();
    let callback = Arc::new(
        move |_shortcut: ShortcutRegistration| -> Result<(), String> {
            called_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    );
    let api = ExtensionApi::with_callbacks(
        registry,
        None,
        None,
        None,
        None,
        Some(callback),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    api.register_shortcut(ShortcutRegistration {
        key: ExtKeyEvent {
            key: ExtKey::F(5),
            modifiers: ExtModifiers::default(),
        },
        description: "Test shortcut".into(),
    })
    .unwrap();
    assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
}
