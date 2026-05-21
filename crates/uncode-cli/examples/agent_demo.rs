//! uncode Agent 简单示例
//!
//! 本示例展示如何使用 uncode 框架构建和运行一个 AI Agent。
//! 包含以下内容：
//!
//!   1. 消息创建（Message / ContentBlock）
//!   2. 自定义工具实现（ToolExecutor）
//!   3. 工具注册表（ToolRegistry）
//!   4. 系统提示词构建（SystemPromptBuilder）
//!   5. 模拟 API 后端（用于无 API Key 的本地演示）
//!   6. AgentLoop 创建与运行
//!   7. 事件订阅与监听
//!   8. AgentHarness 高级编排
//!
//! 在项目根目录下编译运行：
//! ```bash
//! cargo run --example agent_demo
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use tokio::sync::broadcast;

// uncode-ai        — LLM 抽象层（Api trait、Model、ApiRegistry、ModelRegistry）
use uncode_ai::{Api, ApiRegistry, LlmUsageInfo, ModelRegistry, StreamEvent, ToolCallEndData};
// uncode-core      — 共享类型（消息、工具定义、事件、配置等）
use uncode_core::api_types::{Context, StopReason, StreamOptions};
use uncode_core::error::UncodeError;
use uncode_core::event::AgentEvent;
use uncode_core::message::{ContentBlock, Message, Role, ToolCall, ToolResult as MsgToolResult};
use uncode_core::model::Model;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

// uncode-agent     — 代理引擎（AgentLoop、SystemPromptBuilder、ToolRegistry 等）
use uncode_agent::harness::AgentHarness;
use uncode_agent::session::store::SessionStore;
use uncode_agent::tools::registry::ToolRegistry;
use uncode_agent::tools::{BashTool, EditTool, FindTool, GrepTool, LsTool, ReadTool, WriteTool};
use uncode_agent::{AgentLoop, SystemPromptBuilder};

// ═══════════════════════════════════════════════════════════════════════
// 第一部分：自定义工具
// ═══════════════════════════════════════════════════════════════════════

/// 一个简单的计算器工具，评估数学表达式。
struct CalculatorTool;

#[async_trait]
impl ToolExecutor for CalculatorTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "calculator".to_string(),
            description: "计算数学表达式，支持 +、-、*、/ 和括号".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "数学表达式，如 \"2 + 3 * 4\""
                    }
                },
                "required": ["expression"]
            }),
            label: Some("🧮 计算".to_string()),
            execution_mode: ExecutionMode::Parallel,
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, UncodeError> {
        let expr = arguments["expression"].as_str().unwrap_or("0");
        let result = evaluate_expression(expr);
        Ok(format!("计算结果: {expr} = {result}"))
    }
}

/// 一个简单的"世界时钟"工具，返回模拟时间。
struct ClockTool;

#[async_trait]
impl ToolExecutor for ClockTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "clock".to_string(),
            description: "获取当前 UTC 时间".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            label: Some("🕐 时钟".to_string()),
            execution_mode: ExecutionMode::Parallel,
        }
    }

    async fn execute(&self, _arguments: serde_json::Value) -> Result<String, UncodeError> {
        let now = chrono::Utc::now();
        Ok(format!(
            "当前 UTC 时间: {}",
            now.format("%Y-%m-%d %H:%M:%S")
        ))
    }
}

/// 一个简单的 echo 工具，回显输入。
struct EchoTool;

#[async_trait]
impl ToolExecutor for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            description: "回显输入文本".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "要回显的文本"
                    }
                },
                "required": ["text"]
            }),
            label: Some("📢 回声".to_string()),
            execution_mode: ExecutionMode::Parallel,
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, UncodeError> {
        let text = arguments["text"].as_str().unwrap_or("");
        Ok(format!("echo: {text}"))
    }
}

/// 简单的表达式求值器（仅用于演示，不是完整的数学引擎）
fn evaluate_expression(expr: &str) -> f64 {
    let allowed: Vec<char> = expr
        .chars()
        .filter(|c| {
            c.is_ascii_digit() || matches!(c, '+' | '-' | '*' | '/' | '(' | ')' | '.' | ' ')
        })
        .collect();
    let sanitized: String = allowed.into_iter().collect();

    let parts: Vec<&str> = sanitized.split_whitespace().collect();
    if parts.len() == 1 {
        return parts[0].parse().unwrap_or(0.0);
    }
    if parts.len() == 3 {
        let a: f64 = parts[0].parse().unwrap_or(0.0);
        let b: f64 = parts[2].parse().unwrap_or(0.0);
        return match parts[1] {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            "/" => {
                if b != 0.0 {
                    a / b
                } else {
                    f64::NAN
                }
            }
            _ => 0.0,
        };
    }
    0.0
}

// ═══════════════════════════════════════════════════════════════════════
// 第二部分：模拟 API 后端
// ═══════════════════════════════════════════════════════════════════════

/// 模拟 LLM API 后端，用于本地测试和演示。
///
/// 不需要真实的 API Key，返回预定义的响应序列。
/// 支持多轮对话：Vec 中每个 Vec<StreamEvent> 对应一轮 LLM 调用。
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

// ═══════════════════════════════════════════════════════════════════════
// 第三部分：辅助函数
// ═══════════════════════════════════════════════════════════════════════

/// 构建完整的模拟 Agent 环境。
async fn build_mock_agent(
    mock_responses: Vec<Vec<StreamEvent>>,
) -> (
    AgentLoop,
    Arc<ToolRegistry>,
    broadcast::Receiver<AgentEvent>,
) {
    // 1. 创建 API 注册表并注册 MockApi
    let mut api_registry = ApiRegistry::new();
    api_registry.register(Arc::new(MockApi::new(mock_responses)));
    let api_registry = Arc::new(api_registry);

    // 2. 创建模型注册表并注册模拟模型
    let mut model_registry = ModelRegistry::new();
    model_registry.register(Model {
        id: "mock-model".to_string(),
        name: "Mock Model".to_string(),
        api: "mock".to_string(),
        provider: "mock".to_string(),
        ..Model::default()
    });
    let model_registry = Arc::new(model_registry);

    // 3. 注册自定义工具
    let tool_registry = Arc::new(ToolRegistry::new());
    tool_registry.register("calculator".to_string(), Arc::new(CalculatorTool));
    tool_registry.register("clock".to_string(), Arc::new(ClockTool));
    tool_registry.register("echo".to_string(), Arc::new(EchoTool));

    // 4. 构建系统提示词
    let system_prompt = SystemPromptBuilder::new()
        .base(concat!(
            "你是一个友好的 AI 助手，可以用中文回复用户问题。",
            "当用户要求计算时，使用 calculator 工具。",
            "当用户询问时间时，使用 clock 工具。"
        ))
        .add_tool_guide(&tool_registry.definitions())
        .add_rules("回答保持简洁，不超过三句话。")
        .build();

    // 5. 创建临时会话存储目录
    let tmp_dir = std::env::temp_dir().join("uncode-example-demo");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let session_store = Arc::new(
        SessionStore::new(tmp_dir)
            .await
            .expect("创建 SessionStore 失败"),
    );

    // 6. 创建 AgentLoop
    let agent = AgentLoop::new(
        api_registry,
        model_registry,
        HashMap::new(),
        tool_registry.clone(),
        session_store,
        system_prompt,
        "mock-model".to_string(),
    );

    // 7. 订阅事件
    let event_rx = agent.subscribe();

    (agent, tool_registry, event_rx)
}

/// 打印消息内容（辅助 debug 输出）
fn print_messages(messages: &[Message]) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║                    对话历史                              ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    for (i, msg) in messages.iter().enumerate() {
        let role_icon = match msg.role {
            Role::System => "⚙️ ",
            Role::User => "👤",
            Role::Assistant => "🤖",
            Role::Tool => "🔧",
            _ => "❓",
        };
        println!("\n[{i}] {role_icon} {:?}", msg.role);
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    println!("    📝 {text}");
                }
                ContentBlock::Thinking { text } => {
                    let preview: String = text.chars().take(80).collect();
                    println!("    💭 [思考] {preview}...");
                }
                ContentBlock::ToolCall(tc) => {
                    let args_preview: String = tc.arguments.to_string().chars().take(60).collect();
                    println!("    🔨 调用工具: {} | 参数: {args_preview}", tc.name);
                }
                ContentBlock::ToolResult(tr) => {
                    let preview: String = tr.content.chars().take(120).collect();
                    let status = if tr.is_error { "❌" } else { "✅" };
                    println!(
                        "    {status} 工具结果 [{}/{}]: {preview}",
                        tr.tool_call_id, status
                    );
                }
                ContentBlock::Image { mime_type, .. } => {
                    println!("    🖼️  图片: {mime_type}");
                }
                _ => {
                    // BashExecution, BranchSummary, CompactionSummary 等
                    println!("    📦 其他内容块");
                }
            }
        }
        if let Some(reason) = &msg.stop_reason {
            println!("    ⏹️  停止原因: {reason:?}");
        }
        if let Some(usage) = &msg.usage {
            println!(
                "    📊 Token: in={} out={}",
                usage.input_tokens, usage.output_tokens
            );
        }
    }
    println!("\n── 对话结束 ──\n");
}

/// 在后台打印 AgentEvent 事件流
async fn print_events(mut event_rx: broadcast::Receiver<AgentEvent>) {
    println!("\n┌── 事件流开始 ──────────────────────────────────────────┐");
    loop {
        match event_rx.recv().await {
            Ok(event) => match &event {
                AgentEvent::SessionStart { session_id, .. } => {
                    let short = &session_id[..8.min(session_id.len())];
                    println!("  🔵 SessionStart: {short}...");
                }
                AgentEvent::TurnStart { turn } => {
                    println!("  🟢 TurnStart: 第 {turn} 轮");
                }
                AgentEvent::MessageStart { role, .. } => {
                    println!("  📨 MessageStart: {role:?}");
                }
                AgentEvent::ContentDelta {
                    delta_type,
                    content,
                    ..
                } => {
                    let preview: String = content.chars().take(40).collect();
                    println!("  ✏️  Delta({delta_type:?}): {preview}");
                }
                AgentEvent::ToolCallStart { tool_name, .. } => {
                    println!("  🔨 ToolCallStart: {tool_name}");
                }
                AgentEvent::ToolCallEnd { data } => {
                    let status = if data.is_error { "失败" } else { "成功" };
                    println!("  ✅ ToolCallEnd: {} ({status})", data.tool_name);
                }
                AgentEvent::TurnEnd { turn, usage } => {
                    println!(
                        "  🔴 TurnEnd: 第 {turn} 轮, token={}/{}",
                        usage.input_tokens, usage.output_tokens
                    );
                }
                AgentEvent::SessionEnd { data } => {
                    println!(
                        "  🔵 SessionEnd: {} turns, reason={}",
                        data.total_turns, data.exit_reason
                    );
                }
                AgentEvent::AgentSettled { .. } => {
                    println!("  🏁 AgentSettled");
                    break;
                }
                AgentEvent::Error { message, .. } => {
                    println!("  ❌ Error: {message}");
                }
                _ => {}
            },
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                println!("  ⚠️  事件落后 {n} 条");
            }
        }
    }
    println!("└── 事件流结束 ──────────────────────────────────────────┘\n");
}

// ═══════════════════════════════════════════════════════════════════════
// 第四部分：演示用例
// ═══════════════════════════════════════════════════════════════════════

// ── 场景 1：纯文本对话 ────────────────────────────────────────────────

async fn demo_1_text_conversation() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 1：纯文本对话");
    println!("  Agent 接收用户输入，直接回复文本（无工具调用）");
    println!("{}", "=".repeat(72));

    let mock_responses = vec![vec![
        StreamEvent::TextDelta("你好！".to_string()),
        StreamEvent::TextDelta("我是 uncode AI 助手。".to_string()),
        StreamEvent::TextDelta("有什么可以帮助你的？".to_string()),
        StreamEvent::Usage(LlmUsageInfo {
            input_tokens: 50,
            output_tokens: 15,
        }),
        StreamEvent::Done {
            reason: StopReason::Stop,
        },
    ]];

    let (agent, _tools, event_rx) = build_mock_agent(mock_responses).await;
    let event_handle = tokio::spawn(print_events(event_rx));

    let user_msg = Message::user("你好，请介绍一下自己");
    let messages = agent.run(user_msg).await.expect("Agent 运行失败");

    let _ = event_handle.await;
    print_messages(&messages);

    println!("✅ 场景 1 完成：Agent 成功回复了文本\n");
}

// ── 场景 2：带工具调用的对话 ──────────────────────────────────────────

async fn demo_2_tool_call() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 2：带工具调用的对话");
    println!("  Agent 识别用户意图 → 调用工具 → 获取结果 → 回复用户");
    println!("{}", "=".repeat(72));

    let mock_responses = vec![
        // Turn 1：工具调用
        vec![
            StreamEvent::ToolCallStart {
                id: "call_1".to_string(),
                name: "calculator".to_string(),
            },
            StreamEvent::ToolCallDelta {
                id: "call_1".to_string(),
                arguments: r#"{"expression":"2 + 3 * 4"}"#.to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_1".to_string(),
                name: "calculator".to_string(),
                arguments: serde_json::json!({"expression": "2 + 3 * 4"}),
            })),
            StreamEvent::Usage(LlmUsageInfo {
                input_tokens: 60,
                output_tokens: 20,
            }),
            StreamEvent::Done {
                reason: StopReason::ToolUse,
            },
        ],
        // Turn 2：文本回复
        vec![
            StreamEvent::ThinkingDelta("计算结果已经出来了。".to_string()),
            StreamEvent::TextDelta("根据计算，2 + 3 × 4 = 14。".to_string()),
            StreamEvent::TextDelta("先乘除后加减，3×4=12，再加2得14。".to_string()),
            StreamEvent::Usage(LlmUsageInfo {
                input_tokens: 100,
                output_tokens: 30,
            }),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ],
    ];

    let (agent, _tools, event_rx) = build_mock_agent(mock_responses).await;
    let event_handle = tokio::spawn(print_events(event_rx));

    let user_msg = Message::user("帮我算一下 2 + 3 * 4 等于多少？");
    let messages = agent.run(user_msg).await.expect("Agent 运行失败");

    let _ = event_handle.await;
    print_messages(&messages);

    println!("✅ 场景 2 完成：Agent 成功调用工具并返回结果\n");
}

// ── 场景 3：多工具并行调用 ────────────────────────────────────────────

async fn demo_3_parallel_tools() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 3：多工具并行调用");
    println!("  Agent 同时调用多个工具，并行执行后汇总结果");
    println!("{}", "=".repeat(72));

    let mock_responses = vec![
        // Turn 1：并行调用两个工具
        vec![
            StreamEvent::ToolCallStart {
                id: "call_clock".to_string(),
                name: "clock".to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_clock".to_string(),
                name: "clock".to_string(),
                arguments: serde_json::json!({}),
            })),
            StreamEvent::ToolCallStart {
                id: "call_echo".to_string(),
                name: "echo".to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_echo".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({"text": "你好，并行世界！"}),
            })),
            StreamEvent::Done {
                reason: StopReason::ToolUse,
            },
        ],
        // Turn 2：文本回复
        vec![
            StreamEvent::TextDelta("我已经同时获取了时间和回显结果。".to_string()),
            StreamEvent::TextDelta("两个工具都执行成功了！".to_string()),
            StreamEvent::Usage(LlmUsageInfo {
                input_tokens: 80,
                output_tokens: 25,
            }),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ],
    ];

    let (agent, _tools, event_rx) = build_mock_agent(mock_responses).await;
    let event_handle = tokio::spawn(print_events(event_rx));

    let user_msg = Message::user("现在几点了？同时帮我回显一段文字");
    let messages = agent.run(user_msg).await.expect("Agent 运行失败");

    let _ = event_handle.await;
    print_messages(&messages);

    println!("✅ 场景 3 完成：Agent 成功并行执行多个工具\n");
}

// ── 场景 4：AgentHarness 高级编排 ─────────────────────────────────────

async fn demo_4_harness() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 4：AgentHarness 高级编排");
    println!("  使用 Harness 包装 AgentLoop，获取 Phase 守卫、");
    println!("  会话持久化、模型切换等高级功能");
    println!("{}", "=".repeat(72));

    let mock_responses = vec![vec![
        StreamEvent::TextDelta("Harness 演示回复。".to_string()),
        StreamEvent::TextDelta("一切运行正常！".to_string()),
        StreamEvent::Usage(LlmUsageInfo {
            input_tokens: 30,
            output_tokens: 10,
        }),
        StreamEvent::Done {
            reason: StopReason::Stop,
        },
    ]];

    // ── 共享 SessionStore ──
    let tmp_dir = std::env::temp_dir().join("uncode-example-demo-harness");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let session_store = Arc::new(
        SessionStore::new(tmp_dir)
            .await
            .expect("创建 SessionStore 失败"),
    );

    // ── 构建 API / Model / Tool 注册表 ──
    let mut api_registry = ApiRegistry::new();
    api_registry.register(Arc::new(MockApi::new(mock_responses)));
    let api_registry = Arc::new(api_registry);

    let mut model_registry = ModelRegistry::new();
    model_registry.register(Model {
        id: "mock-model".to_string(),
        name: "Mock Model".to_string(),
        api: "mock".to_string(),
        provider: "mock".to_string(),
        ..Model::default()
    });
    let model_registry = Arc::new(model_registry);

    let tool_registry = Arc::new(ToolRegistry::new());
    tool_registry.register("echo".to_string(), Arc::new(EchoTool));

    let system_prompt = SystemPromptBuilder::new()
        .base("你是一个演示用的 AI 助手。")
        .build();

    // ── 创建 AgentLoop + AgentHarness ──
    let agent = AgentLoop::new(
        api_registry,
        model_registry,
        HashMap::new(),
        tool_registry,
        session_store.clone(), // 共享同一个 store
        system_prompt,
        "mock-model".to_string(),
    );

    // 预先初始化会话
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    session_store
        .init_session("demo-session-001", "mock-model", &cwd)
        .await
        .expect("初始化会话失败");

    let mut harness = AgentHarness::new(agent, session_store.clone());

    // 检查 Phase 状态
    println!("  Phase 状态: {:?}", harness.phase());
    println!("  是否 Idle: {}", harness.is_idle());

    // 设置 session ID
    harness.set_session_id("demo-session-001".to_string());
    println!("  Session ID: {:?}", harness.session_id());

    // 订阅事件
    let mut harness_event_rx = harness.subscribe();

    // 运行 Agent
    let user_msg = Message::user("用 Harness 运行这个请求");
    let messages = harness.prompt(user_msg).await.expect("Harness 运行失败");

    assert!(harness.is_idle());

    // 收集事件
    println!("\n┌── Harness 事件流 ─────────────────────────────────────┐");
    while let Ok(event) = harness_event_rx.try_recv() {
        println!("  {:?}", std::mem::discriminant(&event));
    }
    println!("└────────────────────────────────────────────────────────┘");

    print_messages(&messages);

    // 演示模型切换
    println!("  🔄 切换模型: mock-model → new-model");
    harness.set_model("new-model", "mock").await;

    // 演示 abort
    harness.abort().await;
    println!("  Abort 后 Idle: {}", harness.is_idle());

    println!("\n✅ 场景 4 完成：AgentHarness 编排正常运行\n");
}

// ── 场景 5：消息构建 API 展示 ─────────────────────────────────────────

fn demo_5_message_api() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 5：消息构建 API");
    println!("  展示 Message、ContentBlock 的各种构造方式");
    println!("{}", "=".repeat(72));

    // 构造用户消息
    let user_msg = Message::user("你好！");
    println!("👤 用户消息: {user_msg:?}");

    // 构造带多内容块的消息
    let complex_msg = Message::new(
        Role::Assistant,
        vec![
            ContentBlock::Thinking {
                text: "我需要先思考一下...".to_string(),
            },
            ContentBlock::Text {
                text: "这是助手的回复内容。".to_string(),
            },
            ContentBlock::ToolCall(Box::new(ToolCall {
                id: "tool_01".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            })),
        ],
    );
    println!("🤖 复杂消息: {complex_msg:?}");

    // 构造工具结果消息
    let tool_msg = Message::new(
        Role::Tool,
        vec![ContentBlock::ToolResult(Box::new(MsgToolResult {
            tool_call_id: "tool_01".to_string(),
            content: "文件内容: Hello World".to_string(),
            is_error: false,
        }))],
    );
    println!("🔧 工具结果消息: {tool_msg:?}");

    // 系统消息
    let system_msg = Message::system("你是一个有用的助手");
    println!("⚙️  系统消息: {system_msg:?}");

    println!("\n✅ 场景 5 完成：消息 API 展示完毕\n");
}

// ── 场景 6：工具定义与验证 ────────────────────────────────────────────

fn demo_6_tool_definition() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 6：工具定义与验证");
    println!("  展示 ToolDefinition、ToolRegistry 的参数验证");
    println!("{}", "=".repeat(72));

    let registry = ToolRegistry::new();

    // 注册一个参数校验严格的自定义工具
    struct GrepTool;

    #[async_trait]
    impl ToolExecutor for GrepTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "grep".to_string(),
                description: "在文件中搜索正则表达式模式".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "正则表达式模式"
                        },
                        "path": {
                            "type": "string",
                            "description": "搜索目录路径"
                        }
                    },
                    "required": ["pattern"]
                }),
                label: Some("🔍 搜索".to_string()),
                execution_mode: ExecutionMode::Parallel,
            }
        }

        async fn execute(&self, args: serde_json::Value) -> Result<String, UncodeError> {
            Ok(format!(
                "搜索完成: {}",
                args["pattern"].as_str().unwrap_or("")
            ))
        }
    }

    registry.register("grep".to_string(), Arc::new(GrepTool));

    // 参数验证：缺少必填字段
    let result = registry.validate("grep", &serde_json::json!({"path": "/tmp"}));
    match result {
        Ok(()) => println!("✅ 验证通过"),
        Err(e) => println!("❌ 验证失败: {e}"),
    }

    // 参数验证：完整参数
    let result = registry.validate(
        "grep",
        &serde_json::json!({"pattern": "TODO", "path": "/tmp"}),
    );
    match result {
        Ok(()) => println!("✅ 验证通过: pattern + path"),
        Err(e) => println!("❌ 验证失败: {e}"),
    }

    // 列出所有注册的工具
    println!("\n  已注册工具:");
    for def in registry.definitions() {
        let label = def.label.as_deref().unwrap_or(&def.name);
        println!(
            "    {label} — {name} ({desc})",
            name = def.name,
            desc = def.description
        );
    }

    println!("\n✅ 场景 6 完成：工具定义与验证展示完毕\n");
}

// ── 场景 7：真实内置工具调用 ──────────────────────────────────────────

/// 构建注册了 uncode 内置工具的 Agent（配合 MockApi 使用）。
async fn build_builtin_agent(
    mock_responses: Vec<Vec<StreamEvent>>,
) -> (
    AgentLoop,
    Arc<ToolRegistry>,
    broadcast::Receiver<AgentEvent>,
) {
    let mut api_registry = ApiRegistry::new();
    api_registry.register(Arc::new(MockApi::new(mock_responses)));
    let api_registry = Arc::new(api_registry);

    let mut model_registry = ModelRegistry::new();
    model_registry.register(Model {
        id: "mock-model".to_string(),
        name: "Mock Model".to_string(),
        api: "mock".to_string(),
        provider: "mock".to_string(),
        ..Model::default()
    });
    let model_registry = Arc::new(model_registry);

    // ── 注册 uncode 真实内置工具 ──
    let tool_registry = Arc::new(ToolRegistry::new());
    tool_registry.register("read".to_string(), Arc::new(ReadTool::new()));
    tool_registry.register("write".to_string(), Arc::new(WriteTool));
    tool_registry.register("edit".to_string(), Arc::new(EditTool));
    tool_registry.register("bash".to_string(), Arc::new(BashTool::new()));
    tool_registry.register("grep".to_string(), Arc::new(GrepTool::default()));
    tool_registry.register("find".to_string(), Arc::new(FindTool));
    tool_registry.register("ls".to_string(), Arc::new(LsTool));

    let system_prompt = SystemPromptBuilder::new()
        .base(concat!(
            "你是一个 AI 编码助手。",
            "你可以使用 read 读取文件、write 写入文件、edit 编辑文件、",
            "bash 执行命令、grep 搜索代码。"
        ))
        .add_tool_guide(&tool_registry.definitions())
        .build();

    let tmp_dir = std::env::temp_dir().join("uncode-example-builtin");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let session_store = Arc::new(
        SessionStore::new(tmp_dir)
            .await
            .expect("创建 SessionStore 失败"),
    );

    let agent = AgentLoop::new(
        api_registry,
        model_registry,
        HashMap::new(),
        tool_registry.clone(),
        session_store,
        system_prompt,
        "mock-model".to_string(),
    );

    let event_rx = agent.subscribe();
    (agent, tool_registry, event_rx)
}

async fn demo_7_builtin_tools() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 7：真实内置工具调用");
    println!("  Agent 使用 uncode 内置 ReadTool / BashTool 执行真实操作");
    println!("{}", "=".repeat(72));

    // 在项目根目录下准备一个临时文件供 read 工具读取
    // （因为工具沙箱要求路径在当前工作目录内）
    let cwd = std::env::current_dir().unwrap();
    let tmp_file = cwd.join("__uncode_demo_readme.txt");
    std::fs::write(
        &tmp_file,
        "Hello from uncode!\nThis is a demo file.\nLine 3",
    )
    .unwrap();
    // 使用相对路径引用（避免绝对路径问题）
    let rel_path = "__uncode_demo_readme.txt";

    let mock_responses = vec![
        // Turn 1：LLM 调用 read 工具读取临时文件
        vec![
            StreamEvent::ToolCallStart {
                id: "call_read".to_string(),
                name: "read".to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_read".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": rel_path}),
            })),
            StreamEvent::Done {
                reason: StopReason::ToolUse,
            },
        ],
        // Turn 2：LLM 调用 bash 查看文件状态
        vec![
            StreamEvent::ToolCallStart {
                id: "call_bash".to_string(),
                name: "bash".to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_bash".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command": format!("ls -la {}", rel_path)}),
            })),
            StreamEvent::Done {
                reason: StopReason::ToolUse,
            },
        ],
        // Turn 3：LLM 回复文本摘要
        vec![
            StreamEvent::TextDelta("已成功读取文件并使用 bash 查看文件信息。".to_string()),
            StreamEvent::TextDelta("一切正常！".to_string()),
            StreamEvent::Usage(LlmUsageInfo {
                input_tokens: 120,
                output_tokens: 25,
            }),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ],
    ];

    let (agent, _tools, event_rx) = build_builtin_agent(mock_responses).await;
    let event_handle = tokio::spawn(print_events(event_rx));

    let user_msg = Message::user("帮我读取临时文件并用 bash 确认它的存在");
    let messages = agent.run(user_msg).await.expect("Agent 运行失败");

    let _ = event_handle.await;
    print_messages(&messages);

    // 清理临时文件
    let _ = std::fs::remove_file(&tmp_file);

    println!("✅ 场景 7 完成：内置工具 read + bash 调用成功\n");
}

// ── 场景 8：内置工具多路并行（read + grep + ls） ───────────────────────

async fn demo_8_parallel_builtin_tools() {
    println!("\n{}", "=".repeat(72));
    println!("  场景 8：内置工具并行调用");
    println!("  Agent 同时调用 read、grep、ls 三个真实工具");
    println!("{}", "=".repeat(72));

    // 在项目根目录下准备测试文件（工具沙箱要求路径在 cwd 内）
    let cwd = std::env::current_dir().unwrap();
    let test_dir = cwd.join("__uncode_demo_parallel");
    let _ = std::fs::create_dir_all(&test_dir);
    std::fs::write(
        test_dir.join("main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    std::fs::write(
        test_dir.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();
    let rel_dir = "__uncode_demo_parallel";

    let mock_responses = vec![
        // Turn 1：LLM 并行发起 read + grep + ls
        vec![
            StreamEvent::ToolCallStart {
                id: "call_read".to_string(),
                name: "read".to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_read".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": format!("{}/main.rs", rel_dir)}),
            })),
            StreamEvent::ToolCallStart {
                id: "call_grep".to_string(),
                name: "grep".to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_grep".to_string(),
                name: "grep".to_string(),
                arguments: serde_json::json!({"pattern": "fn|pub", "path": rel_dir}),
            })),
            StreamEvent::ToolCallStart {
                id: "call_ls".to_string(),
                name: "ls".to_string(),
            },
            StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                id: "call_ls".to_string(),
                name: "ls".to_string(),
                arguments: serde_json::json!({"path": rel_dir}),
            })),
            StreamEvent::Done {
                reason: StopReason::ToolUse,
            },
        ],
        // Turn 2：LLM 汇总回复
        vec![
            StreamEvent::TextDelta("并行读取、搜索、列出目录均已完成。".to_string()),
            StreamEvent::TextDelta("所有工具执行成功！".to_string()),
            StreamEvent::Usage(LlmUsageInfo {
                input_tokens: 150,
                output_tokens: 30,
            }),
            StreamEvent::Done {
                reason: StopReason::Stop,
            },
        ],
    ];

    let (agent, _tools, event_rx) = build_builtin_agent(mock_responses).await;
    let event_handle = tokio::spawn(print_events(event_rx));

    let user_msg = Message::user("请同时读取 main.rs、搜索函数定义、列出目录内容");
    let messages = agent.run(user_msg).await.expect("Agent 运行失败");

    let _ = event_handle.await;
    print_messages(&messages);

    // 清理
    let _ = std::fs::remove_dir_all(&test_dir);

    println!("✅ 场景 8 完成：read + grep + ls 并行调用成功\n");
}

// ═══════════════════════════════════════════════════════════════════════
// 主函数：顺序执行所有演示
// ═══════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║        uncode Agent 框架 — 简单示例程序                 ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    // 先执行不需要 async 的纯数据演示
    demo_5_message_api();
    demo_6_tool_definition();

    // 执行需要 async runtime 的 Agent 演示
    demo_1_text_conversation().await;
    demo_2_tool_call().await;
    demo_3_parallel_tools().await;
    demo_4_harness().await;
    demo_7_builtin_tools().await;
    demo_8_parallel_builtin_tools().await;

    println!("\n🎉 所有演示场景执行完毕！\n");
}
