# uncode 事件系统

> AgentEvent（18 variants）+ EventRouter + HookResult | 基于源码分析，2026-05 修订

> **L1 对照：** Pi 四层 `AgentEvent`（10 种）与 Harness Hook 的映射见 [`UNCODE_PI_MECHANISM_MAP.md`](UNCODE_PI_MECHANISM_MAP.md) §5；Pi 侧见 [`PI_EVENT_SYSTEM.md`](../pi-technologies/PI_EVENT_SYSTEM.md)。

uncode 的跨层通信通过 `AgentEvent` 枚举实现。上层（TUI / Platform / RPC）通过 `broadcast::Receiver<AgentEvent>` 订阅事件流，实现发布-订阅解耦。

---

## AgentEvent 枚举

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentEvent {
    // ── Session 生命周期 ──
    SessionStart { session_id: String, timestamp: DateTime<Utc> },
    SessionEnd { data: Box<SessionEndData> },

    // ── Turn 生命周期 ──
    TurnStart { turn: u64 },
    TurnEnd { turn: u64, usage: UsageInfo },

    // ── Message 生命周期 ──
    MessageStart { role: Role, message_id: String },
    MessageEnd { role: Role, message_id: String },

    // ── 内容流式传输 ──
    ContentDelta { delta_type: DeltaType, content: String, content_index: Option<usize> },

    // ── 工具执行生命周期 ──
    ToolCallStart { tool_id: String, tool_name: String, arguments_summary: String },
    ToolCallProgress { tool_id: String, progress_type: ProgressType, detail: String },
    ToolCallEnd { data: Box<ToolCallEndEventData> },

    // ── 任务/阶段 ──
    TaskUpdate { data: Box<TaskUpdateData> },
    PhaseSummary { data: Box<PhaseSummaryData> },

    // ── 压缩完成 ──
    CompactionComplete { messages_replaced: usize, tokens_before: u64, tokens_after: u64, summary_text: String },

    // ── 消息队列 ──
    MessageQueued { text: String },
    MessageDelivered { text: String },

    // ── 错误 / 中断 ──
    Error { category: ErrorCategory, message: String, recoverable: bool },
    AgentInterrupted { turn: u64, partial_response: bool },

    // ── 安定状态 ──
    AgentSettled { session_id: String },
}
```

### 支撑枚举

| 枚举 | 值 |
|------|-----|
| `DeltaType` | `Thinking`, `Text` |
| `ProgressType` | `Spinner(String)`, `Percentage { current, total }`, `LogLine`, `Stdout` |
| `ToolCallStatus` | `Success`, `Failed`, `Cancelled` |
| `TaskStatus` | `Pending`, `Running`, `Done`, `Failed`, `Blocked` |
| `ErrorCategory` | `Llm`, `Tool`, `Network`, `Config` |

---

## 事件序列示例

一个完整的 Agent 交互生命周期：

```
SessionStart { session_id, timestamp }
│
├── TurnStart { turn: 1 }
│   ├── ContentDelta { Thinking, "正在分析..." }
│   ├── ContentDelta { Text, "我来帮你..." }
│   ├── ToolCallStart { tool_id: "tc1", tool_name: "read", arguments_summary: "path=..." }
│   ├── ToolCallProgress { tool_id: "tc1", Spinner, "读取中" }
│   ├── ToolCallEnd { tool_id: "tc1", status: Success, duration_ms: 45 }
│   ├── ContentDelta { Text, "文件内容如下..." }
│   └── TurnEnd { turn: 1, usage: UsageInfo { input: 1500, output: 320 } }
│
├── TurnStart { turn: 2 }
│   ├── ToolCallStart { tool_id: "tc2", tool_name: "edit", ... }
│   ├── ToolCallEnd { tool_id: "tc2", status: Success }
│   └── TurnEnd { turn: 2, usage: UsageInfo { input: 2800, output: 180 } }
│
├── MessageQueued { text: "请也修改测试文件" }  ← 用户中途注入
│
├── TurnStart { turn: 3 }  ← steering 消息被 pickup
│   ├── ContentDelta { Text, "好的，我来修改测试..." }
│   └── TurnEnd { turn: 3, usage: ... }
│
├── CompactionComplete { messages_replaced: 8, tokens_before: 45000, tokens_after: 18000 }
│
├── SessionEnd { session_id, total_turns: 3, exit_reason: "completed" }
└── AgentSettled { session_id }
```

---

## EventRouter

双通道事件路由器：观察型（fire-and-forget）+ 控制型（异步返回 HookResult）。

```rust
pub struct EventRouter {
    sync_handlers: HashMap<String, Vec<SyncEventHandler>>,
    hook_handlers: HashMap<String, Vec<AsyncHookHandler>>,
}
```

### 注册 API

```rust
// 观察型：仅接收事件，无返回值
router.on("tool_call_start", Box::new(|event| {
    println!("Tool started: {:?}", event);
}));

// 控制型：可返回 HookResult 修改 Agent 行为
router.on_hook("tool_call_end", Box::new(|event| {
    Box::pin(async move {
        HookResult::Block { reason: "手动拒绝".into() }
    })
}));
```

### 事件类型匹配

事件类型通过 serde tag name 匹配（`event_tag()` 函数），避免 JSON 序列化开销：

```rust
fn event_tag(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::SessionStart { .. } => "session_start",
        AgentEvent::ToolCallEnd { .. } => "tool_call_end",
        // ... 所有 16 个 variant
    }
}
```

---

## HookResult — 控制指令

```rust
pub enum HookResult {
    Continue,                                              // 无干预
    Block { reason: String },                              // 阻止执行
    PatchMessages { messages: Vec<Message> },              // 替换上下文消息
    PatchToolResult { content: Option<...>, terminate: Option<bool> }, // 修改工具结果
    CancelCompaction,                                      // 取消压缩
}
```

| 指令 | 适用场景 |
|------|----------|
| `Continue` | 默认，正常流程 |
| `Block` | `beforeToolCall` 阻止危险工具执行 |
| `PatchMessages` | 动态修改发送给 LLM 的上下文 |
| `PatchToolResult` | 脱敏、额外验证、强制终止 |
| `CancelCompaction` | 阻止上下文压缩（如保留完整调试信息） |

---

## broadcast 通道

`AgentLoop` 内部持有 `broadcast::Sender<AgentEvent>`。上层通过 `agent.subscribe()` 获取 `Receiver`：

```rust
// uncode-cli/src/main.rs
let event_rx = agent.subscribe();       // broadcast::Receiver<AgentEvent>
let event_tx = agent.event_sender();    // broadcast::Sender<AgentEvent>

tokio::spawn(async move {
    tui.run(event_rx, |text, cancel_token| {
        // 每次用户输入创建新 AgentLoop，共享同一个 event_tx
        let mut a = AgentLoop::with_event_sender(..., event_tx.clone());
        a.run(Message::user(text)).await;
    });
});
```

TUI 的 `tokio::select!` 循环同时监听 crossterm UI 事件和 `event_rx.recv()`，实现实时渲染。

---

## Extension Hook 系统

`uncode-extensions` 提供更高层的生命周期 Hook：

```rust
pub enum LifecycleHook {
    SessionStart, TurnStart, MessageReceived, MessageSending,
    ToolCallBefore, ToolCallAfter, TurnEnd, SessionEnd,
}
```

Extension 实现 `Extension` trait：

```rust
#[async_trait]
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;
    async fn on_hook(&self, ctx: &HookContext) -> anyhow::Result<()>;
}
```

通过 `HookRegistry` 注册和调度（基于 `DashMap`），WASM 加载器为 scaffold 阶段。

---

## 附录：Pi 事件对照速查

完整矩阵（含 Harness Hook、1:1 / 1:N 标注）见 [`UNCODE_PI_MECHANISM_MAP.md`](UNCODE_PI_MECHANISM_MAP.md) §5。  
uncode 共 **18** 个 `AgentEvent` 变体；Pi UI 层 **10** 种四层事件 + 20+ Harness Hook。

| Pi 四层事件 | uncode 主要变体 |
|-------------|-----------------|
| `agent_start` / `agent_end` | `SessionStart` / `SessionEnd`（近似） |
| `turn_start` / `turn_end` | `TurnStart` / `TurnEnd` |
| `message_*` | `MessageStart` / `MessageEnd` + `ContentDelta` |
| `tool_execution_*` | `ToolCallStart` / `ToolCallProgress` / `ToolCallEnd` |

---

*本文档基于 uncode 源码（`crates/uncode-core/src/event.rs`、`crates/uncode-extensions/src/`）编写。*
