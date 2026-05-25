use uncode_core::message::{ContentBlock, Message};

/// 估算文本的 token 数量（字符数/3.5）
pub fn estimate_tokens(text: &str) -> u64 {
    let char_count = text.chars().count();
    // Integer division ceiling: (a + b - 1) / b
    let tokens = (char_count * 10).div_ceil(35); // char_count / 3.5 ≈ char_count * 10 / 35
    tokens as u64
}

/// 估算单条消息的 token 数量
pub fn estimate_message_tokens(msg: &Message) -> u64 {
    let mut total: u64 = 0;
    for block in &msg.content {
        total += match block {
            ContentBlock::Text { text } => estimate_tokens(text),
            ContentBlock::Thinking { text } => estimate_tokens(text),
            ContentBlock::ToolCall(tc) => {
                let args = serde_json::to_string(&tc.arguments).unwrap_or_default();
                estimate_tokens(&tc.name) + estimate_tokens(&args)
            }
            ContentBlock::ToolResult(tr) => estimate_tokens(&tr.content),
            ContentBlock::Image { .. } => 200,
            _ => 0,
        };
    }
    total
}

/// 估算 LLM 调用费用（基于7个模型的定价表）
pub fn estimate_cost(input_tokens: u64, output_tokens: u64, model: &str) -> f64 {
    let (input_price, output_price) = get_pricing(model);
    (input_tokens as f64 / 1000.0 * input_price) + (output_tokens as f64 / 1000.0 * output_price)
}

fn get_pricing(model: &str) -> (f64, f64) {
    match model {
        m if m.contains("deepseek") => (0.27, 1.10),
        m if m.contains("glm") => (0.10, 0.10),
        m if m.contains("gpt-4") => (30.0, 60.0),
        m if m.contains("gpt-3.5") => (0.50, 1.50),
        m if m.contains("claude") => (15.0, 75.0),
        m if m.contains("gemini") => (2.50, 10.0),
        _ => (1.0, 2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_core::message::{Role, ToolCall};

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_short() {
        let tokens = estimate_tokens("hello");
        assert!(tokens > 0);
        assert!(tokens < 5);
    }

    #[test]
    fn test_estimate_tokens_longer() {
        let short = estimate_tokens("hello");
        let long = estimate_tokens("hello world, this is a longer text");
        assert!(long > short);
    }

    #[test]
    fn test_estimate_message_tokens_text() {
        let msg = Message::user("hello world");
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_message_tokens_thinking() {
        let msg = Message::new(
            Role::User,
            vec![ContentBlock::Thinking {
                text: "thinking about the solution".into(),
            }],
        );
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_message_tokens_tool_call() {
        let msg = Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolCall(Box::new(ToolCall {
                id: "1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "/foo/bar.rs"}),
            }))],
        );
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_cost_deepseek() {
        let cost = estimate_cost(1000, 500, "deepseek-chat");
        assert!((cost - 0.82).abs() < 0.01);
    }

    #[test]
    fn test_estimate_cost_unknown_model() {
        let cost = estimate_cost(1000, 500, "nonexistent-model");
        assert!((cost - 2.0).abs() < 0.01);
    }
}
