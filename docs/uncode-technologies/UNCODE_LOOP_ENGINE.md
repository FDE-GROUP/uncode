# uncode 循环引擎

> AgentLoop 双层循环架构 | 基于 `crates/uncode-agent/src/loop_engine.rs` 源码分析

> **L1 机制对齐（Pi）**：双层 `while` 循环、`Turn`、`Steering` / `Follow-up` / `NextTurn` 三通道、`terminate` 批次 AND 语义与 Pi `agentLoop` 同构。对照表见 [`UNCODE_PI_MECHANISM_MAP.md`](UNCODE_PI_MECHANISM_MAP.md)；Pi 侧见 [`PI_LOOP_ENGINE.md`](../pi-technologies/PI_LOOP_ENGINE.md)。

uncode 的核心是一个双标签循环（outer `'outer` + inner `while`），与 Pi 的双层 while 同构。外层处理 followUp（会话延续），内层处理 ReAct 闭环（工具调用链）。

与 Pi 的整体对齐与取舍见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

---

## 双层循环架构

```
AgentLoop::run_inner(user_message)
│
│  ① Session 初始化（SurrealDB / `SessionStore`，必要时从导入的 JSONL 迁移）
│  ② 持久化用户消息
│  ③ build_context() 从 session 重建消息历史
│  ④ 插入 System Prompt
│  ⑤ drain next_turn → pending_messages
│
├── 'outer: loop {
│   │
│   │  has_more_tool_calls = true
│   │
│   ├── while has_more_tool_calls || !pending_messages.is_empty() {
│   │   │
│   │   │  检查 CancellationToken / MAX_TURNS(50)
│   │   │  注入 pending_messages → messages[] 并持久化
│   │   │  turn++
│   │   │
│   │   │  解析模型 → 检查是否需要压缩 → 重建上下文
│   │   │  确定 ThinkingLevel
│   │   │  构建 Context { system_prompt, messages, tools }
│   │   │  构建 StreamOptions { temperature: 0.7, max_tokens: 8192 }
│   │   │  emit TurnStart
│   │   │
│   │   │  ┌─ LLM Stream ─────────────────────────┐
│   │   │  │  ThinkingDelta → 累积 + emit ContentDelta  │
│   │   │  │  TextDelta     → 累积 + emit ContentDelta  │
│   │   │  │  ToolCallStart → 缓冲 (id, name)           │
│   │   │  │  ToolCallDelta → 累积 arguments             │
│   │   │  │  ToolCallEnd   → 缓冲完整参数               │
│   │   │  │  Usage         → 追踪 token                │
│   │   │  │  Error         → emit 错误事件             │
│   │   │  │  Done          → 组装助手消息               │
│   │   │  └───────────────────────────────────────┘
│   │   │
│   │   │  持久化助手消息
│   │   │  批量执行工具调用（并行或串行）
│   │   │  emit ToolCallEnd 事件
│   │   │  持久化工具结果
│   │   │  检查 terminate 标志
│   │   │
│   │   │  emit TurnEnd
│   │   │  prepare_next_turn 回调
│   │   │  should_stop_after_turn 回调 → break 'outer
│   │   │  drain steering → pending_messages  ← 关键：每 turn 轮询
│   │   }
│   │
│   │  === 内层循环退出 ===
│   │
│   │  drain follow_up → pending_messages
│   │  if follow_ups exist → continue 'outer
│   │  else → break 'outer
│   }
│
├── emit SessionEnd + AgentSettled
└── return Vec<Message>
```

---

## Turn 驱动因素

Turn 由 5 种机制驱动循环：

| 机制 | 注入时机 | 作用域 |
|------|----------|--------|
| **工具调用** | LLM 返回 ToolCall → 执行后 `has_more_tool_calls = true` | 内层循环 |
| **Steering** | 每 turn 结束后 drain steering channel | 内层循环 |
| **FollowUp** | 内层循环退出后 drain follow_up channel | 外层循环 |
| **NextTurn** | 首次进入内层前 drain | 内层循环 |
| **CancellationToken** | 5 个检查点（预流式、流式中、每 turn 开始） | 全局中断 |

---

## Steering 三通道设计

`MessageQueue`（`steering.rs`）维护三个独立的 `mpsc::channel<Message>`（容量 64）：

```rust
pub struct MessageQueue {
    steering:   mpsc::Sender<Message>,   // 中途纠偏
    follow_up:  mpsc::Sender<Message>,   // 会话延续
    next_turn:  mpsc::Sender<Message>,   // 下一轮预排队
}
```

| Channel | 用途 | drain 时机 |
|---------|------|------------|
| `steering` | 用户中途纠正/补充指令 | 每 turn 结束后，注入 pending_messages，喂入内层循环 |
| `follow_up` | 后续用户消息（agent 本应停止后） | 内层循环退出时，重新进入外层循环 |
| `next_turn` | 下一轮预排队消息 | 首次进入内层前，注入 pending_messages |

**关键路径**：`AgentHarness.steer(msg)` → `agent.steer(msg)` → lock queue → send to steering channel + emit `MessageQueued` 事件。

Steering 消息无需等待当前 LLM 调用结束——它们在当前 turn 完成后立即被 pickup。

---

## 终止信号语义

```rust
// loop_engine.rs
let mut should_terminate = !all_outcomes.is_empty();
for (_id, _name, tool_result) in &all_outcomes {
    if !tool_result.terminate { should_terminate = false; }
}
```

**AND 语义**：批次中所有工具结果都必须设置 `terminate = true` 才会终止循环。只要有一个工具没有声明终止，循环继续。这是保守策略——不会因为单个工具的终止信号就中断整个批次。

---

## 并发保护

```rust
// AgentLoop 使用 AtomicBool 作为运行锁
if self.active_run.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
    return Err(HarnessError::Busy.into());
}
// run_inner 结束时重置为 false
```

同时运行的 `run()` 调用会被拒绝并返回 `HarnessError::Busy`。

---

## 回调钩子

`AgentLoop` 支持三个外部回调（通过闭包注入）：

| 回调 | 签名 | 作用 |
|------|------|------|
| `should_stop_after_turn` | `Fn(turn, &messages) -> Option<StopReason>` | 外部条件终止循环 |
| `prepare_next_turn` | `Fn(turn, &messages) -> Option<(Context, Model)>` | 动态切换上下文或模型 |
| `transform_context` | `Fn(&mut Vec<Message>)` | 发送给 LLM 前最后修改消息数组 |

---

## 压缩触发

每个 turn 结束时检查是否需要压缩（在 `compact_if_needed()` 中）：

```rust
// loop_engine.rs — 每 turn 检查
if should_compact_session(&self.session_store, &session_id, context_window) {
    compact_session(&self.session_store, &session_id, ...).await?;
    // 重建上下文
    let built = context_builder::build_context(&self.session_store, &session_id)?;
    messages = built.messages;
    messages.insert(0, Message::system(self.system_prompt.clone()));
}
```

触发条件：估算 token 总量超过 `context_window * 80%`。压缩后保留最近 `context_window * 20%` 的 token。详见 [会话模型](UNCODE_SESSION_MODEL.md)。

---

## 错误与中断

| 场景 | 处理 |
|------|------|
| LLM 返回 `Error` stopReason | emit Error 事件，跳出循环 |
| `CancellationToken` 取消 | emit `AgentInterrupted` 事件，立即退出 |
| 超过 `MAX_TURNS(50)` | emit Error 事件，跳出循环 |
| 工具执行失败 | 记录 `is_error: true`，LLM 看到后自行推理重试 |
| 压缩失败 | emit Error 事件，循环继续（使用未压缩的上下文） |

---

*本文档基于 uncode 源码（`crates/uncode-agent/src/loop_engine.rs`、`steering.rs`）编写。*
