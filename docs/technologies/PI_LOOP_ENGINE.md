# Pi 循环引擎

> 双层循环架构、Turn 生命周期、agentLoop 低层 API、Agent 生命周期管理

---

## 双层循环架构

`agentLoop()` 的核心是**双层 while 循环**，由 `Agent` 和 `AgentHarness` 两层共用：

```
prompt("Hello")
    │
    ▼
┌─ 外层循环：follow-up 驱动 (while true) ────────────────────┐
│                                                             │
│  ┌─ 内层循环：tool-call + steering 驱动 ───────────────┐  │
│  │  while (hasMoreToolCalls || pendingMessages.length) │  │
│  │                                                      │  │
│  │  turn_start                                          │  │
│  │    ↓                                                 │  │
│  │  注入 pendingMessages（steering/nextTurn/followUp）   │  │
│  │    ↓                                                 │  │
│  │  transformContext() → convertToLlm() → LLM 流式调用   │  │
│  │    ↓                                                 │  │
│  │  解析 assistant message                              │  │
│  │    ↓                                                 │  │
│  │  有 tool_calls? ──Yes──→ executeToolCalls()          │  │
│  │    │                      ↓                          │  │
│  │    │                  emit tool_execution_*           │  │
│  │    │                      ↓                          │  │
│  │    │                  toolResults → 推入 context      │  │
│  │    │                      ↓                          │  │
│  │    │                  hasMoreToolCalls = true         │  │
│  │    │                      ↓                          │  │
│  │    │              turn_end → shouldStopAfterTurn?     │  │
│  │    │                      ↓                          │  │
│  │    │           prepareNextTurn() → 刷新上下文         │  │
│  │    │                      ↓                          │  │
│  │    │           steering = getSteeringMessages()       │  │
│  │    │           → 汇入 pendingMessages，继续内层循环    │  │
│  │    │                                                  │  │
│  │    No → turn_end → shouldStopAfterTurn?               │  │
│  │              ↓                                       │  │
│  │         exit 内层循环                                │  │
│  └──────────────────────────────────────────────────────┘  │
│                         ↓                                  │
│         followUp = getFollowUpMessages()                    │
│         有? → pendingMessages = followUp, continue 外层      │
│         无? → emit agent_end, break                        │
└─────────────────────────────────────────────────────────────┘
```

关键特性：
- **内层循环继续条件**：`hasMoreToolCalls || pendingMessages.length > 0`
- **外层循环继续条件**：有 follow-up 消息
- **steering vs follow-up**：steering 在内层循环每轮 turn 后注入（修正方向）；follow-up 在内层循环完全退出后注入（追加任务）
- **nextTurn**：在首次进入内层循环前注入（prepend 到下一轮 prompt）

### 终止机制

工具有三种方式终止 agent：
- **`terminate: true`**：当**整批**工具都标记 terminate 时，`hasMoreToolCalls = false`，内层循环退出
- **`shouldStopAfterTurn`**：turn_end 后回调返回 true，直接 break 外层循环
- **错误/中止**：LLM 返回 `stopReason: "error"` 或 `"aborted"`，立即 exit

---

## Turn 生命周期控制

### shouldStopAfterTurn

`turn_end` 后检查，返回 `true` 则优雅退出：

```typescript
shouldStopAfterTurn: async ({ message, toolResults, context, newMessages }) => {
    return shouldCompactBeforeNextTurn(context.messages);
}
```

参数说明：
- `message`：当前 assistant 消息
- `toolResults`：本轮工具执行结果
- `context`：完整 AgentContext
- `newMessages`：本轮新增的所有消息

### prepareNextTurn

`turn_end` 后可返回新的 context/model/thinkingLevel，实现跨轮次动态切换：

```typescript
prepareNextTurn: async ({ message, toolResults, context }) => {
    if (message.stopReason === "length") {
        return { model: largerModel, thinkingLevel: "high" };
    }
}
```

返回值说明：
- `context`：可选，替换 AgentContext
- `model`：可选，切换 LLM 模型
- `thinkingLevel`：可选，切换思考级别

Harness 用此回调刷新 turn 间状态。

---

## Agent 生命周期管理

### 运行保护

```typescript
class Agent {
    private activeRun?: ActiveRun;  // { promise, resolve, abortController }

    async prompt(message): Promise<void> {
        if (this.activeRun) throw new Error("Agent is already processing...");
        // 创建 ActiveRun、执行、清理
    }
}
```

同一时刻只能有一个 run。

### ActiveRun 追踪

```typescript
type ActiveRun = {
    promise: Promise<void>;
    resolve: () => void;
    abortController: AbortController;
};
```

### Abort 机制

```typescript
agent.abort();              // 触发 AbortController
await agent.waitForIdle();  // 等待 run 完全结束（含 agent_end 监听器）
agent.reset();              // 清空 transcript、runtime state、queues
```

### 错误恢复

run 内部未捕获异常时：自动合成 `agent_end` 事件（含错误信息）→ 清理运行时状态 → 恢复 idle。

### getApiKey 动态解析

`AgentLoopConfig.getApiKey` 每次 LLM 调用前解析 API key，支持 OAuth 短期 token 刷新（如 GitHub Copilot）。

---

## agentLoop() 低层 API

```typescript
const stream = agentLoop(
    [userMessage],    // 初始消息
    context,          // AgentContext { systemPrompt, messages, tools }
    config,           // AgentLoopConfig
);

for await (const event of stream) { /* AgentEvent */ }
const finalMessages = await stream.result();
```

**内部实现**：
- `agentLoop()` 创建 `EventStream`，后台调用 `runLoop()`，立即返回 stream
- `runLoop()` 是有状态的 `async function`（`currentContext`、`hasMoreToolCalls`、`pendingMessages`），返回 `Promise<void>`
- 所有事件通过 `emit()` 推入 `EventStream`

**与上层的关键差异**：
- agentLoop 不管理持久状态——context 由调用者维护
- agentLoop 不等待事件监听器——fire-and-forget
- 适合自定义状态管理或 proxy 后端

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
