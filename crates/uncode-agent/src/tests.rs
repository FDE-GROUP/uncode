#[cfg(test)]
mod tests {
    use uncode_core::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
    use uncode_core::tool::ToolDefinition;

    use crate::compaction::{estimate_context_tokens, extract_text, should_compact};
    use crate::system_prompt::SystemPromptBuilder;
    use crate::token;

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
        // 4 chars = 1 token (EST_CHARS_PER_TOKEN = 4)
        let msg = Message::user("abcd");
        assert_eq!(estimate_context_tokens(&[msg]), 1);
    }

    #[test]
    fn test_estimate_context_tokens_partial_division() {
        // 5 chars / 4 = 1.25 -> ceil = 2
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
        // 80 tokens exactly = at threshold, should NOT compact (> not >=)
        let text = "a".repeat(320); // 320 / 4 = 80 tokens
        let msg = Message::user(text);
        assert!(!should_compact(&[msg], 100));
    }

    #[test]
    fn test_should_compact_just_above_80_percent() {
        // 81 tokens = just above threshold
        let text = "a".repeat(324); // 324 / 4 = 81 tokens
        let msg = Message::user(text);
        assert!(should_compact(&[msg], 100));
    }

    #[test]
    fn test_extract_text_utf8_truncation_safe() {
        // 中文字符每个 3 bytes，200 bytes 不会在字符中间切割
        let chinese = "你".repeat(100); // 300 bytes of Chinese
        let msg = Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult {
                tool_call_id: "c1".into(),
                content: chinese,
                is_error: false,
            })],
        );
        // extract_text 不应 panic
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
        assert_eq!(tokens, 2); // 5 chars / 3.5 = 1.42 -> ceil 2
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
        assert!((cost - 1.37).abs() < 0.01); // (1000/1000*0.27) + (1000/1000*1.10) = 1.37
    }

    #[test]
    fn test_token_cost_unknown_model() {
        let cost = token::estimate_cost(1000, 1000, "unknown-model");
        assert!((cost - 3.0).abs() < 0.01); // default: (1*1) + (1*2) = 3.0
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
        assert_eq!(tokens, 200); // fixed estimate for images
    }
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
    use uncode_core::message::Message;

    let condition = crate::stop::text_contains("DONE");
    let empty: Vec<Message> = vec![];
    assert!(condition.should_stop(0, &empty).is_none());

    let msg = Message::assistant("task DONE");
    assert!(condition.should_stop(0, &[msg]).is_some());
}
