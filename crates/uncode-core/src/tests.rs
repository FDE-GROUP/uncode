#[cfg(test)]
mod tests {
    use crate::event::*;
    use crate::message::*;
    use crate::session::*;
    use crate::tool::*;

    #[test]
    fn test_message_user_creation() {
        let msg = Message::user("hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected text block"),
        }
    }

    #[test]
    fn test_message_with_usage() {
        let msg = Message::assistant("response").with_usage(100, 50);
        assert_eq!(msg.role, Role::Assistant);
        let usage = msg.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = Message::user("test message");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, msg.id);
        assert_eq!(parsed.role, msg.role);
    }

    #[test]
    fn test_content_block_json() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains("hello"));

        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        match parsed {
            ContentBlock::Text { text } => assert_eq!(text, "hello"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_content_block_tool_call_json() {
        let block = ContentBlock::ToolCall(Box::new(ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
        }));
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"tool_call""#));
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        match parsed {
            ContentBlock::ToolCall(tc) => assert_eq!(tc.id, "call_1"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_agent_event_json() {
        let event = AgentEvent::TaskUpdate {
            data: Box::new(crate::event::TaskUpdateData {
                task_id: "t1".into(),
                status: TaskStatus::Running,
                title: "test task".into(),
                subtasks: vec![],
                depends_on: vec![],
            }),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::TaskUpdate { data } => {
                assert_eq!(data.task_id, "t1");
                assert!(matches!(data.status, TaskStatus::Running));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_session_header_json() {
        let header = SessionHeader::new(
            "abc123".into(),
            "deepseek-v3".into(),
            "/home/user/project".into(),
        );
        let json = serde_json::to_string(&header).unwrap();
        assert!(json.contains(r#""type":"session""#));
        assert!(json.contains("abc123"));
        assert!(json.contains(r#""version":2"#));
    }

    #[test]
    fn test_session_entry_message_json() {
        let msg = Message::user("hello");
        let entry = SessionEntry::Message(Box::new(MessageEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            role: msg.role,
            content: msg.content,
            usage: None,
        }));
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""type":"message""#));
        let parsed: SessionEntry = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SessionEntry::Message(..)));
    }

    #[test]
    fn test_tool_definition_default() {
        let def = ToolDefinition {
            name: "test".into(),
            description: "a test tool".into(),
            parameters: serde_json::json!({"type": "object"}),
            label: None,
            execution_mode: ExecutionMode::default(),
        };
        assert_eq!(def.name, "test");
    }

    #[test]
    fn test_execution_mode_default() {
        assert_eq!(ExecutionMode::default(), ExecutionMode::Parallel);
    }

    #[test]
    fn test_error_types() {
        let err = crate::error::UncodeError::Tool("tool failed".into());
        assert!(err.to_string().contains("tool failed"));

        let io_err = crate::error::UncodeError::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "nope",
        ));
        assert!(matches!(io_err, crate::error::UncodeError::File(..)));
    }

    #[test]
    fn test_error_code_stability() {
        use crate::error::*;

        // File errors: 10xx
        let e = FileError::not_found("/tmp/missing.txt");
        assert_eq!(e.code(), 1001);
        let e = FileError::sandbox_violation("/etc/passwd");
        assert_eq!(e.code(), 1003);

        // Execution errors: 20xx
        let e = ExecutionError::non_zero_exit("rm -rf /", 1);
        assert_eq!(e.code(), 2001);
        let e = ExecutionError::timeout("sleep 999", 5000);
        assert_eq!(e.code(), 2002);

        // Compaction errors: 30xx
        let e = CompactionError::llm_failed("timeout");
        assert_eq!(e.code(), 3001);
        let e = CompactionError::cut_point_not_found();
        assert_eq!(e.code(), 3002);

        // Harness errors: 50xx
        let e = HarnessError::busy("turn");
        assert_eq!(e.code(), 5001);
        let e = HarnessError::no_session();
        assert_eq!(e.code(), 5002);

        // Io → File routing
        let io_err = UncodeError::from(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
        match io_err {
            UncodeError::File(fe) => assert_eq!(fe.code(), 1001),
            other => panic!("expected File, got {other}"),
        }
    }

    #[test]
    fn test_model_info() {
        let info = crate::model::ModelInfo {
            id: "gpt-4".into(),
            provider: "openai".into(),
            display_name: "GPT-4".into(),
            max_tokens: 8192,
            supports_vision: false,
            supports_tools: true,
            pricing: None,
        };
        assert_eq!(info.provider, "openai");
        assert!(info.supports_tools);
    }

    #[test]
    fn test_config_default() {
        let config = crate::config::AppConfig::default();
        assert_eq!(config.model, "deepseek-v3");
        assert!(config.providers.ollama.is_none());
    }

    #[test]
    fn test_session_entry_branch_json() {
        let entry = SessionEntry::Branch(Box::new(BranchEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            parent_session_id: "parent123".into(),
            reason: "explore alternative".into(),
        }));
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""type":"branch""#));
        let parsed: SessionEntry = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SessionEntry::Branch(..)));
    }

    #[test]
    fn test_convert_to_llm_filters_excluded() {
        use crate::message::{ContentBlock, Message, convert_to_llm};

        let msg = Message::new(
            crate::message::Role::User,
            vec![ContentBlock::BashExecution {
                command: "rm -rf /".into(),
                output: "deleted".into(),
                exit_code: 0,
                cancelled: false,
                exclude_from_context: true,
            }],
        );
        let result = convert_to_llm(vec![msg]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_convert_to_llm_branch_summary() {
        use crate::message::{ContentBlock, Message, convert_to_llm};

        let msg = Message::new(
            crate::message::Role::User,
            vec![ContentBlock::BranchSummary {
                summary: "explored auth".into(),
                from_id: "abc".into(),
            }],
        );
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        match &result[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("[branch summary from abc]"));
                assert!(text.contains("explored auth"));
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn test_convert_to_llm_passes_through() {
        use crate::message::{Message, convert_to_llm};

        let msgs = vec![Message::user("hello"), Message::assistant("world")];
        let result = convert_to_llm(msgs);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, crate::message::Role::User);
        assert_eq!(result[1].role, crate::message::Role::Assistant);
    }

    #[test]
    fn test_convert_to_llm_bash_visible() {
        use crate::message::{ContentBlock, Message, convert_to_llm};

        let msg = Message::new(
            crate::message::Role::User,
            vec![ContentBlock::BashExecution {
                command: "ls".into(),
                output: "file.txt".into(),
                exit_code: 0,
                cancelled: false,
                exclude_from_context: false,
            }],
        );
        let result = convert_to_llm(vec![msg]);
        assert_eq!(result.len(), 1);
        match &result[0].content[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("[bash] ls"));
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_hook_block_tool() {
        use crate::event::*;

        let mut router = EventRouter::new();
        router.on_hook(
            "tool_call_start",
            Box::new(|_event| {
                Box::pin(async {
                    HookResult::Block {
                        reason: "unsafe".into(),
                    }
                })
            }),
        );

        let event = AgentEvent::ToolCallStart {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            arguments_summary: "rm -rf /".into(),
        };
        let results = router.dispatch_hooks(&event).await;
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], HookResult::Block { reason } if reason == "unsafe"));
    }

    #[tokio::test]
    async fn test_sync_handler_unaffected() {
        use crate::event::*;
        use std::sync::{Arc, Mutex};

        let counter = Arc::new(Mutex::new(0u32));
        let counter_clone = counter.clone();

        let mut router = EventRouter::new();
        router.on(
            "turn_start",
            Box::new(move |_event| {
                *counter_clone.lock().unwrap() += 1;
            }),
        );

        let event = AgentEvent::TurnStart { turn: 1 };
        router.dispatch(&event);
        router.dispatch(&event);
        assert_eq!(*counter.lock().unwrap(), 2);

        // dispatch_hooks on event with no hooks returns empty
        let results = router.dispatch_hooks(&event).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_hook_patch_messages() {
        use crate::event::*;

        let mut router = EventRouter::new();
        router.on_hook(
            "turn_end",
            Box::new(|_event| {
                Box::pin(async {
                    HookResult::PatchMessages {
                        messages: vec![Message::user("injected")],
                    }
                })
            }),
        );

        let event = AgentEvent::TurnEnd {
            turn: 1,
            usage: UsageInfo::default(),
        };
        let results = router.dispatch_hooks(&event).await;
        assert_eq!(results.len(), 1);
        match &results[0] {
            HookResult::PatchMessages { messages } => assert_eq!(messages.len(), 1),
            other => panic!("expected PatchMessages, got {other:?}"),
        }
    }

    // ── clamp_thinking_level ──

    #[test]
    fn test_clamp_thinking_level() {
        use crate::api_types::ThinkingLevel;
        use crate::model::{Model, clamp_thinking_level};
        use std::collections::HashMap;

        // 空 map → Off
        let model = Model::default();
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::High, &model),
            ThinkingLevel::Off
        );

        // 精确匹配
        let mut model = Model::default();
        model.thinking_level_map = HashMap::from([
            (ThinkingLevel::Minimal, None),
            (ThinkingLevel::Low, None),
            (ThinkingLevel::Medium, None),
            (ThinkingLevel::High, Some("high".into())),
            (ThinkingLevel::XHigh, Some("max".into())),
        ]);
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::High, &model),
            ThinkingLevel::High
        );

        // XHigh 在 map 中 → 直接返回
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::XHigh, &model),
            ThinkingLevel::XHigh
        );

        // 降级：请求 Medium 但 map 只有 Low 和 High → 向下找到 Low
        let mut model_sparse = Model::default();
        model_sparse.thinking_level_map = HashMap::from([
            (ThinkingLevel::Low, None),
            (ThinkingLevel::High, Some("high".into())),
        ]);
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Medium, &model_sparse),
            ThinkingLevel::Low
        );

        // Off 始终返回 Off
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::Off, &model_sparse),
            ThinkingLevel::Off
        );
    }

    // ── Transport enum ──

    #[test]
    fn test_transport_default_and_serialize() {
        use crate::api_types::Transport;

        assert_eq!(Transport::default(), Transport::Sse);

        let json = serde_json::to_string(&Transport::WebSocket).unwrap();
        assert!(json.contains("web_socket"));

        let parsed: Transport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Transport::WebSocket);
    }

    #[test]
    fn test_stream_options_transport_field() {
        use crate::api_types::{StreamOptions, Transport};

        let opts = StreamOptions::default();
        assert!(opts.transport.is_none());

        let opts = StreamOptions {
            transport: Some(Transport::Auto),
            ..Default::default()
        };
        assert_eq!(opts.transport.unwrap(), Transport::Auto);
    }
}
