# uncode ↔ Pi 机制对照（L1）

> **一页纸速查**：uncode 与 [Pi](../pi-technologies/) 在 **机制层（L1）** 的对齐关系。  
> **不表示** API 兼容、存储格式一致或可直接移植代码。  
> 策略背景：[术语对齐策略](../technologies/TERMINOLOGY_ALIGNMENT_STRATEGY.md) · Epic [#255](https://github.com/FDE-GROUP/uncode/issues/255)

| 项 | 说明 |
|----|------|
| **文档类型** | 机制对照 / Mechanism map |
| **路径** | `docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md` |
| **uncode 源码** | `crates/uncode-agent/`、`crates/uncode-core/` |
| **Pi 参考** | `packages/agent`（agentLoop、session） |
| **最后更新** | 2026-05 |

---

## 1. 总览

| 机制 | Pi（L1 概念） | uncode（L2 实现） | 对齐度 |
|------|---------------|-------------------|--------|
| 编排外壳 | `AgentHarness` | `AgentHarness` + `LoopEngine` | 高 |
| 主循环 | `agentLoop` 双层 `while` | `LoopEngine::run_inner`（`'outer` + `while`） | 高 |
| 中途纠偏 | `steering` 队列 | `MessageQueue` steering 通道 | 高 |
| 会话延续 | `followUp` 队列 | `MessageQueue` follow_up 通道 | 高 |
| 预排队 | `nextTurn` 队列 | `MessageQueue` next_turn 通道 | 高 |
| 会话树 | `SessionTreeEntry` + `leafId` | `SessionEntry` + leaf 指针 | 逻辑同构 |
| 上下文重建 | `buildContext()` | `build_context()` → `BuiltContext` | 高 |
| 压缩 | Compaction + 迭代摘要 | `compact_if_needed` / `CompactionEntry` | 高 |
| 分支摘要 | `branch_summary` | `BranchSummary` 条目 | 高 |
| 事件 UI 解耦 | 订阅式事件流 | `broadcast::Sender<AgentEvent>` | 高（变体名有差异） |
| 持久化 | JSONL 文件主存 | SurrealDB 主存 + JSONL 互操作 | **工程取舍** |

OpenCode 为 **L3 能力对照**（非机制主参照），见 [OPENCODE_VS_PI](../technologies/OPENCODE_VS_PI.md) 与术语表 OpenCode 列。

---

## 2. 双层循环

```
Pi agentLoop                          uncode LoopEngine::run_inner
────────────────                      ─────────────────────────────
outer: followUp drain                 'outer: follow_up drain
  inner: tool-call loop                 inner while: tool-call loop
    steering drain (per turn)             steering drain (per turn)
    LLM stream + tools                    LLM stream + tools
    terminate AND (batch)                 terminate AND (batch)
  nextTurn drain (before inner)         next_turn drain (before inner)
```

| 阶段 | Pi | uncode |
|------|-----|--------|
| 会话初始化 | session load / header | `SessionStore` + `append_entry` |
| 用户消息持久化 | append message entry | 同上 |
| 构建 LLM 上下文 | `buildContext()` | `build_context()` |
| 系统提示 | system prompt bundle | `SystemPromptBuilder` |
| 单轮上限 | turn 上限（配置） | `MAX_TURNS` |
| 并发保护 | — | `active_run`（`HarnessError::Busy`） |

详见 [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md)、[PI_LOOP_ENGINE](../pi-technologies/PI_LOOP_ENGINE.md)。

---

## 3. 三通道消息队列

| 通道 | Pi | uncode | 时机 |
|------|-----|--------|------|
| Steering | `steering` | `MessageQueue::steering` | 每 turn 结束后 drain，中途纠偏 |
| Follow-up | `followUp` | `MessageQueue::follow_up` | 内层循环退出后 drain，延续会话 |
| Next turn | `nextTurn` | `MessageQueue::next_turn` | 进入内层前 drain，预排队 |

TUI 在 `agent_busy` 时将用户输入路由到 Follow-up/Steering，见 [UNCODE_REQUEST_LIFECYCLE](UNCODE_REQUEST_LIFECYCLE.md)。

---

## 4. SessionEntry ↔ Pi 条目类型

| Pi `EntryType` | uncode `SessionEntry` | 备注 |
|----------------|----------------------|------|
| `message` | `Message` | 用户/助手/工具 |
| `thinking_level_change` | `ThinkingLevelChange` | |
| `model_change` | `ModelChange` | |
| `compaction` | `Compaction` | 压缩摘要边界 |
| `branch_summary` | `BranchSummary` | 遗弃分支摘要 |
| `custom` / `custom_message` | `Custom` / `CustomMessage` | |
| `label` | `Label` | |
| `session_info` | `SessionInfo` | |
| `leaf` | leaf 指针（`set_leaf` / `get_leaf_id`） | 活跃路径 |
| `branch`（显式） | `Branch` 条目 | uncode 可显式记录分支元数据 |
| `system`（Pi 系统事件） | `System` 条目 | Start/End/PhaseSummary 等 |

**存储差异**：Pi 默认 JSONL 行文件；uncode 默认 **SurrealDB**，JSONL 仅导入/导出。逻辑回放路径一致，物理格式不同。

详见 [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md)、[PI_SESSION_MODEL](../pi-technologies/PI_SESSION_MODEL.md)。

---

## 5. AgentEvent ↔ Pi 事件（概念）

uncode 使用 **18 个** `AgentEvent` 变体（`uncode-core/src/event.rs`）。与 Pi 终端事件流 **语义对齐**，名称不一一对应。

| uncode `AgentEvent` | Pi 侧（概念） | 说明 |
|---------------------|---------------|------|
| `SessionStart` / `SessionEnd` | session lifecycle | 会话级 |
| `TurnStart` / `TurnEnd` | turn lifecycle | 含 `UsageInfo` |
| `MessageStart` / `MessageEnd` | message lifecycle | |
| `ContentDelta` | stream delta | Thinking / Text |
| `ToolCallStart` / `Progress` / `End` | tool execution | |
| `CompactionComplete` | compaction done | |
| `MessageQueued` / `MessageDelivered` | queue events | 三通道可见性 |
| `AgentInterrupted` | cancel / interrupt | |
| `AgentSettled` | idle after session end | |
| `Error` | error surface | `ErrorCategory` |
| `TaskUpdate` / `PhaseSummary` | （预留） | TUI 任务/阶段 |

完整序列与 Hook 见 [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md)。

---

## 6. 刻意不对齐（备忘）

| 项 | Pi | uncode | 原因 |
|----|-----|--------|------|
| 主存储 | JSONL | SurrealDB | 多面索引、Platform |
| 公开 Rust API 命名 | TS 驼峰 | snake_case | Rust 惯例（L2） |
| MCP 主路径 | 非主路径 | 非主路径 | 与 Pi 哲学一致 |
| OpenCode `session.next.*` | — | 不采用 | 非 Pi 机制 |

---

## 相关文档

| 文档 | 说明 |
|------|------|
| [UNCODE_TECHNOLOGIES_GLOSSARY](UNCODE_TECHNOLOGIES_GLOSSARY.md) | 词条级 Pi/OpenCode 列 |
| [UNCODE_PI_ALIGNMENT_AND_EVALUATION](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) | 深度对齐评价 |
| [TERMINOLOGY_LAYERED_REFACTOR_PLAN](../technologies/TERMINOLOGY_LAYERED_REFACTOR_PLAN.md) | 分阶段落地 |
| [PI_TECHNOLOGIES_GLOSSARY](../pi-technologies/PI_TECHNOLOGIES_GLOSSARY.md) | Pi 术语索引 |

---

*机制对照随 Phase 2（#261 事件矩阵细化）扩展；与源码冲突时以 `crates/` 为准。*
