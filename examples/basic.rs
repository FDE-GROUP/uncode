//! uncode 基础示例（最小化演示）
//!
//! 此文件演示 uncode-core 的基本数据类型用法。
//!
//! ⚠️ 由于 workspace 根目录是 virtual manifest，示例必须放在具体的 crate 内才能编译。
//! 完整的 Agent 演示请参考：`crates/uncode-cli/examples/agent_demo.rs`
//!
//! 运行 Agent 完整示例：
//! ```bash
//! cargo run --example agent_demo
//! ```

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
    println!();
    println!("💡 运行完整的 Agent 演示：cargo run --example agent_demo");
}
