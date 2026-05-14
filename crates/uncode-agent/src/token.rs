use uncode_core::message::{ContentBlock, Message};

const CHARS_PER_TOKEN: f32 = 3.5;

/// 估算文本的 token 数量（字符数/3.5）
pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as f32 / CHARS_PER_TOKEN).ceil() as u64
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
            ContentBlock::Image { .. } => 200, // rough estimate for images
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
