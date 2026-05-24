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
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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

// ── Header / Footer / Indicator API tests ──

#[test]
fn test_set_header_no_callback() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let result = api.set_header(None);
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_set_footer_no_callback() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let result = api.set_footer(None);
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_set_working_indicator_no_callback() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let result = api.set_working_indicator(None);
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_set_header_with_callback() {
    use std::sync::atomic::AtomicBool;
    let registry = Arc::new(HookRegistry::new());
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();
    let callback = Arc::new(
        move |_config: Option<crate::header_footer::HeaderConfig>| -> Result<(), String> {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    );
    let api = ExtensionApi::with_callbacks(
        registry,
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
    );
    api.set_header(None).unwrap();
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_set_footer_with_callback() {
    use std::sync::atomic::AtomicBool;
    let registry = Arc::new(HookRegistry::new());
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();
    let callback = Arc::new(
        move |_config: Option<crate::header_footer::FooterConfig>| -> Result<(), String> {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    );
    let api = ExtensionApi::with_callbacks(
        registry,
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
    );
    api.set_footer(None).unwrap();
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_set_indicator_with_callback() {
    use std::sync::atomic::AtomicBool;
    let registry = Arc::new(HookRegistry::new());
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();
    let callback = Arc::new(
        move |_config: Option<crate::header_footer::WorkingIndicatorConfig>| -> Result<(), String> {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    );
    let api = ExtensionApi::with_callbacks(
        registry,
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
    );
    api.set_working_indicator(None).unwrap();
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_set_theme_no_callback() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let config = crate::theme_control::ThemeControlConfig {
        theme_name: "dark".into(),
    };
    let result = api.set_theme(config);
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_set_thinking_labels_no_callback() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let mut labels = std::collections::HashMap::new();
    labels.insert("high".into(), "深度".into());
    let config = crate::theme_control::ThinkingLabelConfig { labels };
    let result = api.set_thinking_labels(config);
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_set_theme_with_callback() {
    use std::sync::atomic::AtomicBool;
    let registry = Arc::new(HookRegistry::new());
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();
    let callback = Arc::new(
        move |_config: crate::theme_control::ThemeControlConfig| -> Result<(), String> {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    );
    let api = ExtensionApi::with_callbacks(
        registry,
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
    );
    api.set_theme(crate::theme_control::ThemeControlConfig {
        theme_name: "monokai".into(),
    })
    .unwrap();
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_set_thinking_labels_with_callback() {
    use std::sync::atomic::AtomicBool;
    let registry = Arc::new(HookRegistry::new());
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();
    let callback = Arc::new(
        move |_config: crate::theme_control::ThinkingLabelConfig| -> Result<(), String> {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    );
    let mut labels = std::collections::HashMap::new();
    labels.insert("high".into(), "deep".into());
    let api = ExtensionApi::with_callbacks(
        registry,
        std::sync::Arc::new(crate::event_bus::EventBus::new()),
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
        None,
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
    );
    api.set_thinking_labels(crate::theme_control::ThinkingLabelConfig { labels })
        .unwrap();
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

// ═══════════════════════════════════════════════════════════════
// New feature tests: #395 session hooks, #396 input/thinking,
// #397 exec, #398 hook_types, state tracker
// ═══════════════════════════════════════════════════════════════

use crate::hooks::{InputAction, InputSource};

// ── #395 Session lifecycle hook names ──

#[test]
fn test_session_lifecycle_hook_names() {
    assert_eq!(
        LifecycleHook::SessionBeforeSwitch.name(),
        "session_before_switch"
    );
    assert_eq!(
        LifecycleHook::SessionBeforeFork.name(),
        "session_before_fork"
    );
    assert_eq!(
        LifecycleHook::SessionBeforeTree.name(),
        "session_before_tree"
    );
    assert_eq!(LifecycleHook::SessionTree.name(), "session_tree");
}

// ── #396 Input/thinking hook names ──

#[test]
fn test_input_thinking_hook_names() {
    assert_eq!(LifecycleHook::Input.name(), "input");
    assert_eq!(
        LifecycleHook::ThinkingLevelSelect.name(),
        "thinking_level_select"
    );
}

// ── HookRegistry: unregister then re-register ──

#[test]
fn test_hook_registry_unregister() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("removable"));
    registry.register(
        ext,
        vec![LifecycleHook::SessionStart, LifecycleHook::TurnStart],
    );
    assert_eq!(registry.extension_count(), 1);
    assert_eq!(registry.hook_count(), 2);

    assert!(registry.unregister("removable"));
    assert_eq!(registry.extension_count(), 0);
    assert_eq!(registry.hook_count(), 0);

    // Second unregister returns false
    assert!(!registry.unregister("removable"));
}

#[test]
fn test_hook_registry_unregister_cleans_empty_hooks() {
    let registry = HookRegistry::new();
    let ext1 = Arc::new(TestExtension::new("shared"));
    let ext2 = Arc::new(TestExtension::new("unique"));

    // Both register for SessionStart, ext2 also registers for TurnStart
    registry.register(ext1, vec![LifecycleHook::SessionStart]);
    registry.register(
        ext2,
        vec![LifecycleHook::SessionStart, LifecycleHook::TurnStart],
    );
    assert_eq!(registry.hook_count(), 2);

    // Remove ext2 — TurnStart should be cleaned up, SessionStart should remain
    registry.unregister("unique");
    assert!(registry.has_hook("session_start"));
    assert!(!registry.has_hook("turn_start"));
}

// ── HookRegistry: fire with new hook variants ──

#[tokio::test]
async fn test_fire_session_before_switch() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("session-guard"));
    registry.register(ext.clone(), vec![LifecycleHook::SessionBeforeSwitch]);

    let ctx = HookContext {
        session_id: Some("s1".into()),
        event: HookEvent::SessionSwitch {
            session_id: "s2".into(),
        },
    };
    let result = registry
        .fire(LifecycleHook::SessionBeforeSwitch, &ctx)
        .await;
    assert_eq!(ext.count(), 1);
    assert!(matches!(result, HookResult::Continue));
}

#[tokio::test]
async fn test_fire_session_before_fork_blocked() {
    let registry = HookRegistry::new();
    let ext = Arc::new(BlockingExtension::new("fork-guard"));
    registry.register(ext, vec![LifecycleHook::SessionBeforeFork]);

    let ctx = HookContext {
        session_id: Some("s1".into()),
        event: HookEvent::SessionFork {
            entry_id: "e1".into(),
        },
    };
    let result = registry.fire(LifecycleHook::SessionBeforeFork, &ctx).await;
    assert!(matches!(result, HookResult::Block { .. }));
}

#[tokio::test]
async fn test_fire_session_before_tree() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("tree-guard"));
    registry.register(ext.clone(), vec![LifecycleHook::SessionBeforeTree]);

    let ctx = HookContext {
        session_id: Some("s1".into()),
        event: HookEvent::SessionTreeNav {
            entry_id: "e1".into(),
        },
    };
    let result = registry.fire(LifecycleHook::SessionBeforeTree, &ctx).await;
    assert_eq!(ext.count(), 1);
    assert!(matches!(result, HookResult::Continue));
}

#[tokio::test]
async fn test_fire_session_tree_notification() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("tree-observer"));
    registry.register(ext.clone(), vec![LifecycleHook::SessionTree]);

    let ctx = HookContext {
        session_id: Some("s1".into()),
        event: HookEvent::SessionTreeResult {
            new_leaf_id: "leaf2".into(),
            old_leaf_id: "leaf1".into(),
            summary: Some("branched".into()),
        },
    };
    let result = registry.fire(LifecycleHook::SessionTree, &ctx).await;
    assert_eq!(ext.count(), 1);
    assert!(matches!(result, HookResult::Continue));
}

// ── #396 Input hook ──

#[tokio::test]
async fn test_fire_input_hook() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("input-observer"));
    registry.register(ext.clone(), vec![LifecycleHook::Input]);

    let ctx = HookContext {
        session_id: Some("s1".into()),
        event: HookEvent::Input {
            source: InputSource::Interactive,
            text: "hello world".into(),
            images: vec![],
        },
    };
    let result = registry.fire(LifecycleHook::Input, &ctx).await;
    assert_eq!(ext.count(), 1);
    assert!(matches!(result, HookResult::Continue));
}

#[tokio::test]
async fn test_fire_input_hook_blocked() {
    let registry = HookRegistry::new();
    let ext = Arc::new(BlockingExtension::new("input-blocker"));
    registry.register(ext, vec![LifecycleHook::Input]);

    let ctx = HookContext {
        session_id: Some("s1".into()),
        event: HookEvent::Input {
            source: InputSource::Rpc,
            text: "dangerous command".into(),
            images: vec!["img1".into()],
        },
    };
    let result = registry.fire(LifecycleHook::Input, &ctx).await;
    assert!(matches!(result, HookResult::Block { .. }));
}

#[tokio::test]
async fn test_fire_thinking_level_select() {
    let registry = HookRegistry::new();
    let ext = Arc::new(TestExtension::new("thinking-observer"));
    registry.register(ext.clone(), vec![LifecycleHook::ThinkingLevelSelect]);

    let ctx = HookContext {
        session_id: Some("s1".into()),
        event: HookEvent::ThinkingLevelSelect {
            level: "high".into(),
            previous_level: Some("medium".into()),
        },
    };
    let result = registry
        .fire(LifecycleHook::ThinkingLevelSelect, &ctx)
        .await;
    assert_eq!(ext.count(), 1);
    assert!(matches!(result, HookResult::Continue));
}

// ── InputAction / InputSource ──

#[test]
fn test_input_source_equality() {
    assert_eq!(InputSource::Interactive, InputSource::Interactive);
    assert_ne!(InputSource::Interactive, InputSource::Rpc);
    assert_ne!(InputSource::Rpc, InputSource::Extension);
}

#[test]
fn test_input_action_debug_format() {
    let action = InputAction::Transform {
        text: Some("replaced".into()),
        images: None,
    };
    let debug = format!("{action:?}");
    assert!(debug.contains("Transform"));
}

#[test]
fn test_hook_modification_default_no_input_action() {
    let mod_ = HookModification::default();
    assert!(mod_.input_action.is_none());
    assert!(mod_.args_override.is_none());
    assert!(mod_.additional_messages.is_none());
}

// ── #397 Exec callback ──

#[test]
fn test_exec_no_callback_returns_error() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let result = api.exec("cargo test");
    assert!(result.unwrap_err().contains("no callback"));
}

#[test]
fn test_exec_empty_command_returns_error() {
    let registry = Arc::new(HookRegistry::new());
    let callback = Arc::new(|_cmd: &str| -> Result<crate::api::ExecResult, String> {
        Ok(crate::api::ExecResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        })
    });

    let api = ExtensionApi::with_callbacks(
        registry,
        Arc::new(crate::event_bus::EventBus::new()),
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
        Some(callback),
    );
    let result = api.exec("");
    assert!(result.unwrap_err().contains("empty"));
}

#[test]
fn test_exec_with_callback_success() {
    let registry = Arc::new(HookRegistry::new());
    let callback = Arc::new(|cmd: &str| -> Result<crate::api::ExecResult, String> {
        Ok(crate::api::ExecResult {
            stdout: format!("ran: {cmd}"),
            stderr: String::new(),
            exit_code: 0,
        })
    });

    let api = ExtensionApi::with_callbacks(
        registry,
        Arc::new(crate::event_bus::EventBus::new()),
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
        Some(callback),
    );
    let result = api.exec("cargo build").unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "ran: cargo build");
}

#[test]
fn test_exec_with_callback_denied() {
    let registry = Arc::new(HookRegistry::new());
    let callback = Arc::new(|cmd: &str| -> Result<crate::api::ExecResult, String> {
        if cmd.starts_with("rm") {
            Err("dangerous command denied".into())
        } else {
            Ok(crate::api::ExecResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
            })
        }
    });

    let api = ExtensionApi::with_callbacks(
        registry,
        Arc::new(crate::event_bus::EventBus::new()),
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
        Some(callback),
    );
    let result = api.exec("rm -rf /");
    assert!(result.unwrap_err().contains("denied"));
}

// ── send_message / append_entry with callbacks ──

#[test]
fn test_send_message_with_callback() {
    let registry = Arc::new(HookRegistry::new());
    use std::sync::atomic::AtomicBool;
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let callback: crate::api::SendMessageCallback = Arc::new(move |_msg| {
        called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });

    let api = ExtensionApi::with_callbacks(
        registry,
        Arc::new(crate::event_bus::EventBus::new()),
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
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(callback),
        None,
        None,
    );

    let msg = uncode_core::message::Message {
        id: "test".into(),
        role: uncode_core::message::Role::User,
        content: vec![uncode_core::message::ContentBlock::Text {
            text: "hello".into(),
        }],
        usage: None,
        stop_reason: None,
        error_message: None,
        timestamp: None,
    };
    api.send_message(msg).unwrap();
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_append_entry_with_callback() {
    let registry = Arc::new(HookRegistry::new());
    use std::sync::atomic::AtomicBool;
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let callback: crate::api::AppendEntryCallback = Arc::new(move |_typ, _data| {
        called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });

    let api = ExtensionApi::with_callbacks(
        registry,
        Arc::new(crate::event_bus::EventBus::new()),
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
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(callback),
        None,
    );

    api.append_entry("custom".into(), serde_json::json!({"key": "value"}))
        .unwrap();
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_append_entry_empty_type_rejected() {
    let registry = Arc::new(HookRegistry::new());
    let api = ExtensionApi::new(registry);
    let result = api.append_entry("".into(), serde_json::json!({}));
    assert!(result.unwrap_err().contains("empty"));
}

// ── HookEvent variant construction ──

#[test]
fn test_hook_event_session_switch() {
    let event = HookEvent::SessionSwitch {
        session_id: "target".into(),
    };
    let ctx = HookContext {
        session_id: Some("src".into()),
        event,
    };
    assert_eq!(ctx.session_id.as_deref(), Some("src"));
}

#[test]
fn test_hook_event_session_fork() {
    let event = HookEvent::SessionFork {
        entry_id: "entry-1".into(),
    };
    let ctx = HookContext {
        session_id: None,
        event,
    };
    assert!(ctx.session_id.is_none());
}

#[test]
fn test_hook_event_session_tree_result() {
    let event = HookEvent::SessionTreeResult {
        new_leaf_id: "new".into(),
        old_leaf_id: "old".into(),
        summary: None,
    };
    let ctx = HookContext {
        session_id: Some("s".into()),
        event,
    };
    assert_eq!(ctx.session_id.as_deref(), Some("s"));
}

#[test]
fn test_hook_event_input_with_source() {
    let event = HookEvent::Input {
        source: InputSource::Extension,
        text: "injected text".into(),
        images: vec!["base64data".into()],
    };
    let ctx = HookContext {
        session_id: None,
        event,
    };
    assert!(ctx.session_id.is_none());
}

#[test]
fn test_hook_event_thinking_level_select() {
    let event = HookEvent::ThinkingLevelSelect {
        level: "high".into(),
        previous_level: None,
    };
    let ctx = HookContext {
        session_id: Some("s".into()),
        event,
    };
    assert_eq!(ctx.session_id.as_deref(), Some("s"));
}

// ── HookModification with input_action ──

#[test]
fn test_hook_modification_with_input_action_continue() {
    let mod_ = HookModification {
        input_action: Some(InputAction::Continue),
        ..Default::default()
    };
    assert!(mod_.input_action.is_some());
}

#[test]
fn test_hook_modification_with_input_action_transform() {
    let mod_ = HookModification {
        input_action: Some(InputAction::Transform {
            text: Some("replaced".into()),
            images: Some(vec!["img".into()]),
        }),
        ..Default::default()
    };
    if let Some(InputAction::Transform { text, images }) = &mod_.input_action {
        assert_eq!(text.as_deref(), Some("replaced"));
        assert_eq!(images.as_ref().unwrap().len(), 1);
    } else {
        panic!("expected Transform");
    }
}

#[test]
fn test_hook_modification_with_input_action_handled() {
    let mod_ = HookModification {
        input_action: Some(InputAction::Handled),
        ..Default::default()
    };
    assert!(matches!(mod_.input_action, Some(InputAction::Handled)));
}

// ── ExtensionStateTracker tests ──

#[test]
fn test_state_tracker_new_is_empty() {
    let tracker = crate::state::ExtensionStateTracker::new();
    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);
    assert!(tracker.list().is_empty());
}

#[test]
fn test_state_tracker_insert_and_get() {
    let tracker = crate::state::ExtensionStateTracker::new();
    let record = crate::state::ExtensionRecord {
        name: "test-ext".into(),
        state: crate::state::ExtensionState::Active,
        wasm_path: std::path::PathBuf::from("/tmp/test.wasm"),
        source: crate::state::ExtensionSource::Global,
        tools: vec!["hello".into()],
        hooks: vec!["session_start".into()],
    };

    tracker.insert(record);
    assert_eq!(tracker.len(), 1);

    let retrieved = tracker.get("test-ext").unwrap();
    assert_eq!(retrieved.name, "test-ext");
    assert!(matches!(
        retrieved.state,
        crate::state::ExtensionState::Active
    ));
}

#[test]
fn test_state_tracker_get_nonexistent() {
    let tracker = crate::state::ExtensionStateTracker::new();
    assert!(tracker.get("nope").is_none());
}

#[test]
fn test_state_tracker_update_state() {
    let tracker = crate::state::ExtensionStateTracker::new();
    tracker.insert(crate::state::ExtensionRecord {
        name: "ext".into(),
        state: crate::state::ExtensionState::Active,
        wasm_path: std::path::PathBuf::from("/tmp/ext.wasm"),
        source: crate::state::ExtensionSource::Project,
        tools: vec![],
        hooks: vec![],
    });

    assert!(tracker.update_state("ext", crate::state::ExtensionState::Error("boom".into())));
    let rec = tracker.get("ext").unwrap();
    assert!(matches!(&rec.state, crate::state::ExtensionState::Error(e) if e == "boom"));

    // Nonexistent extension
    assert!(!tracker.update_state("nope", crate::state::ExtensionState::Disabled));
}

#[test]
fn test_state_tracker_remove() {
    let tracker = crate::state::ExtensionStateTracker::new();
    tracker.insert(crate::state::ExtensionRecord {
        name: "removable".into(),
        state: crate::state::ExtensionState::Active,
        wasm_path: std::path::PathBuf::from("/tmp/r.wasm"),
        source: crate::state::ExtensionSource::Global,
        tools: vec![],
        hooks: vec![],
    });

    let removed = tracker.remove("removable").unwrap();
    assert_eq!(removed.name, "removable");
    assert!(tracker.is_empty());
    assert!(tracker.remove("removable").is_none());
}

#[test]
fn test_state_tracker_find_by_tool() {
    let tracker = crate::state::ExtensionStateTracker::new();
    tracker.insert(crate::state::ExtensionRecord {
        name: "ext-a".into(),
        state: crate::state::ExtensionState::Active,
        wasm_path: std::path::PathBuf::from("/tmp/a.wasm"),
        source: crate::state::ExtensionSource::Global,
        tools: vec!["tool1".into(), "tool2".into()],
        hooks: vec![],
    });
    tracker.insert(crate::state::ExtensionRecord {
        name: "ext-b".into(),
        state: crate::state::ExtensionState::Active,
        wasm_path: std::path::PathBuf::from("/tmp/b.wasm"),
        source: crate::state::ExtensionSource::Project,
        tools: vec!["tool3".into()],
        hooks: vec![],
    });

    assert_eq!(tracker.find_by_tool("tool1"), Some("ext-a".into()));
    assert_eq!(tracker.find_by_tool("tool3"), Some("ext-b".into()));
    assert_eq!(tracker.find_by_tool("tool99"), None);
}

#[test]
fn test_state_tracker_list() {
    let tracker = crate::state::ExtensionStateTracker::new();
    tracker.insert(crate::state::ExtensionRecord {
        name: "ext-a".into(),
        state: crate::state::ExtensionState::Active,
        wasm_path: std::path::PathBuf::from("/tmp/a.wasm"),
        source: crate::state::ExtensionSource::Global,
        tools: vec![],
        hooks: vec![],
    });
    tracker.insert(crate::state::ExtensionRecord {
        name: "ext-b".into(),
        state: crate::state::ExtensionState::Disabled,
        wasm_path: std::path::PathBuf::from("/tmp/b.wasm"),
        source: crate::state::ExtensionSource::Project,
        tools: vec![],
        hooks: vec![],
    });

    let list = tracker.list();
    assert_eq!(list.len(), 2);
    let names: Vec<&str> = list.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"ext-a"));
    assert!(names.contains(&"ext-b"));
}

#[test]
fn test_extension_source_display() {
    assert_eq!(
        format!("{}", crate::state::ExtensionSource::Global),
        "global"
    );
    assert_eq!(
        format!("{}", crate::state::ExtensionSource::Project),
        "project"
    );
}

#[test]
fn test_state_tracker_default() {
    let tracker = crate::state::ExtensionStateTracker::default();
    assert!(tracker.is_empty());
}
