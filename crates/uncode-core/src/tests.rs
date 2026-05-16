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
        let block = ContentBlock::ToolCall(ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
        });
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
            task_id: "t1".into(),
            status: TaskStatus::Running,
            title: "test task".into(),
            subtasks: vec![],
            depends_on: vec![],
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::TaskUpdate {
                task_id, status, ..
            } => {
                assert_eq!(task_id, "t1");
                assert!(matches!(status, TaskStatus::Running));
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
        assert!(json.contains(r#""type":"header""#));
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_session_entry_message_json() {
        let msg = Message::user("hello");
        let entry = SessionEntry::Message(MessageEntry {
            id: None,
            timestamp: chrono::Utc::now(),
            role: msg.role,
            content: msg.content,
            usage: None,
        });
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
        assert!(matches!(io_err, crate::error::UncodeError::Io(..)));
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
        let entry = SessionEntry::Branch(BranchEntry {
            timestamp: chrono::Utc::now(),
            parent_id: "parent123".into(),
            reason: "explore alternative".into(),
        });
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""type":"branch""#));
        let parsed: SessionEntry = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SessionEntry::Branch(..)));
    }
}
