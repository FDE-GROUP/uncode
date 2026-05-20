# uncode 微观规划（Micro-planning）能力说明

> 基于 `crates/uncode-agent` 等源码与 [`EXTENSION_COMPOSABLE_HARNESS_DESIGN.md`](../technologies/EXTENSION_COMPOSABLE_HARNESS_DESIGN.md) §2.3 术语澄清编写。  
> 回答：**uncode 是否已具备微观规划能力？** 与 **Plan 模式有何区别？**

| 项 | 说明 |
|----|------|
| **文档类型** | 实现层能力说明 / 架构澄清 |
| **路径** | `docs/uncode-technologies/UNCODE_MICRO_PLANNING.md` |
| **状态** | 与源码同步（2026-05） |
| **关联** | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md)、[EXTENSION_COMPOSABLE_HARNESS_DESIGN](../technologies/EXTENSION_COMPOSABLE_HARNESS_DESIGN.md) §2.3 |

---

## 1. 结论（摘要）

| 问题 | 答案 |
|------|------|
| uncode 是否具备**微观规划**？ | **是**。由 `AgentLoop` 的 ReAct 双环语义自然提供，无需独立 `micro_planning` 模块或 `PlanMode` 枚举。 |
| 是否存在名为「微观规划引擎」的 crate/API？ | **否**。能力体现在 **Turn 内 LLM 决策 + 工具链 + 多 Turn 串联**，非单独产品功能。 |
| 微观规划**强不强**？ | 取决于**模型**、**Thinking 配置**、**系统提示 / skills / 工作区上下文**、**工具注册表**；机制层已对齐 Pi。 |
| 是否等于 **Plan 模式**？ | **否**。Plan 模式是跨 Turn 的会话级工作流（只读相、工具收缩、用户闸门），见 §2。 |

---

## 2. 术语：微观规划 vs Plan 模式

避免将两种「规划」混谈。权威对照见 [`EXTENSION_COMPOSABLE_HARNESS_DESIGN.md`](../technologies/EXTENSION_COMPOSABLE_HARNESS_DESIGN.md) **§2.3**。

| 用语 | 英文 | 层级 | 含义 | uncode 现状 |
|------|------|------|------|-------------|
| **微观规划** | micro-planning (per turn) | Turn / ReAct | 单 Turn 内：模型选择工具、组织回复；多 Turn 间：根据工具结果决定下一步 | **已具备**（主路径） |
| **Plan 模式** | plan mode (workflow) | 扩展 / 会话 | 只读探索 → 结构化 `Plan:` → 用户确认 → 全工具执行 | **未实现**（哲学一致，扩展宿主未接线） |

**记忆句**：Turn 负责「走一步」；微观规划是这一步里的**决策**；Plan 模式是「这一段路是否只允许看地图」的**策略**，跨多个 Turn。

术语表条目见 [`UNCODE_TECHNOLOGIES_GLOSSARY.md`](UNCODE_TECHNOLOGIES_GLOSSARY.md) §四。

---

## 3. 微观规划在 uncode 中的机制定义

### 3.1 判定标准（本文采用）

满足以下即可认为 **具备微观规划**（与 Pi `agentLoop` 同构）：

1. **每个 Turn 至少一次 LLM 调用**，模型在完整上下文（含历史与工具 schema）下做下一步决策。
2. **模型可发起 0～N 个工具调用**（同一次流式响应内缓冲），宿主执行后将结果写回消息列表。
3. **工具未声明终止时**，内层循环进入下一 Turn，模型基于工具结果继续推理（多步任务链）。
4. **可选**：推理模型通过 Thinking 通道输出中间推理文本（非必需，但增强可观测性）。

uncode **不要求**：结构化 `Plan:` 输出、只读工具集、独立 Planner 模型、单 Turn 内「规划子阶段 + 执行子阶段」硬编码。

### 3.2 与 Turn 生命周期的关系

一个 Turn 在实现上对应（见 [`UNCODE_LOOP_ENGINE.md`](UNCODE_LOOP_ENGINE.md)）：

```text
TurnStart
  → 注入 pending（steering / next_turn 等）
  → build Context（messages + system_prompt + tools）
  → 可选 transform_context
  → LLM stream（text / thinking / tool_call_*）
  → 持久化 assistant 消息
  → 批量执行工具（并行或串行）
  → 工具结果写入 messages
TurnEnd
  → drain steering
  → 若 has_more_tool_calls → 下一 Turn（turn++）
```

**微观规划发生在「LLM stream」阶段**：模型在该时刻决定说什么、调什么工具。后续工具执行是**执行相**，仍属同一 Turn 的闭环，不是第二个规划阶段。

源码锚点：`crates/uncode-agent/src/loop_engine.rs`（模块注释写明 ReAct 工具链）。

---

## 4. 已实现能力（机制层）

### 4.1 ReAct 内层循环

- 外层：`follow_up` 会话延续。
- 内层：`has_more_tool_calls || !pending_messages.is_empty()`，与 Pi 一致。
- 每轮内层迭代：`turn += 1`，一次 `uncode_ai::stream`，一批工具执行。
- 工具批次结束后若未全体 `terminate`，设置 `has_more_tool_calls = true`，进入下一 Turn。

这是微观规划的**骨架**：没有该循环，则不存在「每步决策」。

### 4.2 单 Turn 内多工具决策

模型在一次 `Done` 前可缓冲多个 `ToolCallStart` / `ToolCallEnd`：

- 宿主将多个 `ToolCall` 写入同一条 assistant 消息。
- 执行策略：若批次中**任一**工具为 `ExecutionMode::Sequential`，则**整批串行**；否则 `join_all` **并行**执行。

集成测试 `test_agent_loop_multiple_tool_calls_in_one_turn`（`crates/uncode-agent/src/tests.rs`）验证：一轮 LLM 返回两个 `echo` → 两个 tool 结果 → 下一轮 LLM 文本收尾。

### 4.3 跨 Turn 链式推理

长任务典型轨迹：

```text
Turn 1: read / grep 探索
Turn 2: 根据结果 edit
Turn 3: bash 验证
Turn 4: 向用户总结
```

每格之间的「下一步做什么」由模型在**新 Turn 的 LLM 调用**中完成，仍属微观规划链条，**不是** Plan 模式的「规划相 / 执行相」状态机。

### 4.4 Thinking / 推理通道（可选）

| 组件 | 作用 |
|------|------|
| `ThinkingLevel` | 写入 `StreamOptions`，控制推理强度 |
| `model.reasoning` | 为真时默认倾向高档 thinking |
| `StreamEvent::ThinkingDelta` | 流式推理文本 |
| `ContentBlock::Thinking` | 持久化到 assistant 消息 |
| TUI `DeltaType::Thinking` | 用户可见推理过程 |

**注意**：无推理能力的模型仍可做微观规划（选工具 + 文本）；Thinking 仅增强**可观测性与推理质量**，非微观规划的前提。

### 4.5 支撑决策质量的上下文与工具面

| 能力 | 位置 | 对微观规划的意义 |
|------|------|------------------|
| `SystemPromptBuilder` | `uncode-agent/src/system_prompt.rs` | 拼装基础角色、工具说明、规则 |
| CLI 默认 system prompt | `uncode-cli/src/main.rs` | 引导工程任务与「主动使用工具」 |
| `ContextLoader` | AGENTS.md、skills | 项目规则与技能描述 |
| `workspace_graph` + `Bundle` | 可选 `graph_cache` | Turn 前注入工作区结构摘要 |
| `tool_registry.definitions()` | 全量工具 schema 传入 LLM | 模型可见完整工具菜单 |
| `compaction` | `compaction.rs` | 长会话压缩，保留近期上下文供后续 Turn 决策 |
| `steering` | `steering.rs` | Turn 间用户纠偏，影响后续 Turn 的输入 |
| `transform_context` | `AgentLoop::set_transform_context` | 发送 LLM 前改写 messages |
| `ToolHooks` | `uncode-core::ToolHooks` | 工具调用前后拦截/改写（扩展前奏） |

### 4.6 用户中途纠偏（Steering）

Steering 在 **TurnEnd 之后** drain 进 `pending_messages`，下一 Turn 开始时注入。不改变「微观规划 = 模型决策」的定义，但使用户能在多步链中**修正方向**，属于交互层对微观规划链的干预。

---

## 5. 未实现 / 不属于微观规划范畴

以下常被误认为「微观规划不够」，实则属于**其他层级**或**刻意外置**：

| 项 | 状态 | 说明 |
|----|------|------|
| 独立 Planner crate / 双模型「先规划再执行」 | ❌ | 无单 Turn 内强制两阶段 LLM 管线 |
| 系统提示强制 `Plan:` + 编号步骤 | ❌ | 属 Plan 模式扩展约定，非默认 prompt |
| 内置 todo / 步骤状态机 | ❌ | 与 Pi 一致，由扩展或产品层实现 |
| `set_active_tools` 只读工具集 | ❌ | 属 Plan **宏观**策略；P0 路线图见扩展设计文档 |
| 子 Agent 单 Turn 分解 | ❌ | 未在主路径接线 `uncode-extensions` |
| `PlanMode` 进 `AgentLoop` | ❌ | 违反小内核原则（§2.1 冻结） |

---

## 6. 能力分层判定表

评审或写 Issue 时可引用下表。

| 层级 | 问题 | uncode 现状 |
|------|------|-------------|
| **L0 机制** | 每 Turn 能否由 LLM 选工具并多 Turn 串联？ | ✅ `AgentLoop` / `loop_engine.rs` |
| **L1 协议** | 是否支持 thinking 流、多 tool call、并行/串行批次？ | ✅ `uncode-ai` + loop + `ExecutionMode` |
| **L2 提示与上下文** | 是否引导读代码、用工具、遵守项目规则？ | ✅ `SystemPromptBuilder`、CLI、`ContextLoader`、skills |
| **L3 宏观约束** | 是否强制只读探索 / 结构化计划 / 用户执行闸门？ | ❌ Plan 模式；非微观规划缺失 |
| **L4 可观测** | 用户能否看到推理过程？ | ⚠️ 依赖模型 + `ThinkingLevel` + TUI |

---

## 7. 与 Pi 的对齐

| 维度 | Pi | uncode |
|------|-----|--------|
| 微观规划载体 | `agentLoop` 内层 Turn + 工具链 | `AgentLoop::run_inner` 同构 |
| Plan 模式 | `examples/extensions/plan-mode` | 未实现；目标为扩展宿主 + 参考扩展 |
| 术语 | Turn = 机制；plan mode = 扩展 | 见 §2 与机制对照 [`UNCODE_PI_MECHANISM_MAP.md`](UNCODE_PI_MECHANISM_MAP.md) |

uncode **不追求**在核心复制 OpenCode 式 build/plan 双 Agent 产品化；微观规划能力通过对齐 Pi 循环即可成立。

---

## 8. 验证与测试锚点

| 验证方式 | 位置 |
|----------|------|
| 单 Turn 多工具 | `test_agent_loop_multiple_tool_calls_in_one_turn` |
| Turn 生命周期顺序 | `validate_pi_turn_lifecycle_order`（`uncode-core/src/event.rs`） |
| Thinking 事件 | TUI `chat.rs` 中 thinking 累积测试 |
| 手测 | CLI/TUI 发起「先列出目录再读 README」类任务，观察多 Turn 工具链 |

---

## 9. TUI 呈现与用户体验

机制层已具备微观规划，**终端用户是否「看得懂、跟得上」** 取决于 TUI。当前评价结论（2026-05）：

| 维度 | 约评 | 说明 |
|------|------|------|
| 单步可追溯 | 8/10 | Thinking / 正文 / 工具卡片时间线完整 |
| 多 Turn 链可感知 | 4/10 | 无 Turn 分隔；`TurnStart` 未进聊天区 |
| 进行中状态 | 7/10 | #271：`agent_busy` 对齐 `SessionEnd`；页脚 `turn:N` |
| 中途纠偏 | 6/10 | 排队可见；TUI → Agent `steering` 未贯通 |

**详案**（短板、对照表、P0–P4 改进建议）：[`UNCODE_TUI_MICRO_PLANNING_UX.md`](UNCODE_TUI_MICRO_PLANNING_UX.md)。

---

## 10. 演进说明（不扩大「微观规划」定义）

以下改进**增强**微观规划体验或**宏观**工作流，**不改变**「微观规划 = ReAct Turn 内决策」的定义：

| 方向 | 文档 |
|------|------|
| 扩展宿主、`set_active_tools`、plan-mode 参考扩展 | [`EXTENSION_COMPOSABLE_HARNESS_DESIGN.md`](../technologies/EXTENSION_COMPOSABLE_HARNESS_DESIGN.md) |
| 循环与 Turn 细节 | [`UNCODE_LOOP_ENGINE.md`](UNCODE_LOOP_ENGINE.md) |
| TUI 呈现与 `agent_busy` | [`UNCODE_TUI_MICRO_PLANNING_UX.md`](UNCODE_TUI_MICRO_PLANNING_UX.md) |
| 事件与 `agent_event_tag` | [`UNCODE_EVENT_SYSTEM.md`](UNCODE_EVENT_SYSTEM.md) |

---

## 11. 相关文档

| 文档 | 说明 |
|------|------|
| [UNCODE_TUI_MICRO_PLANNING_UX.md](UNCODE_TUI_MICRO_PLANNING_UX.md) | TUI 层微观规划 UX 评价与改进建议 |
| [EXTENSION_COMPOSABLE_HARNESS_DESIGN.md](../technologies/EXTENSION_COMPOSABLE_HARNESS_DESIGN.md) | Turn vs Plan 模式 §2.3；扩展路线图 |
| [UNCODE_LOOP_ENGINE.md](UNCODE_LOOP_ENGINE.md) | 双环、Turn、Steering |
| [UNCODE_TUI_ARCHITECTURE.md](UNCODE_TUI_ARCHITECTURE.md) | TUI 模块与渲染 |
| [TUI_EVENT_FLOW.md](TUI_EVENT_FLOW.md) | Agent 事件 → UI 状态 |
| [UNCODE_TECHNOLOGIES_GLOSSARY.md](UNCODE_TECHNOLOGIES_GLOSSARY.md) | 微观规划 / Plan 模式 术语 |
| [UNCODE_PI_ALIGNMENT_AND_EVALUATION.md](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) | 与 Pi 哲学对齐 |

---

*与 `crates/uncode-agent/src/loop_engine.rs` 冲突时以源码为准。*
