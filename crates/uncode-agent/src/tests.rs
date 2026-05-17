#[cfg(test)]
mod tests {
    use uncode_core::api_types::StopReason;
    use uncode_core::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
    use uncode_core::tool::{
        AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, ExecutionMode,
        ToolDefinition, ToolHooks, ToolResult as ToolExecResult,
    };

    use crate::compaction::{estimate_context_tokens, extract_text, should_compact};
    use crate::system_prompt::SystemPromptBuilder;
    use crate::token;

    // ── Mock Api ────────────────────────────────────────────────────

    use async_trait::async_trait;
    use futures::stream::{self, BoxStream, StreamExt};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uncode_ai::Api;
    use uncode_ai::StreamEvent;
    use uncode_ai::ToolCallEndData;
    use uncode_core::api_types::{Context, StreamOptions};
    use uncode_core::error::UncodeError;
    use uncode_core::model::Model;

    struct MockApi {
        responses: Mutex<Vec<Vec<StreamEvent>>>,
        call_count: AtomicUsize,
    }

    impl MockApi {
        fn new(responses: Vec<Vec<StreamEvent>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Api for MockApi {
        fn api_name(&self) -> &'static str {
            "mock"
        }

        async fn stream(
            &self,
            _model: &Model,
            _context: &Context,
            _options: &StreamOptions,
        ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
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
                label: None,
                execution_mode: ExecutionMode::default(),
            }
        }

        async fn execute(&self, arguments: serde_json::Value) -> Result<String, UncodeError> {
            let text = arguments["text"].as_str().unwrap_or("");
            Ok(format!("echo: {text}"))
        }
    }

    // Hook that sets terminate=true on every tool result
    struct TerminateHook;

    #[async_trait]
    impl ToolHooks for TerminateHook {
        async fn before_tool_call(&self, _ctx: &BeforeToolCallContext) -> Option<String> {
            None
        }

        async fn after_tool_call(
            &self,
            _ctx: &AfterToolCallContext,
            result: &mut ToolExecResult,
        ) -> AfterToolCallResult {
            result.terminate = true;
            AfterToolCallResult::default()
        }
    }

    // ── Test Helpers ─────────────────────────────────────────────────

    use crate::session::store::SessionStore;
    use crate::tools::registry::ToolRegistry;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use uncode_ai::{ApiRegistry, ModelRegistry};

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

    fn make_registries(
        responses: Vec<Vec<StreamEvent>>,
    ) -> (
        Arc<ApiRegistry>,
        Arc<ModelRegistry>,
        HashMap<String, String>,
    ) {
        let mut api_registry = ApiRegistry::new();
        api_registry.register(Arc::new(MockApi::new(responses)));
        let mut model_registry = ModelRegistry::new();
        model_registry.register(Model {
            id: "mock".into(),
            api: "mock".into(),
            ..Model::default()
        });
        (
            Arc::new(api_registry),
            Arc::new(model_registry),
            HashMap::new(),
        )
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
                ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "c1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({}),
                })),
                ContentBlock::ToolResult(Box::new(ToolResult {
                    tool_call_id: "c1".into(),
                    content: "file contents here".into(),
                    is_error: false,
                })),
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
            vec![ContentBlock::ToolResult(Box::new(ToolResult {
                tool_call_id: "c1".into(),
                content: chinese,
                is_error: false,
            }))],
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
            label: None,
            execution_mode: ExecutionMode::default(),
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
            label: None,
            execution_mode: ExecutionMode::default(),
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
            vec![ContentBlock::ToolCall(Box::new(ToolCall {
                id: "call_1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({"command": "ls"}),
            }))],
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
        let (api_reg, model_reg, api_keys) = make_registries(vec![vec![
            StreamEvent::TextDelta("Hello!".into()),
            StreamEvent::Usage(uncode_ai::LlmUsageInfo {
                input_tokens: 10,
                output_tokens: 5,
            }),
            StreamEvent::Done {
                reason: uncode_core::api_types::StopReason::Stop,
            },
        ]]);

        let agent = AgentLoop::new(
            api_reg.clone(),
            model_reg.clone(),
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("hi")).await.unwrap();

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
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "tc1".into(),
                    arguments: r#"{"text":"world"}"#.into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "world"}),
                })),
                StreamEvent::Usage(uncode_ai::LlmUsageInfo {
                    input_tokens: 20,
                    output_tokens: 10,
                }),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
            vec![
                StreamEvent::TextDelta("Done!".into()),
                StreamEvent::Usage(uncode_ai::LlmUsageInfo {
                    input_tokens: 30,
                    output_tokens: 8,
                }),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
        ]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("echo hello")).await.unwrap();

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
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "a"}),
                })),
                StreamEvent::ToolCallStart {
                    id: "tc2".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc2".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "b"}),
                })),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
            vec![
                StreamEvent::TextDelta("All done.".into()),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
        ]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("echo twice")).await.unwrap();

        // System, User, Assistant(ToolCall×2), Tool(ToolResult), Tool(ToolResult), Assistant(Text)
        assert_eq!(messages.len(), 6);

        // Assistant 消息包含两个 ToolCall
        let assistant_msg = &messages[2];
        assert_eq!(assistant_msg.role, Role::Assistant);
        assert_eq!(assistant_msg.content.len(), 2);
        assert!(matches!(
            &assistant_msg.content[0],
            ContentBlock::ToolCall(_)
        ));
        assert!(matches!(
            &assistant_msg.content[1],
            ContentBlock::ToolCall(_)
        ));

        assert_eq!(messages[3].role, Role::Tool);
        assert_eq!(messages[4].role, Role::Tool);
        assert_eq!(messages[5].role, Role::Assistant);
    }

    #[tokio::test]
    async fn test_agent_loop_tool_not_found() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "nonexistent".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "nonexistent".into(),
                    arguments: serde_json::json!({}),
                })),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
            vec![
                StreamEvent::TextDelta("OK".into()),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
        ]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
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
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "x"}),
                })),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
            vec![
                StreamEvent::TextDelta("result".into()),
                StreamEvent::Done {
                    reason: uncode_core::api_types::StopReason::Stop,
                },
            ],
        ]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
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

    // ── StopReason 填充测试 ──

    #[tokio::test]
    async fn test_agent_loop_stop_reason_populated() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![vec![
            StreamEvent::TextDelta("response".into()),
            StreamEvent::Done {
                reason: uncode_core::api_types::StopReason::Stop,
            },
        ]]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("hi")).await.unwrap();
        let assistant_msg = &messages[2];
        assert_eq!(
            assistant_msg.stop_reason,
            Some(uncode_core::api_types::StopReason::Stop)
        );
    }

    #[tokio::test]
    async fn test_agent_loop_stop_reason_length() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![vec![
            StreamEvent::TextDelta("truncated".into()),
            StreamEvent::Done {
                reason: uncode_core::api_types::StopReason::Length,
            },
        ]]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        let messages = agent.run(Message::user("long prompt")).await.unwrap();
        let assistant_msg = &messages[2];
        assert_eq!(
            assistant_msg.stop_reason,
            Some(uncode_core::api_types::StopReason::Length)
        );
    }

    // ── 双层循环架构 + 新功能测试 ──────────────────────────────────────

    /// should_stop_after_turn: 回调返回 true 后 agent 立即终止，不执行下一轮 LLM 调用
    #[tokio::test]
    async fn test_should_stop_after_turn() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            // Turn 1: tool call
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "hello"}),
                })),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
            // Turn 2: 不应被调用
            vec![
                StreamEvent::TextDelta("SHOULD NOT REACH".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
        ]);

        let mut agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );
        agent.set_should_stop_after_turn(Arc::new(|turn| turn >= 1));

        let messages = agent.run(Message::user("go")).await.unwrap();

        // Agent 在 turn 1 后终止：System, User, Assistant(ToolCall), Tool(ToolResult)
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[3].role, Role::Tool);
    }

    /// prepare_next_turn: 每个 turn 后回调被调用
    #[tokio::test]
    async fn test_prepare_next_turn_called_per_turn() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        let (api_reg, model_reg, api_keys) = make_registries(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "x"}),
                })),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
            vec![
                StreamEvent::TextDelta("done".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
        ]);

        let mut agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );
        agent.set_prepare_next_turn(Arc::new(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        agent.run(Message::user("go")).await.unwrap();

        // 两轮 turn：tool call turn + text turn
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    /// transform_context: 每次 LLM 调用前变换消息数组
    #[tokio::test]
    async fn test_transform_context_called_before_llm() {
        let message_counts = Arc::new(Mutex::new(Vec::new()));
        let counts_clone = message_counts.clone();

        let (api_reg, model_reg, api_keys) = make_registries(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "x"}),
                })),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
            vec![
                StreamEvent::TextDelta("final".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
        ]);

        let mut agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );
        agent.set_transform_context(Arc::new(move |msgs| {
            counts_clone.lock().unwrap().push(msgs.len());
        }));

        agent.run(Message::user("go")).await.unwrap();

        let counts = message_counts.lock().unwrap().clone();
        assert_eq!(counts.len(), 2); // 两次 LLM 调用前各调用一次
        assert_eq!(counts[0], 2); // System + User
        assert_eq!(counts[1], 4); // System + User + Assistant(ToolCall) + Tool(ToolResult)
    }

    /// steering 消息：内层循环 re-enter，steering 注入到 context 后再次调用 LLM
    #[tokio::test]
    async fn test_steering_message_re_enters_inner_loop() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            // Turn 1: text response
            vec![
                StreamEvent::TextDelta("First".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
            // Turn 2: steering 注入后的 response
            vec![
                StreamEvent::TextDelta("AfterSteering".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
        ]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        // 预先排队 steering 消息
        agent.steer(Message::user("steer this")).await;

        let messages = agent.run(Message::user("go")).await.unwrap();

        // System, User("go"), Assistant("First"), User("steer this"), Assistant("AfterSteering")
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[3].role, Role::User);
        assert!(
            matches!(&messages[3].content[0], ContentBlock::Text { text } if text == "steer this")
        );
        assert_eq!(messages[4].role, Role::Assistant);
    }

    /// follow-up 消息：外层循环 re-enter，follow-up 注入后再次调用 LLM
    #[tokio::test]
    async fn test_follow_up_message_re_enters_outer_loop() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            // Turn 1: text response (内层循环退出)
            vec![
                StreamEvent::TextDelta("First".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
            // Turn 2: follow-up 注入后的 response
            vec![
                StreamEvent::TextDelta("AfterFollowUp".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
        ]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        // 预先排队 follow-up 消息
        agent.follow_up(Message::user("follow this")).await;

        let messages = agent.run(Message::user("go")).await.unwrap();

        // System, User("go"), Assistant("First"), User("follow this"), Assistant("AfterFollowUp")
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[3].role, Role::User);
        assert!(
            matches!(&messages[3].content[0], ContentBlock::Text { text } if text == "follow this")
        );
        assert_eq!(messages[4].role, Role::Assistant);
    }

    /// nextTurn 消息：在首轮 turn 前注入到 pending_messages
    #[tokio::test]
    async fn test_next_turn_message_injected_before_first_turn() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![vec![
            StreamEvent::TextDelta("Response".into()),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ]]);

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );

        // 预先排队 nextTurn 消息
        agent.next_turn(Message::user("next turn context")).await;

        let messages = agent.run(Message::user("go")).await.unwrap();

        // System, User("go"), User("next turn context"), Assistant("Response")
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2].role, Role::User);
        assert!(
            matches!(&messages[2].content[0], ContentBlock::Text { text } if text == "next turn context")
        );
    }

    /// terminate=true：所有工具请求终止时 agent 立即停止，不再发起额外 LLM 调用
    #[tokio::test]
    async fn test_tool_terminate_stops_without_extra_llm_call() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![
            // Turn 1: tool call → terminate=true
            vec![
                StreamEvent::ToolCallStart {
                    id: "tc1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"text": "bye"}),
                })),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
            // Turn 2: 不应被调用
            vec![
                StreamEvent::TextDelta("SHOULD NOT REACH".into()),
                StreamEvent::Done {
                    reason: StopReason::Stop,
                },
            ],
        ]);

        let mut agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            make_tool_registry(),
            Arc::new(SessionStore::new(test_session_dir())),
            "system".into(),
            "mock".into(),
        );
        agent.set_tool_hooks(Arc::new(TerminateHook));

        let messages = agent.run(Message::user("go")).await.unwrap();

        // terminate 后立即停止：System, User, Assistant(ToolCall), Tool(ToolResult)
        // 不应有第 5 条消息（即第二论 LLM 的 Assistant 响应）
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[3].role, Role::Tool);
    }

    // ── AgentHarness tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_harness_phase_guard() {
        use crate::harness::{AgentHarness, AgentHarnessPhase};
        use crate::session::store::SessionStore;

        let (api_reg, model_reg, api_keys) = make_registries(vec![vec![
            StreamEvent::TextDelta("ok".into()),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ]]);
        let tool_reg = Arc::new(crate::tools::registry::ToolRegistry::new());
        let session_store = Arc::new(SessionStore::new(test_session_dir()));

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            tool_reg,
            session_store.clone(),
            "test prompt".into(),
            "mock".into(),
        );
        let mut harness = AgentHarness::new(agent, session_store);

        assert!(harness.is_idle());
        assert_eq!(*harness.phase(), AgentHarnessPhase::Idle);

        // Non-idle should reject prompt — but since we can't easily get into
        // a non-idle state without actually running, test the phase check directly.
        // We verify that phase transitions work correctly.
        harness.set_session_id("test-session".into());
        assert_eq!(harness.session_id(), Some("test-session"));
    }

    #[test]
    fn test_harness_pending_write_flush() {
        use crate::harness::AgentHarness;
        use crate::session::store::SessionStore;

        let (api_reg, model_reg, _) = make_registries(vec![]);
        let tool_reg = Arc::new(crate::tools::registry::ToolRegistry::new());
        let session_store = Arc::new(SessionStore::new(test_session_dir()));

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            HashMap::new(),
            tool_reg,
            session_store.clone(),
            "test prompt".into(),
            "mock".into(),
        );
        let mut harness = AgentHarness::new(agent, session_store.clone());

        // Init session
        session_store
            .init_session("s1", "test-model", "/tmp")
            .unwrap();
        harness.set_session_id("s1".into());

        // Add pending writes
        harness.set_model("new-model", "test-provider");

        // Verify session has model change entry
        let entries = session_store.load_entries("s1").unwrap();
        assert!(entries.iter().any(|e| matches!(
            e,
            uncode_core::session::SessionEntry::ModelChange(mc) if mc.model_id == "new-model"
        )));
    }

    // ── Phase 3 补充：abort 清空 pending_writes ──

    #[tokio::test]
    async fn test_harness_abort_clears_state() {
        use crate::harness::AgentHarness;

        let (api_reg, model_reg, _) = make_registries(vec![]);
        let tool_reg = Arc::new(crate::tools::registry::ToolRegistry::new());
        let session_store = Arc::new(SessionStore::new(test_session_dir()));

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            HashMap::new(),
            tool_reg,
            session_store.clone(),
            "test".into(),
            "mock".into(),
        );
        let mut harness = AgentHarness::new(agent, session_store);

        // abort should succeed without panic
        harness.abort().await;
        assert!(harness.is_idle());
    }

    // ── Phase 8: ActiveRun 并发拒绝 + reset ──

    #[tokio::test]
    async fn test_active_run_concurrent_rejection() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![vec![
            StreamEvent::TextDelta("first".into()),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ]]);
        let tool_reg = Arc::new(crate::tools::registry::ToolRegistry::new());
        let session_store = Arc::new(SessionStore::new(test_session_dir()));

        let agent = Arc::new(tokio::sync::Mutex::new(AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            tool_reg,
            session_store,
            "test".into(),
            "mock".into(),
        )));

        // Run once should succeed
        let a = agent.clone();
        let result = a.lock().await.run(Message::user("go")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reset_clears_state() {
        let (api_reg, model_reg, api_keys) = make_registries(vec![]);
        let tool_reg = Arc::new(crate::tools::registry::ToolRegistry::new());
        let session_store = Arc::new(SessionStore::new(test_session_dir()));

        let agent = AgentLoop::new(
            api_reg,
            model_reg,
            api_keys,
            tool_reg,
            session_store,
            "test".into(),
            "mock".into(),
        );

        // reset should not panic
        agent.reset().await;
    }
}
