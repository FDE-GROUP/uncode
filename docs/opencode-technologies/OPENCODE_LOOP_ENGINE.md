# OpenCode 循环引擎

> SessionPrompt 编排、SessionProcessor 流处理、工具循环与压缩

---

## 1. 与 Pi 双层循环的对应关系

Pi / uncode 文档中的 **外层 follow-up + 内层 tool-call/steering** 在 OpenCode 中 **没有同名 API**，但行为等价：

| Pi 概念 | OpenCode 实现线索 |
|---------|-------------------|
| `agentLoop` / Turn | `SessionProcessor.process` 多次调用 `LLM.stream` 直至无工具或 `stop` |
| 工具结果回灌上下文 | AI SDK `streamText` + `MessageV2` 转 `ModelMessage` |
| Steering / 中途输入 | 会话状态 + Prompt 重入（非 Pi 三队列命名） |
| Compaction | `SessionCompaction` + `session.next.compaction.*` |
| Doom loop 检测 | `DOOM_LOOP_THRESHOLD = 3`（`processor.ts`） |

OpenCode 将 **编排**（`prompt.ts`，2000+ 行）与 **流消费**（`processor.ts`）拆分，而非 Pi 的单一 `agent-loop.ts`。

---

## 2. SessionPrompt（编排层）

**文件**：`packages/opencode/src/session/prompt.ts`

职责概览：

1. 解析用户 Prompt（含 `@` 引用、`FileAttachment`、`ReferenceAttachment`）。
2. 选择 **Agent** 与 **Model**（`Agent.get`、`Provider`）。
3. 组装 **系统提示**（`SystemPrompt`、`Instruction`、Agent 专用 txt、结构化输出约束）。
4. 通过 **ToolRegistry.tools** 生成 AI SDK `tool()` 列表（含 MCP、Plugin、LSP）。
5. 创建 **SessionProcessor** Handle，驱动 `process(streamInput)`。
6. 处理 **命令**（`Command.Default.INIT` 等）、**plan/build 切换**、**子任务 TaskTool**。

关键依赖：`SessionCompaction`、`Permission`、`MCP`、`Plugin`、`SessionRevert`、`SessionSummary`。

---

## 3. SessionProcessor（执行层）

**文件**：`packages/opencode/src/session/processor.ts`

### 3.1 核心类型

```typescript
export type Result = "compact" | "stop" | "continue"

export interface Handle {
  readonly message: MessageV2.Assistant
  readonly updateToolCall: (toolCallID, update) => Effect<…>
  readonly completeToolCall: (toolCallID, output) => Effect<void>
  readonly process: (streamInput: LLM.StreamInput) => Effect<Result>
}
```

- **`process`**：消费 `LLM.stream` 发出的事件，更新 `Part` 表，执行工具，发布 `SessionEvent`。
- **返回值**：`continue` 继续工具轮次；`compact` 触发上下文压缩；`stop` 结束本轮。

### 3.2 流事件处理（概念）

```
LLM.stream(streamInput)
    → text / reasoning deltas → TextPart / ReasoningPart + session.next.text.*
    → tool-call lifecycle     → ToolPart + session.next.tool.*
    → step boundaries         → session.next.step.*
    → 工具执行完成            → completeToolCall → 结果写入 Part
    → 需压缩 / 溢出           → needsCompaction → SessionCompaction
    → 重复工具模式            → doom loop 计数 → 可能 stop
```

### 3.3 Snapshot 时机

Processor 在 **LLM 流开始前** 预捕获 snapshot（注释说明：AI SDK 可能在 `start-step` 之前内部执行工具），用于 revert/diff 与文件变更追踪。

### 3.4 常量

- `DOOM_LOOP_THRESHOLD = 3`：连续相似工具调用保护。

---

## 4. LLM 调用边界

**文件**：`packages/opencode/src/session/llm.ts`

- 使用 Vercel **`streamText`** / 相关 AI SDK API。
- `ProviderTransform` 处理各供应商差异（与 `@opencode-ai/llm` 适配器哲学一致：quirks 不下沉到业务）。
- `isOverflow`（`overflow.ts`）检测上下文溢出，联动压缩；溢出检测思路参考 **pi-mono**（`provider/error.ts` 注释）。

`SessionProcessor` 的 `Event` 类型别名自 `LLM.Event`，保证处理器与 LLM 层事件形状一致。

---

## 5. 压缩（Compaction）

- **模块**：`session/compaction.ts`、`SessionCompaction` Service。
- **事件**：`session.next.compaction.started` / `delta` / `ended`。
- **触发**：Processor 置 `needsCompaction` 或上下文溢出后返回 `compact`。

与 Pi **CompactionSummary** 消息 / uncode **`SessionEntry::Compaction`** 不同：OpenCode 以 **会话时间戳 + 事件流** 表达压缩过程，摘要内容落在 Message/Part 数据中。

---

## 6. 子 Agent（Task）

**TaskTool**（`tool/task.ts`）：

- 创建 **子 session**（`parent_id` 指向父会话）。
- 在子会话中运行独立 Prompt（`TaskPromptOps`）。
- 父会话通过工具结果获得子任务输出。

这是 OpenCode **内建多 Agent** 路径，区别于 Pi 推荐的外部多进程方案。

---

## 相关文档

- [OPENCODE_SESSION_MODEL.md](OPENCODE_SESSION_MODEL.md)
- [OPENCODE_LLM_LAYER.md](OPENCODE_LLM_LAYER.md)
- [OPENCODE_EVENT_SYSTEM.md](OPENCODE_EVENT_SYSTEM.md)
- [../pi-technologies/PI_LOOP_ENGINE.md](../pi-technologies/PI_LOOP_ENGINE.md)（Pi 双层循环参考）
