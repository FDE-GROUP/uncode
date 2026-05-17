# Pi Agent 架构分析

本文档分析 Pi (`@earendil-works/pi-agent-core`) 的 Agent 运行时架构，不涉及与 uncode 的对比。

---

## 一、三层架构总览

Pi Agent 是**三层架构**，每层有独立的状态管理和职责边界：

```
┌──────────────────────────────────────────────────────────────┐
│  AgentHarness（高层 — 生产编排器）                             │
│  session 持久化 / compaction / skills / templates / resources │
│  hook 系统（20+ 事件） / ExecutionEnv / tree navigation       │
│  prompt() / skill() / compact() / navigateTree() / steer()   │
├──────────────────────────────────────────────────────────────┤
│  Agent 类（中层 — 有状态封装）                                  │
│  transcript 管理 / steering & follow-up 队列                   │
│  事件订阅 / ActiveRun 追踪 / 生命周期管理                       │
│  prompt() / continue() / abort() / reset() / subscribe()     │
├──────────────────────────────────────────────────────────────┤
│  agentLoop()（底层 — 无状态引擎）                               │
│  双层 while 循环 / 工具执行 / 事件发射 / 上下文转换              │
│  返回 EventStream<AgentEvent>                                  │
├──────────────────────────────────────────────────────────────┤
│  @earendil-works/pi-ai（LLM 抽象层）                           │
│  9 个内置 API / 25+ provider / 延迟加载 / 兼容层               │
│  streamSimple() / stream() / complete() / Model / Context     │
└──────────────────────────────────────────────────────────────┘
```

### 三层关系

| 层 | 状态 | 队列 | 会话 | 适用场景 |
|---|---|---|---|---|
| **AgentHarness** | 完整（含 session tree） | 三种（steer/followUp/nextTurn） | 树状持久化 | 生产应用（CLI/IDE） |
| **Agent** | 轻量（transcript） | 两种（steer/followUp） | 无 | 自定义状态管理 |
| **agentLoop** | 无（调用者维护） | 通过 config 注入 | 无 | proxy 后端、测试 |

---

## 二、双层循环架构

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

### 2.1 终止机制

工具有三种方式终止 agent：
- **`terminate: true`**：当**整批**工具都标记 terminate 时，`hasMoreToolCalls = false`，内层循环退出
- **`shouldStopAfterTurn`**：turn_end 后回调返回 true，直接 break 外层循环
- **错误/中止**：LLM 返回 `stopReason: "error"` 或 `"aborted"`，立即 exit

---

## 三、pi-ai LLM 抽象层

`@earendil-works/pi-ai` 是 Pi 的 LLM 驱动层，独立于 agent 包。

### 3.1 Provider 注册表

```
ApiRegistry
├── registerApiProvider()        ← 注册 provider
├── unregisterApiProviders()     ← 动态卸载
├── clearApiProviders()          ← 清空
└── 延迟加载：provider 首次使用时才 import()
```

### 3.2 内置 API（9 个）

| API | 说明 |
|-----|------|
| `anthropic-messages` | Anthropic Messages API |
| `openai-completions` | OpenAI Chat Completions |
| `openai-responses` | OpenAI Responses API |
| `azure-openai-responses` | Azure OpenAI Responses |
| `openai-codex-responses` | OpenAI Codex |
| `mistral-conversations` | Mistral Conversations |
| `google-generative-ai` | Google Generative AI |
| `google-vertex` | Google Vertex AI |
| `bedrock-converse-stream` | AWS Bedrock |

### 3.3 OpenAI 兼容层（25+ provider）

`OpenAICompletionsCompat` 提供 ~15 个自动检测标志，覆盖所有 OpenAI 兼容 provider：

amazon-bedrock, deepseek, github-copilot, xai, groq, cerebras, openrouter, vercel-ai-gateway, mistral, minimax, moonshotai, huggingface, fireworks, together, kimi-coding, cloudflare-workers-ai, zai 等。

### 3.4 流式调用入口

```typescript
// 统一入口（自动 reasoning 支持）
streamSimple(model, context, options): EventStream<AssistantMessageEvent>

// 原始入口（provider 特定选项）
stream(model, context, options): EventStream<AssistantMessageEvent>

// 同步等待结果
complete() / completeSimple()
```

### 3.5 高级 LLM 特性

| 特性 | 说明 |
|------|------|
| **Transport** | `sse | websocket | websocket-cached | auto` |
| **Cache Retention** | `none | short | long`，映射到 provider 特定参数（Anthropic `cache_control.ttl`，OpenAI `prompt_cache_retention`） |
| **ThinkingBudgets** | per-level token 预算（minimal/low/medium/high） |
| **ThinkingLevel clamping** | `clampThinkingLevel()` 自动降级到模型最近支持级别 |
| **Session ID** | 贯穿全栈用于 provider cache affinity |
| **ThinkingLevel 映射** | `Model.thinkingLevelMap` 将 Pi 级别映射到 provider 特定值（如 Anthropic 的 budget tokens） |

---

## 四、AgentMessage 抽象

Pi 的核心设计之一是 `AgentMessage` —— 比 LLM 原生 `Message` 更丰富的消息类型：

### 4.1 类型系统

```typescript
// LLM 原生消息
type Message = UserMessage | AssistantMessage | ToolResultMessage

// AgentMessage = LLM 消息 + 应用自定义消息（通过 TypeScript declaration merging）
type AgentMessage = Message | CustomAgentMessages[keyof CustomAgentMessages]
```

### 4.2 内置自定义消息类型（4 种）

| 类型 | role | 关键字段 | 说明 |
|------|------|---------|------|
| `BashExecutionMessage` | `"bashExecution"` | command, output, exitCode, cancelled, truncated, `excludeFromContext?` | bash 执行结果；`excludeFromContext=true` 时从 LLM context 静默丢弃 |
| `CustomMessage<T>` | `"custom"` | customType, content, display, details | 通用类型化消息 |
| `BranchSummaryMessage` | `"branchSummary"` | summary, fromId | 分支切换时生成的摘要 |
| `CompactionSummaryMessage` | `"compactionSummary"` | summary, tokensBefore | 上下文压缩摘要 |

### 4.3 消息桥接：convertToLlm

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

## 五、事件系统

Agent 通过事件流驱动 UI 更新。

### 5.1 AgentEvent（10 种，四层）

| 层级 | 事件 | 含义 |
|------|------|------|
| **Agent** | `agent_start` / `agent_end` | 一次 prompt/continue 调用的生命周期 |
| **Turn** | `turn_start` / `turn_end` | 一轮 LLM 调用 + 工具执行 |
| **Message** | `message_start` / `message_update` / `message_end` | 单条消息生命周期 |
| **Tool** | `tool_execution_start` / `tool_execution_update` / `tool_execution_end` | 单个工具执行生命周期 |

### 5.2 AgentHarness 事件（20+ 种）

Harness 通过 `on(type, handler)` 注册带返回值的 hook 事件：

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

纯观察事件（无返回值）：`queue_update`, `save_point`, `abort`, `settled`, `session_compact`, `session_tree`, `model_select`, `thinking_level_select`, `resources_update`

### 5.3 事件序列示例

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

### 5.4 订阅语义

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

## 六、工具执行系统

### 6.1 AgentTool 定义

```typescript
interface AgentTool<TParameters, TDetails> {
    name: string;
    label: string;                // UI 展示名
    description: string;
    parameters: TSchema;          // TypeBox schema
    executionMode?: "sequential" | "parallel";  // 覆盖全局设置
    prepareArguments?: (args: unknown) => Static<TParameters>;  // 兼容性垫片
    execute: (
        toolCallId: string,
        params: Static<TParameters>,
        signal?: AbortSignal,
        onUpdate?: AgentToolUpdateCallback,  // 流式进度回调
    ) => Promise<AgentToolResult<TDetails>>;
}
```

### 6.2 AgentToolResult

```typescript
interface AgentToolResult<T = unknown> {
    content: (TextContent | ImageContent)[];  // 多模态内容
    details: T;                                // 结构化详情
    terminate?: boolean;                       // 终止标志
}
```

### 6.3 执行模式

| 模式 | 行为 |
|------|------|
| `sequential` | 逐个执行：prepare → validate → execute → finalize → 下一个 |
| `parallel` | prepare 串行（需 beforeToolCall 结果），execute 并发 `Promise.all`，`tool_execution_end` 按完成顺序发射，`toolResult` 消息按 assistant 源码顺序发射 |

混合规则：如果任何工具标记为 `sequential`，则**整批降级为串行**。

### 6.4 工具执行流水线

```
raw toolCall (来自 LLM)
    │
    ▼ prepareArguments()        ← 预处理参数（兼容性垫片）
    │
    ▼ validateToolArguments()   ← TypeBox 校验
    │
    ▼ beforeToolCall()          ← hook: 可 block 执行
    │
    ▼ tool.execute()            ← 实际执行，可流式 onUpdate
    │
    ▼ afterToolCall()           ← hook: 可覆盖 content/details/isError/terminate
    │
    ▼ createToolResultMessage() ← 构建标准 toolResult 消息
```

### 6.5 错误处理

**工具失败时抛出异常**，Agent 自动包装为 `isError: true` 的 toolResult 反馈给 LLM。

---

## 七、消息队列系统

### 7.1 三种队列

| 队列 | 注入时机 | 用途 |
|------|---------|------|
| **Steering** | 内层循环每轮 turn 后 | 用户中途修正 Agent 方向 |
| **Follow-up** | 内层循环完全退出后 | Agent 自然停止后追加新任务 |
| **NextTurn** | 首次进入内层循环前 | prepend 到下一轮 prompt |

### 7.2 QueueMode

| 模式 | 行为 |
|------|------|
| `"one-at-a-time"`（默认） | 每次只取一条，剩余保留 |
| `"all"` | 一次取完所有排队消息 |

Steering 和 Follow-up 各自独立的 QueueMode，通过 `setSteeringMode()` / `setFollowUpMode()` 配置。

### 7.3 队列操作

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

## 八、Agent 状态管理

### 8.1 AgentState

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

### 8.2 动态配置

```typescript
agent.state.model = newModel;           // 热切换模型
agent.state.thinkingLevel = "high";
agent.toolExecution = "sequential";     // 运行时切换执行模式
agent.beforeToolCall = async ({ toolCall }) => { ... };
```

---

## 九、Turn 生命周期控制

### 9.1 shouldStopAfterTurn

`turn_end` 后检查，返回 `true` 则优雅退出：

```typescript
shouldStopAfterTurn: async ({ message, toolResults, context, newMessages }) => {
    return shouldCompactBeforeNextTurn(context.messages);
}
```

### 9.2 prepareNextTurn

`turn_end` 后可返回新的 context/model/thinkingLevel，实现跨轮次动态切换：

```typescript
prepareNextTurn: async ({ message, toolResults, context }) => {
    if (message.stopReason === "length") {
        return { model: largerModel, thinkingLevel: "high" };
    }
}
```

Harness 用此回调刷新 turn 间状态。

---

## 十、AgentHarness 完整 API

### 10.1 核心方法

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

### 10.2 运行时配置

| 方法 | 说明 |
|------|------|
| `setModel(model)` / `getModel()` | 切换 LLM 模型 |
| `setThinkingLevel(level)` / `getThinkingLevel()` | 切换思考级别 |
| `setActiveTools(names)` | 激活/停用工具 |
| `setTools(tools, activeNames?)` | 替换工具集 |
| `setResources(resources)` / `getResources()` | 更新技能和模板 |
| `setStreamOptions(opts)` / `getStreamOptions()` | LLM 流选项 |
| `setSteeringMode(mode)` / `setFollowUpMode(mode)` | 队列模式 |

### 10.3 事件注册

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

### 10.4 AgentHarness Phases

```typescript
type AgentHarnessPhase = "idle" | "turn" | "compaction" | "branch_summary" | "retry";
```

Phase 守卫：非 idle 时 `prompt()` 抛 `AgentHarnessError("busy")`。Phase 决定哪些操作被允许。

### 10.5 Pending Session Write

运行中的 session 写入（model change、thinking level change、custom entries、labels）缓存在 `pendingSessionWrites[]` 中，在 turn 边界、agent end、prepareNextTurn 时 flush。防止 mid-turn 状态变化导致 session 损坏。

---

## 十一、ExecutionEnv 环境抽象

`ExecutionEnv = FileSystem + Shell`，解耦 agent 与运行时环境。

### 11.1 FileSystem 接口

```typescript
interface FileSystem {
    readTextFile(path): Promise<Result<string>>;
    readTextLines(path, start, end): Promise<Result<string[]>>;
    readBinaryFile(path): Promise<Result<Uint8Array>>;
    writeFile(path, content): Promise<Result<void>>;
    appendFile(path, content): Promise<Result<void>>;
    fileInfo(path): Promise<Result<FileInfo>>;
    listDir(path): Promise<Result<DirEntry[]>>;
    canonicalPath(path): Promise<Result<string>>;
    exists(path): Promise<Result<boolean>>;
    createDir(path): Promise<Result<void>>;
    remove(path, options?): Promise<Result<void>>;
    createTempDir(): Promise<Result<string>>;
    createTempFile(): Promise<Result<string>>;
    // ...
}
```

所有操作返回 `Result<T>`（不抛异常），错误通过 `FileError` + stable error code 返回。

### 11.2 Shell 接口

```typescript
interface Shell {
    exec(command, options?: {
        timeout?: number;
        signal?: AbortSignal;
        onStdout?: (data: string) => void;
        onStderr?: (data: string) => void;
    }): Promise<ShellResult>;
}
```

### 11.3 参考实现

`NodeExecutionEnv` 实现了 `FileSystem + Shell`，提供完整的 Node.js 运行时环境。

---

## 十二、Session 树状模型

### 12.1 数据结构

会话是**树状条目**，不是平坦列表：

```typescript
interface SessionTreeEntry {
    type: EntryType;
    id: string;           // UUIDv7（时间可排序）
    parentId: string | null;  // 形成树结构
    timestamp: string;    // ISO 格式
}
```

**10 种条目类型**：

| 类型 | 说明 |
|------|------|
| `message` | 用户/助手/工具消息 |
| `thinking_level_change` | 思考级别变更 |
| `model_change` | 模型变更 |
| `compaction` | 压缩摘要 |
| `branch_summary` | 分支摘要 |
| `custom` | 自定义数据 |
| `custom_message` | 自定义消息 |
| `label` | 标签标记 |
| `session_info` | session 元数据 |
| `leaf` | 当前活跃叶节点指针 |

### 12.2 核心操作

```typescript
interface Session {
    getBranch(fromId?): SessionTreeEntry[];   // 从 leafId 到 root 的路径
    moveTo(entryId, summary?): void;          // 切换活跃叶节点，可选生成摘要
    buildContext(): SessionContext;            // 重建消息数组 + effective model/thinkingLevel
    fork(options?): Session;                  // 从分支点创建新 session
}
```

分支是**隐含的**（通过 `leafId` 指向不同路径），不是显式对象。`buildContext()` 自动处理压缩条目（找到最近 CompactionEntry，注入 CompactionSummaryMessage，跳过旧条目）。

### 12.3 存储

| 后端 | 用途 |
|------|------|
| `JsonlSessionStorage` | 生产环境（CWD 编码目录结构） |
| `InMemorySessionStorage` | 测试环境 |

Model/ThinkingLevel 变更作为 session entry 持久化，`buildContext()` 回放恢复。

### 12.4 Label 系统

`LabelEntry` 允许为条目打标签，`getLabel()` 通过 label cache 高效查找。

---

## 十三、Compaction（上下文压缩）

### 13.1 压缩流程

```
compact()
├── shouldCompact()              ← contextTokens > contextWindow - reserveTokens
│                                 reserveTokens=16384, keepRecentTokens=20000
├── findCutPoint()              ← 累积 token 向前找到截断位置（必须在 turn 边界）
├── prepareCompaction()         ← 构造压缩请求
│   ├── 检测 split-turn（cut 跨 turn 中间）
│   ├── 提取 file operations（read/write/edit）
│   └── 复用 previousSummary（增量更新）
├── generateSummary()           ← LLM 生成结构化摘要
│   ├── 首次：SUMMARIZATION_PROMPT
│   ├── 增量：UPDATE_SUMMARIZATION_PROMPT
│   └── split-turn 时：Promise.all([历史摘要, turn-prefix 摘要])
└── appendEntry(compactionEntry) ← 持久化到 session tree
```

### 13.2 摘要格式（8 节）

```
## Goal
## Constraints & Preferences
## Progress
### Done
### In Progress
### Blocked
## Key Decisions
## Next Steps
## Critical Context
```

增量更新语义：PRESERVE 已有、ADD 新信息、MOVE 项在 Done/In Progress 间、UPDATE Next Steps。

### 13.3 File Operation Tracking

`extractFileOpsFromMessage()` 分析 assistant tool call 中的 read/write/edit 操作，跨压缩边界累积，写入摘要的 `<files_read>` / `<files_modified>` XML 标签。压缩后模型仍知道之前操作过哪些文件。

### 13.4 Token 估算

混合策略：
- **Provider usage 优先**：使用最近 assistant message 的 `usage.totalTokens`
- **Chars/4 兜底**：provider 未报告时使用字符启发式（images 固定 4800 tokens）

### 13.5 Branch Summarization

导航到不同分支时，`collectEntriesForBranchSummary()` 找到公共祖先，收集旧分支独有条目，生成结构化摘要。前缀 `"The user explored a different conversation branch before returning here."`

---

## 十四、Skills 系统

### 14.1 技能加载

从 `.pi/skills/*.md` 或 `SKILL.md` 加载，YAML frontmatter 定义元数据：

```yaml
---
name: git-release
description: Create git releases
disable-model-invocation: true   # 仅应用可用，模型不可见
---
```

- 递归目录遍历（尊重 `.gitignore` / `.ignore` / `.fdignore`）
- `loadSourcedSkills()` 支持 tagged provenance
- 诊断系统（`SkillDiagnostic` codes）

### 14.2 技能注入

`formatSkillsForSystemPrompt()` 生成 `<available_skills>` XML 块注入 system prompt。`formatSkillInvocation()` 包裹内容 + 位置上下文。

---

## 十五、Prompt Templates 系统

从 `.md` 文件 + YAML frontmatter 加载模板：

```
promptFromTemplate("refactor", "src/lib.rs --dry-run")
    → 加载 refactor.md
    → 替换 $1 → "src/lib.rs"
    → 替换 $@ → "src/lib.rs --dry-run"
```

占位符：`$1` / `$@` / `$ARGUMENTS` / `${@:N}` / `${@:N:L}`

Shell 风格参数解析（支持引号），通过 `promptFromTemplate()` 调用。

---

## 十六、Resources 系统

`AgentHarnessResources<TSkill, TPromptTemplate>` 是 skills 和 templates 的泛型容器：

- 每个 turn 开始时快照当前 resources，传给 system prompt callback
- 应用自行管理加载/重载，调用 `setResources()` 更新
- 变更时发射 `resources_update` 事件

---

## 十七、Proxy Stream 架构

`streamProxy()` 支持通过后端服务器路由 LLM 调用（非客户端直连）：

```
客户端                         服务端
┌──────────────┐              ┌──────────────┐
│  streamProxy │─── SSE ────→│  LLM Provider│
│  解析 delta  │              │  API Key 管理 │
│  重建消息    │←── events ──│  速率限制     │
│  带宽优化    │              │  审计日志     │
└──────────────┘              └──────────────┘
```

- 自定义 SSE 解析和 partial message 重建
- 带宽优化：delta 事件中剥离 `partial` 字段
- `ProxyAssistantMessageEvent` 类型（缩减 payload）
- 适用场景：服务端认证、速率限制、审计日志

---

## 十八、Stream Options 管理

`AgentHarnessStreamOptions` 提供对 LLM 请求参数的细粒度控制：

| 字段 | 说明 |
|------|------|
| `transport` | `"sse" | "websocket" | "websocket-cached" | "auto"` |
| `timeout` | 请求超时 |
| `retries` / `retryDelayCap` | 重试策略 |
| `headers` | 自定义 HTTP 头 |
| `metadata` | 请求元数据 |
| `cacheRetention` | `"none" | "short" | "long"` |

每个 turn 开始时快照 options，`before_provider_request` hook 可 patch，然后传给 stream function。

---

## 十九、错误层级

6 种结构化错误类 + stable error codes：

| 错误类 | 场景 |
|--------|------|
| `FileError` | 文件操作失败 |
| `ExecutionError` | shell 命令失败 |
| `CompactionError` | 上下文压缩失败 |
| `BranchSummaryError` | 分支摘要失败 |
| `SessionError` | 会话操作失败 |
| `AgentHarnessError` | harness 操作失败（如 busy guard） |

---

## 二十、Agent 类生命周期管理

### 20.1 运行保护

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

### 20.2 ActiveRun 追踪

```typescript
type ActiveRun = {
    promise: Promise<void>;
    resolve: () => void;
    abortController: AbortController;
};
```

### 20.3 Abort 机制

```typescript
agent.abort();              // 触发 AbortController
await agent.waitForIdle();  // 等待 run 完全结束（含 agent_end 监听器）
agent.reset();              // 清空 transcript、runtime state、queues
```

### 20.4 错误恢复

run 内部未捕获异常时：自动合成 `agent_end` 事件（含错误信息）→ 清理运行时状态 → 恢复 idle。

### 20.5 getApiKey 动态解析

`AgentLoopConfig.getApiKey` 每次 LLM 调用前解析 API key，支持 OAuth 短期 token 刷新（如 GitHub Copilot）。

---

## 二十一、低层 API：agentLoop()

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

## 二十二、Shell 输出处理

`executeShellWithCapture()` 提供流式输出捕获：

- **自动截断**：50KB 默认限制，超出部分溢出到临时文件
- **二进制清理**：非 UTF-8 内容自动过滤
- **行级截断**：`truncateHead()` / `truncateTail()` 支持 line/byte 限制
- **grep 输出**：`truncateLine()` 500 字符限制

---

## 二十三、核心设计决策

| 决策 | 内容 | 理由 |
|------|------|------|
| **三层架构** | Harness → Agent → agentLoop | 分离持久化、状态管理、纯执行 |
| **双层循环** | 外层 follow-up，内层 tool-call + steering | 分离"修正方向"和"追加任务" |
| **AgentMessage 抽象** | TypeScript declaration merging 扩展 | 编译期类型安全，支持非 LLM 消息 |
| **convertToLlm 桥接** | 在 LLM 调用边界转换 | 内部保持富类型，LLM 只看到标准消息 |
| **事件驱动** | 10 种 AgentEvent + 20+ Harness 事件 | UI 精确响应 + hook 扩展 |
| **Hook 返回值语义** | 事件监听器可返回 typed result | 非侵入式行为修改（block/patch/cancel） |
| **工具抛出异常** | execute() 失败时 throw | Agent 自动包装为 isError |
| **ExecutionEnv 抽象** | FileSystem + Shell 接口 | 解耦运行环境，支持沙箱/远程 |
| **Proxy Stream** | 服务端路由 LLM 调用 | 企业部署（认证/审计/限流） |
| **Session 树** | parentId 链形成会话树 | 分支/fork/导航，非平坦日志 |
| **Pending Write** | turn 边界 flush | 并发安全，防止 mid-turn 损坏 |
| **Parallel 执行** | execute 并发，事件按完成顺序 | 减少延迟，UI 实时反馈 |
| **QueueMode one-at-a-time** | 默认每次只取一条 | 防止大量消息淹没 Agent |

---

## 二十四、模块依赖关系

```
packages/
├── agent/
│   ├── src/
│   │   ├── agent.ts                ← Agent 类（有状态封装）
│   │   ├── agent-loop.ts           ← runAgentLoop() 核心引擎
│   │   ├── types.ts                ← 全部类型定义
│   │   ├── index.ts                ← 公共导出
│   │   ├── proxy.ts                ← 服务端 LLM 路由
│   │   └── harness/                ← 生产编排层
│   │       ├── agent-harness.ts    ←   AgentHarness 完整 API
│   │       ├── types.ts            ←   20+ hook 事件 + 类型
│   │       ├── env/
│   │       │   └── nodejs.ts       ←   NodeExecutionEnv 实现
│   │       ├── compaction/
│   │       │   ├── compaction.ts   ←   上下文压缩 + split-turn
│   │       │   ├── branch-summarization.ts ← 分支摘要
│   │       │   └── utils.ts        ←   file operation tracking
│   │       ├── session/
│   │       │   ├── session.ts      ←   树状会话模型
│   │       │   ├── jsonl-repo.ts   ←   JSONL 存储后端
│   │       │   └── memory-repo.ts  ←   内存存储后端
│   │       ├── messages.ts         ←   消息转换 + 自定义类型
│   │       ├── prompt-templates.ts ←   模板加载与占位符
│   │       ├── skills.ts           ←   技能加载与注入
│   │       ├── system-prompt.ts    ←   系统提示词构建
│   │       └── utils/
│   │           ├── shell-output.ts ←   shell 输出捕获/截断
│   │           └── truncate.ts     ←   通用截断工具
│   └── test/
└── ai/
    ├── src/
    │   ├── types.ts                ← LLM 类型（Model, Context, ThinkingLevel）
    │   ├── api-registry.ts         ← provider 注册表 + 延迟加载
    │   ├── stream.ts               ← 流式调用核心
    │   ├── models.ts               ← ThinkingLevel 映射 + clamping
    │   ├── providers/
    │   │   ├── register-builtins.ts ← 9 个内置 API 注册
    │   │   ├── openai-completions/  ← OpenAI + 25+ 兼容 provider
    │   │   ├── anthropic-messages/  ← Anthropic Messages API
    │   │   ├── google/              ← Google GenAI + Vertex
    │   │   ├── mistral/             ← Mistral Conversations
    │   │   └── bedrock/             ← AWS Bedrock
    │   └── utils/
    │       └── event-stream.ts     ← EventStream 泛型
    └── test/
```

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
