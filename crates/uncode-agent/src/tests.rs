#[cfg(test)]
mod tests {
    use uncode_core::message::Message;
    use uncode_core::tool::ToolDefinition;

    use crate::compaction::{estimate_context_tokens, should_compact};
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
