# Pi 事件与 Hook 系统

> AgentEvent（10 种四层）、AgentHarness 事件（20+）、Hook 返回值语义、订阅模型

---

## AgentEvent（10 种，四层）

| 层级 | 事件 | 含义 |
|------|------|------|
| **Agent** | `agent_start` / `agent_end` | 一次 prompt/continue 调用的生命周期 |
| **Turn** | `turn_start` / `turn_end` | 一轮 LLM 调用 + 工具执行 |
| **Message** | `message_start` / `message_update` / `message_end` | 单条消息生命周期 |
| **Tool** | `tool_execution_start` / `tool_execution_update` / `tool_execution_end` | 单个工具执行生命周期 |

---

## AgentHarness Hook 事件（20+ 种）

Harness 通过 `on(type, handler)` 注册带返回值的 hook 事件：

### Hook 事件（可修改行为）

| Hook 事件 | 返回值能力 |
|-----------|-----------|
| `before_agent_start` | 可注入 messages、覆盖 systemPrompt |
| `context` | 可替换消息数组 |
| `before_provider_request` | 可 patch streamOptions |
| `before_provider_payload` | 可修改原始 HTTP payload |
| `after_provider_response` | 接收 HTTP status/headers |
| `tool_call` | 可 `{ block: true, reason }` 阻止执行 |
| `tool_result` | 可 patch content/set terminate |
| `session_before_compact` | 可 cancel 或提供预计算结果 |
| `session_before_tree` | 可 cancel/提供 summary |

### 纯观察事件（无返回值）

`queue_update`, `save_point`, `abort`, `settled`, `session_compact`, `session_tree`, `model_select`, `thinking_level_select`, `resources_update`

---

## Hook 返回值语义

Pi 的 hook 系统核心设计是**事件监听器可返回 typed result**，实现非侵入式行为修改：

```typescript
// 阻止工具执行
harness.on("tool_call", async ({ toolCall }) => {
    return { block: true, reason: "unsafe" };
});

// 替换上下文消息
const unsubscribe = harness.on("context", async ({ messages }) => {
    return { messages: trimOldMessages(messages) };
});

// 修改工具结果
harness.on("tool_result", async ({ toolResult }) => {
    return { content: "...", terminate: true };
});

// 取消压缩
harness.on("session_before_compact", async () => {
    return { cancel: true };
});
```

每种 hook 事件的返回类型不同，TypeScript 编译期检查返回值合法性。

---

## 事件序列示例

```
prompt("读取 config.json")
├── agent_start
├── turn_start
├── message_start   { user message }
├── message_end     { user message }
├── message_start   { assistant message (partial) }
├── message_update  { text_delta: "我来读取..." }
├── message_update  { toolcall_delta: ... }
├── message_end     { assistant message (含 toolCall) }
├── tool_execution_start  { toolCallId, toolName: "read_file", args }
├── tool_execution_update { partialResult }
├── tool_execution_end    { result, isError: false }
├── message_start   { toolResult message }
├── message_end     { toolResult message }
├── turn_end
│
├── turn_start                          ← 下一轮
├── message_start   { assistant message }
├── message_update  { text_delta: "文件内容是..." }
├── message_end     { assistant message }
├── turn_end
└── agent_end
```

---

## 订阅语义

```typescript
agent.subscribe(async (event, signal) => {
    if (event.type === "agent_end") {
        await flushToDisk(signal);  // 阻塞 idle 判定
    }
});
```

- 监听器按注册顺序依次 await
- `agent_end` 的 await 监听器完成前，Agent 不算 idle
- 每个监听器收到当前的 `AbortSignal`

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
