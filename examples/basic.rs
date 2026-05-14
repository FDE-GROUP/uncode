use uncode_core::message::{ContentBlock, Message, Role};
use uncode_core::tool::ToolDefinition;

fn main() {
    let msg = Message::user("Hello from uncode!");
    println!("Created message: {:?}", msg.role);

    let tool_def = ToolDefinition {
        name: "example".into(),
        description: "An example tool".into(),
        parameters: serde_json::json!({"type": "object"}),
    };
    println!("Tool definition: {}", tool_def.name);
    println!("uncode is ready!");
}
