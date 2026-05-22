# earendil-works/pi 的架构哲学

> 基于实际源码对微信公众号文章[《从 pi-main 源码拆解：顶尖 AI Agent 的工程设计（17 维度全解）》](https://mp.weixin.qq.com/s/h8HZyoyOOX2Aodfngq25FA?scene=1)的逐维度修正。原文存在 2 处事实错误、多处关键简化，本文以 [Pi 源码](https://github.com/earendil-works/pi) 为唯一事实来源进行修正。
>
> 源码版本：`packages/engine/src/` 及 `packages/ai/src/`

---

## 一、任务规划：单体还是多体

**原文判定**：基本准确。

Pi 默认为单体 ReAct Agent，无内置 Planner/Router。多 Agent 协作通过 Extension API 将「调用子 Agent」封装为普通工具实现。

**补充**：Pi 的 Agent 类（`agent.ts`）通过 `PendingMessageQueue` 管理消息队列，队列有两种模式：

```typescript
// packages/agent/src/agent.ts
class PendingMessageQueue {
  constructor(public mode: QueueMode) {} // "all" | "one-at-a-time"

  drain(): AgentMessage[] {
    if (this.mode === "all") {
      const drained = this.messages.slice();
      this.messages = [];
      return drained;
    }
    // "one-at-a-time" 模式：每次只取第一条
    const first = this.messages[0];
    if (!first) return [];
    this.messages = this.messages.slice(1);
    return [first];
  }
}
```

steering 和 followUp 队列默认均为 `"one-at-a-time"` 模式——每次内循环迭代只注入一条用户消息，避免单次上下文过载。

---

## 二、核心主循环：双层 while

**原文判定**：核心结构正确，但缺少关键细节。

### 原文代码的修正版

```typescript
// packages/engine/src/agent-loop.ts — runLoop()
async function runLoop(
  initialContext: AgentContext,
  newMessages: AgentMessage[],
  initialConfig: AgentLoopConfig,
  signal: AbortSignal | undefined,
  emit: AgentEventSink,
  streamFn?: StreamFn,
): Promise<void> {
  let currentContext = initialContext;
  let config = initialConfig;

  // 启动时即检查是否有 steering 消息
  let pendingMessages: AgentMessage[] =
    (await config.getSteeringMessages?.()) || [];

  while (true) {                          // 外层：followUp 驱动
    let hasMoreToolCalls = true;

    while (hasMoreToolCalls || pendingMessages.length > 0) { // 内层：ReAct
      // 注入 steering 消息
      if (pendingMessages.length > 0) {
        for (const message of pendingMessages) {
          await emit({ type: "message_start", message });
          await emit({ type: "message_end", message });
          currentContext.messages.push(message);
          newMessages.push(message);
        }
        pendingMessages = [];
      }

      const message = await streamAssistantResponse(...);
      const toolCalls = message.content.filter(c => c.type === "toolCall");

      hasMoreToolCalls = false;
      if (toolCalls.length > 0) {
        const executedToolBatch = await executeToolCalls(...);
        hasMoreToolCalls = !executedToolBatch.terminate;
        // ...
      }

      // ★ 原文遗漏：每个 turn 结束后重新轮询 steering
      pendingMessages = (await config.getSteeringMessages?.()) || [];
    }

    // 外层检查 followUp
    const followUpMessages = (await config.getFollowUpMessages?.()) || [];
    if (followUpMessages.length > 0) {
      pendingMessages = followUpMessages;
      continue;
    }
    break;
  }
}
```

### 关键补充

**1. Steering 轮询时机**：原文只展示了入口处一次轮询。实际源码在**每个 turn 结束后**都会重新轮询 `getSteeringMessages`，确保用户中途注入的消息不会丢失。

**2. terminate 信号语义**：

```typescript
function shouldTerminateToolBatch(
  finalizedCalls: FinalizedToolCallOutcome[]
): boolean {
  return finalizedCalls.length > 0
    && finalizedCalls.every(f => f.result.terminate === true);
}
```

原文说「工具执行完后可以返回 `terminate=true`」没错，但**没有强调是 AND 语义**：批次中**所有**工具都必须返回 `terminate=true` 才会停止。只要有一个工具没有声明终止，循环继续。这是一个微妙但重要的设计——默认保守，不会因为单个工具的终止信号就中断整个批次。

**3. 错误/中断提前退出**：

```typescript
if (message.stopReason === "error" || message.stopReason === "aborted") {
  await emit({ type: "turn_end", message, toolResults: [] });
  await emit({ type: "agent_end", messages: newMessages });
  return; // 直接退出，不进入外层 followUp 检查
}
```

原文未提及：当 LLM 返回 `error` 或 `aborted` stopReason 时，循环立即退出，不会走 followUp 流程。

**4. `prepareNextTurn` 钩子**：原文未提及。每个 turn 结束后可调用 `config.prepareNextTurn` 返回新的 context/model/reasoning 配置，支持动态切换模型或调整推理级别。

---

## 三、反思与自我纠错

**原文判定**：准确。

统一返回结构 `{ content, isError }` 是 Pi 的核心设计。LLM 看到错误信息后自行推理是否重试，无需额外 Critic Agent。

**补充**：错误返回的创建有标准化函数：

```typescript
function createErrorToolResult(message: string): ToolResultMessage {
  return {
    role: "toolResult",
    content: [{ type: "text", text: message }],
    isError: true,
  };
}
```

---

## 四、工具系统：Schema + 截断 + 执行模式

**原文判定**：基本准确，但遗漏了一条重要的执行模式路径。

### 执行模式判定逻辑（原文简化版 vs 源码）

原文展示的代码：
```typescript
const hasSequentialTool = toolCalls.some(
  tc => tool.executionMode === "sequential"
);
if (hasSequentialTool) return executeToolCallsSequential(...);
return executeToolCallsParallel(...);
```

实际源码：
```typescript
// packages/engine/src/agent-loop.ts — executeToolCalls()
const hasSequentialToolCall = toolCalls.some(
  (tc) => currentContext.tools?.find(
    (t) => t.name === tc.name
  )?.executionMode === "sequential"
);

if (config.toolExecution === "sequential" || hasSequentialToolCall) {
  return executeToolCallsSequential(...);
}
return executeToolCallsParallel(...);
```

**差异**：原文遗漏了 `config.toolExecution === "sequential"` 全局覆盖路径。实际有**两条**路进入串行模式：
1. **全局配置**：`config.toolExecution` 设为 `"sequential"`，强制所有工具串行
2. **工具声明**：批次中任一工具声明 `executionMode: "sequential"`

原文只展示了第 2 条路径。这很重要——全局覆盖允许运行时动态切换执行策略，而无需修改工具定义。

---

## 五、输出格式与参数校验

### ❌ 原文错误

> "pi 在输出解析上**完全信任 Provider 原生 Function Calling**，没有额外的 Schema 校验层。"

### ✅ 源码事实

Pi **有完整的参数校验层**，使用 TypeBox 编译的 Schema 验证器：

```typescript
// packages/ai/src/utils/validation.ts
export function validateToolArguments(tool: Tool, toolCall: ToolCall): any {
  const args = structuredClone(toolCall.arguments);
  Value.Convert(tool.parameters, args);  // TypeBox 内置类型转换

  const validator = getValidator(tool.parameters);

  // 处理纯 JSON Schema（非 TypeBox）的额外转换
  if (!hasTypeBoxMetadata(tool.parameters) && isJsonSchemaObject(tool.parameters)) {
    const coerced = coerceWithJsonSchema(args, tool.parameters);
    if (coerced !== args) {
      // ...合并转换后的值
    }
  }

  if (validator.Check(args)) {
    return args; // 校验通过，返回（可能经过转换的）参数
  }

  // 校验失败：收集所有错误，抛出详细描述
  const errors = validator
    .Errors(args)
    .map(error => `  - ${formatValidationPath(error)}: ${error.message}`)
    .join("\n");

  throw new Error(
    `Validation failed for tool "${toolCall.name}":\n${errors}\n\n` +
    `Received arguments:\n${JSON.stringify(toolCall.arguments, null, 2)}`
  );
}
```

校验流程：
1. `structuredClone` 避免原参数被修改
2. `Value.Convert` 执行 TypeBox 内置类型转换（如字符串 "123" → 数字 123）
3. 对于纯 JSON Schema，额外执行 `coerceWithJsonSchema`（处理 `allOf`/`anyOf`/`oneOf` 嵌套、基本类型转换）
4. `validator.Check(args)` 编译后的类型检查
5. 失败时抛出包含完整错误路径和原始参数的 `Error`

**`validateToolArguments` 在 `prepareToolCall` 中被调用**：

```typescript
// packages/engine/src/agent-loop.ts — prepareToolCall()
async function prepareToolCall(...) {
  const tool = currentContext.tools?.find(t => t.name === toolCall.name);
  if (!tool) {
    return { kind: "immediate", result: createErrorToolResult(`Tool ${toolCall.name} not found`), isError: true };
  }

  const preparedToolCall = prepareToolCallArguments(tool, toolCall);
  const validatedArgs = validateToolArguments(tool, preparedToolCall); // ★ 校验在这里
  // ...
}
```

**结论**：原文关于「没有额外的 Schema 校验层」的论述是**错误的**。Pi 有一个功能完备的校验管线，包含 TypeBox 编译验证 + JSON Schema 兼容转换 + 详细错误报告。原文后续建议「在 `validateToolArguments` 前面加一层宽松解析」暴露了矛盾——既然说没有校验层，又引用了函数名。

---

## 六、记忆与上下文压缩

**原文判定**：准确。

`DEFAULT_COMPACTION_SETTINGS`、`calculateContextTokens`、`SUMMARIZATION_PROMPT` 格式与源码完全一致：

```typescript
// packages/engine/src/harness/compaction/compaction.ts
export const DEFAULT_COMPACTION_SETTINGS: CompactionSettings = {
  enabled: true,
  reserveTokens: 16384,
  keepRecentTokens: 20000,
};

export function calculateContextTokens(usage: Usage): number {
  return usage.totalTokens || usage.input + usage.output + usage.cacheRead + usage.cacheWrite;
}
```

**补充**：

1. `SUMMARIZATION_SYSTEM_PROMPT` 明确要求 LLM 只做摘要，不继续对话：

```typescript
export const SUMMARIZATION_SYSTEM_PROMPT =
  `You are a context summarization assistant. ...
   Do NOT continue the conversation. Do NOT respond to any questions in the conversation.
   ONLY output the structured summary.`;
```

2. `UPDATE_SUMMARIZATION_PROMPT` 用于迭代更新已有摘要，`previousSummary` 被包裹在 `<previous-summary>` XML 标签中传入。

3. `TURN_PREFIX_SUMMARIZATION_PROMPT` 处理 split turn（一次 turn 跨越压缩边界的情况）。

---

## 七、状态管理与会话分支

**原文判定**：准确。

JSONL append-only + `BranchSummaryMessage` 的描述与源码一致。

---

## 八、人机协同

### ❌ 原文错误（部分）

原文展示的代码：
```typescript
// 原文声称：
async steer(text: string): Promise<void> {
  this.agent.steer({ role: "user", content: [{ type: "text", text }] });
}
```

### ✅ 源码事实

```typescript
// packages/agent/src/agent.ts

/** Queue a message to be injected after the current assistant turn finishes. */
steer(message: AgentMessage): void {
  this.steeringQueue.enqueue(message);
}

/** Queue a message to run only after the agent would otherwise stop. */
followUp(message: AgentMessage): void {
  this.followUpQueue.enqueue(message);
}
```

**三个差异**：

| | 原文 | 源码 |
|---|---|---|
| `steer` 是否 async | `async` | **同步** |
| `steer` 返回类型 | `Promise<void>` | `void` |
| `steer` 参数类型 | `text: string` | `message: AgentMessage` |

`steer()` 是**同步方法**，不返回 Promise。它只是把消息塞进 `PendingMessageQueue`，不等待任何异步操作。这完全合理——消息入队不需要异步。

`followUp()` 同理——同步 `void`，入队到 followUp 队列。

**原文结论「steer 是异步的」是错误的**，但原文关于「AbortController 必须透传到底层」的论述是正确的。

---

## 九、提示词结构

**原文判定**：准确。

时空锚点（`Current date`、`Current working directory`）和 Guidelines 按工具动态生成的描述正确。

---

## 十、提示词动态组装

**原文判定**：准确。

Skill 机制（`/skill:debug` → 模板展开 → XML 标签包裹）的描述正确。

---

## 十一、多模态支持

**原文判定**：准确。

`read` 工具的图片自动 resize + 优雅降级描述正确。

---

## 十二、模型路由

**原文判定**：准确。

中途热切换写入 JSONL 事件流 + 自动重试正则的描述正确。

---

## 十三、安全与权限

**原文判定**：准确。

Pi 确实没有文件路径 Jail、命令白名单、敏感信息脱敏。安全策略依赖基础设施层。

---

## 十四、可观测性

**原文判定**：准确。

JSONL 回放 + 缺少结构化成本追踪的描述正确。

---

## 十五、扩展性

**原文判定**：基本准确，但 `beforeToolCall` 返回值需要精确化。

原文未明确 `beforeToolCall` 的返回结构。源码中：

```typescript
// prepareToolCall() 中的调用
const beforeResult = await config.beforeToolCall(
  { assistantMessage, toolCall, args: validatedArgs, context: currentContext },
  signal,
);

if (beforeResult?.block) {
  return {
    kind: "immediate",
    result: createErrorToolResult(beforeResult.reason || "Tool execution was blocked"),
    isError: true,
  };
}
```

`beforeToolCall` 返回 `{ block?: boolean, reason?: string }`——当 `block` 为 `true` 时，工具执行被阻止，返回错误结果。`reason` 用于解释阻止原因。

---

## 十六、评估体系

**原文判定**：准确。

Pi 确实没有自动化 Evals / Golden Set / 回归测试框架。

---

## 十七、成本控制

**原文判定**：准确。

压缩参数和策略描述与源码一致。

---

## 修正总结

| 维度 | 判定 | 修正内容 |
|------|------|----------|
| 一、任务规划 | ✅ | 补充 `PendingMessageQueue` 的两种 drain 模式 |
| 二、核心主循环 | ⚠️ | 补充 steering 每 turn 轮询、terminate AND 语义、error/aborted 提前退出、`prepareNextTurn` 钩子 |
| 三、反思纠错 | ✅ | 补充 `createErrorToolResult` 标准化函数 |
| 四、工具系统 | ⚠️ | 补充 `config.toolExecution === "sequential"` 全局覆盖路径 |
| 五、输出格式 | ❌ **错误** | Pi **有** `validateToolArguments` 完整校验层，包含 TypeBox 编译验证 + JSON Schema 兼容转换 |
| 六、记忆压缩 | ✅ | 补充 `SUMMARIZATION_SYSTEM_PROMPT`、`TURN_PREFIX_SUMMARIZATION_PROMPT` |
| 七、会话分支 | ✅ | — |
| 八、人机协同 | ❌ **错误** | `steer()` 是**同步 `void`**，参数类型是 `AgentMessage` 不是 `string` |
| 九～十七 | ✅ | 十五补充 `beforeToolCall` 返回值结构 |

**核心修正**：
1. **维度五**是最大错误——原文声称「没有额外校验层」，但 `validateToolArguments` 提供了完整的 TypeBox 编译验证管线，且在 `prepareToolCall` 中被显式调用
2. **维度八**的 `steer()` 签名与源码不符——同步而非异步，参数是 `AgentMessage` 而非 `string`
3. **维度二、四**存在简化——terminate 的 AND 语义、steering 的每 turn 轮询、全局串行覆盖路径等关键细节被遗漏
