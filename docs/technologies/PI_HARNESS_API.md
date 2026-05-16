# Pi AgentHarness API 参考

> 核心方法、运行时配置、事件注册、Phase 守卫、Pending Session Write

---

## 核心方法

| 方法 | 说明 |
|------|------|
| `prompt(text, {images?})` | 开始新 turn |
| `continue()` | 从当前 transcript 继续 |
| `skill(name, instructions?)` | 调用已加载技能 |
| `promptFromTemplate(name, args)` | 调用 prompt 模板 |
| `steer(text, {images?})` | 注入 steering 消息 |
| `followUp(text, {images?})` | 排队 follow-up 消息 |
| `nextTurn(text, {images?})` | prepend 到下一轮 prompt |
| `appendMessage(msg)` | 运行中追加消息（经 pending writes 缓冲） |
| `compact(instructions?)` | 手动触发上下文压缩 |
| `navigateTree(targetId, opts)` | 分支导航 + 摘要生成 |
| `abort()` | 中断 + 清空队列 |

---

## 运行时配置

| 方法 | 说明 |
|------|------|
| `setModel(model)` / `getModel()` | 切换 LLM 模型 |
| `setThinkingLevel(level)` / `getThinkingLevel()` | 切换思考级别 |
| `setActiveTools(names)` | 激活/停用工具 |
| `setTools(tools, activeNames?)` | 替换工具集 |
| `setResources(resources)` / `getResources()` | 更新技能和模板 |
| `setStreamOptions(opts)` / `getStreamOptions()` | LLM 流选项 |
| `setSteeringMode(mode)` / `setFollowUpMode(mode)` | 队列模式 |

---

## 事件注册

```typescript
// 通配订阅（所有事件）
harness.subscribe((event) => { ... });

// 类型化 hook（带返回值）
harness.on("tool_call", async ({ toolCall }) => {
    return { block: true, reason: "unsafe" };
});
const unsubscribe = harness.on("context", async ({ messages }) => {
    return { messages: trimOldMessages(messages) };
});
```

---

## AgentHarness Phases

```typescript
type AgentHarnessPhase = "idle" | "turn" | "compaction" | "branch_summary" | "retry";
```

Phase 守卫：非 idle 时 `prompt()` 抛 `AgentHarnessError("busy")`。Phase 决定哪些操作被允许。

---

## Pending Session Write

运行中的 session 写入（model change、thinking level change、custom entries、labels）缓存在 `pendingSessionWrites[]` 中，在 turn 边界、agent end、prepareNextTurn 时 flush。防止 mid-turn 状态变化导致 session 损坏。

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
