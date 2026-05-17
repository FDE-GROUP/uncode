# Pi 消息系统

> AgentMessage 抽象、convertToLlm 桥接、消息队列（三种 + QueueMode）、Agent 状态管理

---

## AgentMessage 抽象

Pi 的核心设计之一是 `AgentMessage` —— 比 LLM 原生 `Message` 更丰富的消息类型：

### 类型系统

```typescript
// LLM 原生消息
type Message = UserMessage | AssistantMessage | ToolResultMessage

// AgentMessage = LLM 消息 + 应用自定义消息（通过 TypeScript declaration merging）
type AgentMessage = Message | CustomAgentMessages[keyof CustomAgentMessages]
```

### 内置自定义消息类型（4 种）

| 类型 | role | 关键字段 | 说明 |
|------|------|---------|------|
| `BashExecutionMessage` | `"bashExecution"` | command, output, exitCode, cancelled, truncated, `excludeFromContext?` | bash 执行结果；`excludeFromContext=true` 时从 LLM context 静默丢弃 |
| `CustomMessage<T>` | `"custom"` | customType, content, display, details | 通用类型化消息 |
| `BranchSummaryMessage` | `"branchSummary"` | summary, fromId | 分支切换时生成的摘要 |
| `CompactionSummaryMessage` | `"compactionSummary"` | summary, tokensBefore | 上下文压缩摘要 |

---

## 消息桥接：convertToLlm

`AgentMessage[]` 发给 LLM 前需经两次转换：

```
AgentMessage[]
    │
    ▼ transformContext()      ← 可选：修剪旧消息、注入外部上下文
    │                           操作对象：AgentMessage[]
    │
    ▼ convertToLlm()          ← 必须：过滤非 LLM 消息、转换自定义类型
    │                           操作对象：AgentMessage[] → Message[]
    │
    ▼ LLM
```

`convertToLlm` 实际逻辑：
- `bashExecution` → `excludeFromContext` 时丢弃，否则转为 user message
- `custom` → 转为 user message
- `branchSummary` → 包裹 `BRANCH_SUMMARY_PREFIX/SUFFIX` 后转为 user message
- `compactionSummary` → 包裹 `COMPACTION_SUMMARY_PREFIX/SUFFIX` 后转为 user message
- `user` / `assistant` / `toolResult` → 透传
- 其他 → `undefined`，被过滤

---

## 消息队列系统

### 三种队列

| 队列 | 注入时机 | 用途 |
|------|---------|------|
| **Steering** | 内层循环每轮 turn 后 | 用户中途修正 Agent 方向 |
| **Follow-up** | 内层循环完全退出后 | Agent 自然停止后追加新任务 |
| **NextTurn** | 首次进入内层循环前 | prepend 到下一轮 prompt |

### QueueMode

| 模式 | 行为 |
|------|------|
| `"one-at-a-time"`（默认） | 每次只取一条，剩余保留 |
| `"all"` | 一次取完所有排队消息 |

Steering 和 Follow-up 各自独立的 QueueMode，通过 `setSteeringMode()` / `setFollowUpMode()` 配置。

### 队列操作

```typescript
agent.steer(message);
agent.followUp(message);
agent.nextTurn(message);
agent.clearSteeringQueue();
agent.clearFollowUpQueue();
agent.clearAllQueues();
agent.hasQueuedMessages();
```

---

## Agent 状态管理

### AgentState

```typescript
interface AgentState {
    systemPrompt: string;
    model: Model;
    thinkingLevel: ThinkingLevel;       // off/minimal/low/medium/high/xhigh
    tools: AgentTool[];                 // setter 自动 copy 数组
    messages: AgentMessage[];           // setter 自动 copy 数组
    readonly isStreaming: boolean;
    readonly streamingMessage?: AgentMessage;
    readonly pendingToolCalls: ReadonlySet<string>;
    readonly errorMessage?: string;
}
```

数组保护：`tools` 和 `messages` 的 setter 在存储前浅拷贝，防止外部修改。

### 动态配置

```typescript
agent.state.model = newModel;           // 热切换模型
agent.state.thinkingLevel = "high";
agent.toolExecution = "sequential";     // 运行时切换执行模式
agent.beforeToolCall = async ({ toolCall }) => { ... };
```

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
