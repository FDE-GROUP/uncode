#[cfg(test)]
mod tests {
    use uncode_core::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
    use uncode_core::tool::ToolDefinition;

    use crate::compaction::{estimate_context_tokens, extract_text, should_compact};
    use crate::system_prompt::SystemPromptBuilder;
    use crate::token;

    // ── Mock LLM Driver ──────────────────────────────────────────────

    use async_trait::async_trait;
    use futures::stream;
    use futures::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use uncode_core::error::UncodeError;
    use uncode_llm::driver::{
        CompletionRequest, LlmDriver, StreamEvent, UsageInfo as LlmUsageInfo,
    };

    struct MockLlmDriver {
        responses: Mutex<Vec<Vec<StreamEvent>>>,
        call_count: AtomicUsize,
    }

    impl MockLlmDriver {
        fn new(responses: Vec<Vec<StreamEvent>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmDriver for MockLlmDriver {
        fn provider_name(&self) -> &'static str {
            "mock"
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<futures::stream::BoxStream<'static, StreamEvent>, UncodeError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let events = self
                .responses
                .lock()
                .unwrap()
                .get(idx)
                .cloned()
                .unwrap_or_default();
            Ok(stream::iter(events).boxed())
        }
    }

    // ── Mock Tool ────────────────────────────────────────────────────

    struct EchoTool;

    #[async_trait]
    impl uncode_core::tool::ToolExecutor for EchoTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "echo".into(),
                description: "echo back input".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "text": {"type": "string"} },
                    "required": ["text"]
                }),
            }
        }

        async fn execute(
            &self,
            arguments: serde_json::Value,
        ) -> Result<String, UncodeError> {
            let text = arguments["text"].as_str().unwrap_or("");
            Ok(format!("echo: {text}"))
        }
    }

    // ── Test Helpers ─────────────────────────────────────────────────

    use std::path::PathBuf;
    use std::sync::Arc;
    use uncode_session::store::SessionStore;
    use uncode_tools::registry::ToolRegistry;

    use crate::loop_engine::AgentLoop;

    fn test_session_dir() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("uncode-test-loop-{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_tool_registry() -> Arc<ToolRegistry> {
        let reg = Arc::new(ToolRegistry::new());
        reg.register("echo".to_string(), Arc::new(EchoTool));
        reg
    }

    // ── 纯函数测试 ──────────────────────────────────────────────────

    #[test]
    fn test_estimate_context_tokens_empty() {
        assert_eq!(estimate_context_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_context_tokens_text() {
        let msg = Message::user("hello world this is a test");
        let tokens = estimate_context_tokens(&[msg]);
        assert!(tokens > 0);
        assert!(tokens < 20);
    }

    #[test]
    fn test_estimate_context_tokens_exact_division() {
        let msg = Message::user("abcd");
        assert_eq!(estimate_context_tokens(&[msg]), 1);
    }

    #[test]
    fn test_estimate_context_tokens_partial_division() {
        let msg = Message::user("abcde");
        assert_eq!(estimate_context_tokens(&[msg]), 2);
    }

    #[test]
    fn test_estimate_context_tokens_mixed_blocks() {
        let msg = Message::new(
            Role::User,
            vec![
                ContentBlock::Text {
                    text: "hello".into(),
                },
                ContentBlock::Thinking {
                    text: "reasoning".into(),
                },
                ContentBlock::ToolCall(ToolCall {
                    id: "c1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({}),
                }),
                ContentBlock::ToolResult(ToolResult {
                    tool_call_id: "c1".into(),
                    content: "file contents here".into(),
                    is_error: false,
                }),
            ],
        );
        let tokens = estimate_context_tokens(&[msg]);
        assert!(tokens > 0);
    }

    #[test]
    fn test_should_compact_below_threshold() {
        let msg = Message::user("hello");
        assert!(!should_compact(&[msg], 100));
    }

    #[test]
    fn test_should_compact_above_threshold() {
        let long_text = "x".repeat(1000);
        let msg = Message::user(long_text);
        assert!(should_compact(&[msg], 100));
    }

    #[test]
    fn test_should_compact_exactly_at_80_percent() {
        let text = "a".repeat(320);
        let msg = Message::user(text);
        assert!(!should_compact(&[msg], 100));
    }

    #[test]
    fn test_should_compact_just_above_80_percent() {
        let text = "a".repeat(324);
        let msg = Message::user(text);
        assert!(should_compact(&[msg], 100));
    }

    #[test]
    fn test_extract_text_utf8_truncation_safe() {
        let chinese = "你".repeat(100);
        let msg = Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult {
                tool_call_id: "c1".into(),
                content: chinese,
                is_error: false,
            })],
        );
        let text = extract_text(&msg.content);
        assert!(!text.is_empty());
    }

    #[test]
    fn test_extract_text_empty_content() {
        let blocks: Vec<ContentBlock> = vec![];
        let text = extract_text(&blocks);
        assert!(text.is_empty());
    }

    #[test]
    fn test_extract_text_only_thinking() {
        let blocks = vec![ContentBlock::Thinking {
            text: "internal".into(),
        }];
        let text = extract_text(&blocks);
        assert!(text.is_empty());
    }

    #[test]
    fn test_system_prompt_builder_empty() {
        let prompt = SystemPromptBuilder::new().build();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_system_prompt_builder_with_base() {
        let prompt = SystemPromptBuilder::new().base("hello").build();
        assert_eq!(prompt, "hello");
    }

    #[test]
    fn test_system_prompt_builder_with_tools() {
        let tools = vec![ToolDefinition {
            name: "read".into(),
            description: "read files".into(),
            parameters: serde_json::json!({}),
        }];
        let prompt = SystemPromptBuilder::new().add_tool_guide(&tools).build();
        assert!(prompt.contains("read"));
        assert!(prompt.contains("read files"));
    }

    #[test]
    fn test_system_prompt_builder_with_all_sections() {
        let tools = vec![ToolDefinition {
            name: "bash".into(),
            description: "run commands".into(),
            parameters: serde_json::json!({}),
        }];
        let skills = vec![("git-release".into(), "create releases".into())];
        let prompt = SystemPromptBuilder::new()
            .base("you are an AI")
            .add_tool_guide(&tools)
            .add_context("project context")
            .add_skills(&skills)
            .add_rules("no unsafe code")
            .build();
        assert!(prompt.contains("you are an AI"));
        assert!(prompt.contains("bash"));
        assert!(prompt.contains("project context"));
        assert!(prompt.contains("git-release"));
        assert!(prompt.contains("no unsafe code"));
    }

    #[test]
    fn test_system_prompt_builder_empty_tools_skipped() {
        let prompt = SystemPromptBuilder::new().add_tool_guide(&[]).build();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_system_prompt_builder_empty_context_skipped() {
        let prompt = SystemPromptBuilder::new().add_context("").build();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_token_estimate_empty() {
        assert_eq!(token::estimate_tokens(""), 0);
    }

    #[test]
    fn test_token_estimate_short() {
        let tokens = token::estimate_tokens("hello");
        assert_eq!(tokens, 2);
    }

    #[test]
    fn test_token_estimate_message() {
        let msg = Message::assistant("hello world");
        let tokens = token::estimate_message_tokens(&msg);
        assert!(tokens >= 3);
    }

    #[test]
    fn test_token_cost_deepseek() {
        let cost = token::estimate_cost(1000, 1000, "deepseek-v3");
        assert!((cost - 1.37).abs() < 0.01);
    }

    #[test]
    fn test_token_cost_unknown_model() {
        let cost = token::estimate_cost(1000, 1000, "unknown-model");
        assert!((cost - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_token_cost_zero_tokens() {
        let cost = token::estimate_cost(0, 0, "deepseek-v3");
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_token_estimate_message_with_tool_call() {
        let msg = Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolCall(ToolCall {
                id: "call_1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({"command": "ls"}),
            })],
        );
        let tokens = token::estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_token_estimate_message_with_image() {
        let msg = Message::new(
            Role::User,
            vec![ContentBlock::Image {
                mime_type: "image/png".into(),
                data: "base64data".into(),
            }],
        );
        let tokens = token::estimate_message_tokens(&msg);
        assert_eq!(tokens, 200);
    }

    #[test]
    fn test_step_count_is() {
        let condition = crate::stop::step_count_is(3);
        assert!(condition.should_stop(3, &[]).is_some());
        assert!(condition.should_stop(2, &[]).is_none());
        assert!(condition.should_stop(0, &[]).is_none());
    }

    #[test]
    fn test_text_contains_stop() {
        let condition = crate::stop::text_contains("DONE");
        let empty: Vec<Message> = vec![];
        assert!(condition.should_stop(0, &empty).is_none());

        let msg = Message::assistant("task DONE");
        assert!(condition.should_stop(0, &[msg]).is_some());
    }

    // ── AgentLoop 集成测试 ──────────────────────────────────────────

    #[tokio::test]
    async fn test_agent_loop_text_only() {
        let driver = Arc::new(MockLlmDriver::new(vec![vec![
            StreamEvent::TextDelta("Hello!".into()),
            StreamEvent::Usage(LlmUsageInfo {
                input_tokens: 10,
                output_tokens: 5,
            }),
            StreamEvent::Done,
        ]]));

        let agent = AgentLoop::new(
            driver.clone(),
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("hi")).await.unwrap();

        assert_eq!(driver.call_count(), 1);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[2].role, Role::Assistant);
        assert!(matches!(
            &messages[2].content[0],
            ContentBlock::Text { text } if text == "Hello!"
        ));
    }

    #[tokio::test]
    async fn test_agent_loop_tool_call_then_text() {
        let driver = Arc::new(MockLlmDriver::new(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "tc1".into(),
                    arguments: r#"{"text":"world"}"#.into(),
                },
                StreamEvent::ToolCallEnd {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "world"}),
                },
                StreamEvent::Usage(LlmUsageInfo {
                    input_tokens: 20,
                    output_tokens: 10,
                }),
                StreamEvent::Done,
            ],
            vec![
                StreamEvent::TextDelta("Done!".into()),
                StreamEvent::Usage(LlmUsageInfo {
                    input_tokens: 30,
                    output_tokens: 8,
                }),
                StreamEvent::Done,
            ],
        ]));

        let agent = AgentLoop::new(
            driver.clone(),
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("echo hello")).await.unwrap();

        assert_eq!(driver.call_count(), 2);
        // System, User, Assistant(ToolCall), Tool(ToolResult), Assistant(Text)
        assert_eq!(messages.len(), 5);

        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);

        // messages[2] = Assistant 包含 ToolCall
        assert_eq!(messages[2].role, Role::Assistant);
        assert!(
            matches!(&messages[2].content[0], ContentBlock::ToolCall(tc) if tc.name == "echo"),
            "expected Assistant with ToolCall, got {:?}",
            messages[2].content
        );

        // messages[3] = Tool 包含 ToolResult
        assert_eq!(messages[3].role, Role::Tool);
        assert!(
            matches!(
                &messages[3].content[0],
                ContentBlock::ToolResult(tr) if tr.content == "echo: world"
            ),
            "expected Tool with ToolResult, got {:?}",
            messages[3].content
        );

        // messages[4] = Assistant 包含 Text
        assert_eq!(messages[4].role, Role::Assistant);
        assert!(
            matches!(&messages[4].content[0], ContentBlock::Text { text } if text == "Done!"),
            "expected Assistant with Text, got {:?}",
            messages[4].content
        );
    }

    #[tokio::test]
    async fn test_agent_loop_multiple_tool_calls_in_one_turn() {
        let driver = Arc::new(MockLlmDriver::new(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "a"}),
                },
                StreamEvent::ToolCallStart {
                    id: "tc2".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd {
                    id: "tc2".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "b"}),
                },
                StreamEvent::Done,
            ],
            vec![
                StreamEvent::TextDelta("All done.".into()),
                StreamEvent::Done,
            ],
        ]));

        let agent = AgentLoop::new(
            driver.clone(),
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("echo twice")).await.unwrap();

        assert_eq!(driver.call_count(), 2);
        // System, User, Assistant(ToolCall×2), Tool(ToolResult), Tool(ToolResult), Assistant(Text)
        assert_eq!(messages.len(), 6);

        // Assistant 消息包含两个 ToolCall
        let assistant_msg = &messages[2];
        assert_eq!(assistant_msg.role, Role::Assistant);
        assert_eq!(assistant_msg.content.len(), 2);
        assert!(matches!(&assistant_msg.content[0], ContentBlock::ToolCall(_)));
        assert!(matches!(&assistant_msg.content[1], ContentBlock::ToolCall(_)));

        assert_eq!(messages[3].role, Role::Tool);
        assert_eq!(messages[4].role, Role::Tool);
        assert_eq!(messages[5].role, Role::Assistant);
    }

    #[tokio::test]
    async fn test_agent_loop_tool_not_found() {
        let driver = Arc::new(MockLlmDriver::new(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "nonexistent".into(),
                },
                StreamEvent::ToolCallEnd {
                    id: "tc1".into(),
                    name: "nonexistent".into(),
                    arguments: serde_json::json!({}),
                },
                StreamEvent::Done,
            ],
            vec![
                StreamEvent::TextDelta("OK".into()),
                StreamEvent::Done,
            ],
        ]));

        let agent = AgentLoop::new(
            driver.clone(),
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("test")).await.unwrap();

        let tool_msg = &messages[3];
        assert_eq!(tool_msg.role, Role::Tool);
        if let ContentBlock::ToolResult(tr) = &tool_msg.content[0] {
            assert!(tr.content.contains("not found"));
        } else {
            panic!("expected ToolResult");
        }
    }

    #[tokio::test]
    async fn test_agent_loop_message_sequence_integrity() {
        let driver = Arc::new(MockLlmDriver::new(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "x"}),
                },
                StreamEvent::Done,
            ],
            vec![
                StreamEvent::TextDelta("result".into()),
                StreamEvent::Done,
            ],
        ]));

        let agent = AgentLoop::new(
            driver,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("go")).await.unwrap();

        // 验证每条 Tool 消息前面一定是 Assistant 消息
        for i in 1..messages.len() {
            let prev = &messages[i - 1];
            let curr = &messages[i];

            if curr.role == Role::Tool {
                assert_eq!(
                    prev.role,
                    Role::Assistant,
                    "Tool message at index {i} must follow Assistant, got {:?}",
                    prev.role
                );
            }
        }
    }
}
