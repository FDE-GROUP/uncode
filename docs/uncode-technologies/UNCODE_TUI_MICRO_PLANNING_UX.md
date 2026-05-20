# TUI 层微观规划（Micro-planning）用户体验评价

> 从 **uncode-tui 渲染与状态机** 角度评价：用户能否感知 Agent 在单 Turn / 多 Turn 上的「想一步 → 做一步」决策链。  
> 机制层定义见 [`UNCODE_MICRO_PLANNING.md`](UNCODE_MICRO_PLANNING.md)；事件流细节见 [`TUI_EVENT_FLOW.md`](TUI_EVENT_FLOW.md)、[`UNCODE_TUI_ARCHITECTURE.md`](UNCODE_TUI_ARCHITECTURE.md)。

| 项 | 说明 |
|----|------|
| **文档类型** | UX 评价 / 实现层差距分析 |
| **路径** | `docs/uncode-technologies/UNCODE_TUI_MICRO_PLANNING_UX.md` |
| **状态** | 与 `crates/uncode-tui` 同步（2026-05） |
| **评价范围** | 仅 TUI 呈现与 `agent_busy` 语义；不含 Plan 模式扩展 |

---

## 1. 评价目标

**微观规划**在引擎层指：每个 Turn 内 LLM 选择工具并组织回复，多 Turn 间根据工具结果继续推理（ReAct）。  
本文回答：**终端用户通过 TUI 能否看清这一过程？** 与 **缺口主要在何处？**

不评价 Plan 模式（只读相、`/plan`、结构化 `Plan:`）——见 [`EXTENSION_COMPOSABLE_HARNESS_DESIGN.md`](../technologies/EXTENSION_COMPOSABLE_HARNESS_DESIGN.md)。

---

## 2. 总评

| 维度 | 评分（约） | 说明 |
|------|------------|------|
| 单步可追溯性 | **8/10** | Thinking / 正文 / 工具卡片按时间堆叠，信息完整 |
| 多 Turn 链可感知性 | **4/10** | 无 Turn 分隔；`TurnStart` 未进入聊天区 |
| 「仍在规划/执行中」状态 | **5/10** | 有状态行，但 `agent_busy` 与 `TurnEnd` 绑定存在缺口 |
| 纠偏与排队 | **8/10** | Enter 同 run `steer`；`/later` 排队 follow-up |
| 信息密度控制 | **7/10** | 可折叠 thinking/工具；`read` 不自动展开 |

**一句话**：Agent 已在做多 Turn 微观规划，TUI 将其呈现为 **连续 scrollback + 工具卡片**——适合跟读审计，不适合一眼判断「第几轮决策、整条链是否尚未结束」。

---

## 3. 事件 → 渲染映射（现状）

### 3.1 ChatState：单 Turn 内决策可见

`ChatState::handle_event`（`crates/uncode-tui/src/chat.rs`）将 Agent 事件转为消息块：

| AgentEvent | UI 行为 | 用户感知 |
|------------|---------|----------|
| `ContentDelta(Thinking)` | `append_thinking_text`，`active` spinner | 「正在想」 |
| `ContentDelta(Text)` | `deactivate_thinking` + 追加 `Assistant` | 「正在说」 |
| `ToolCallStart` | `finalize_assistant` + `ToolCall` / `BashExecution` | 「决定调用某工具」 |
| `ToolCallProgress` | 更新参数/stdout | 执行中细节 |
| `ToolCallEnd` | 状态、耗时、结果；非 `read` 常自动展开 | 「做完了，结果在这」 |
| `TurnStart` (turn≥2) | `TurnDivider` | 多 Turn 链可见分界 |
| `TurnEnd` / `SessionEnd` | `deactivate_thinking`；`TurnEnd` 另同步 Markdown Todos | Thought 收尾；待办清单 |
| `TaskUpdate` / `PhaseSummary` | `TodoList` 卡片 | 结构化步骤 |

**微观规划单格**（想 → 说 → 调工具）在聊天区按时间展开，认知路径合理。

### 3.2 TuiEngine：全局活动态

`TuiEngine::handle_event`（`crates/uncode-tui/src/lib.rs`）维护：

- `agent_busy`：是否允许直接提交、是否显示状态行
- `activity`：`Thinking` / `Writing` / `RunningTool { name }` / `Idle`
- `FooterState`：token、ctx%、**单轮**耗时

状态行仅在 `agent_busy == true` 时渲染（`render_status`）。

### 3.3 工具专用渲染

`ToolRendererRegistry`（`tool_renderer.rs`）为 read/write/edit/grep/bash 等提供一行摘要 + 可展开语法高亮结果，支撑用户判断 Agent 是否「先读再改」——微观规划链上最关键的**可审计性**。

---

## 4. 做得好的地方

1. **流式三层块**：Thinking → Assistant（Markdown）→ Tool，与引擎事件顺序一致。  
2. **状态行**：当前在推理、写字还是跑哪个工具；配合页脚 token / ctx% 预警。  
3. **工具卡片**：Running/Success/Failed、耗时、`Ctrl+O` 控制输出展开；`read` 默认折叠减轻噪音。  
4. **Thinking / Thought**：流式阶段标题为 **Thinking**，结束后为 **Thought · {时长}**；折叠时显示一行预览；`Ctrl+T` 全局展开/折叠。
5. **Todos**：`TaskUpdate`、`PhaseSummary`（已完成/下一步）与助手 Markdown `- [ ]`/`- [x]` 清单会渲染 **Todos** 卡片；`Turn ≥ 2` 时显示 `── Turn N ──` 分隔。  
6. **卡片焦点**：`Ctrl+J/K`、Space 在长链后复查某次工具决策。  
7. **排队与压缩**：`QueuedMessage`、`CompactionSummary` 有独立样式，长任务上下文变化可感知。

---

## 5. 主要短板

### 5.1 Turn 边界仍偏弱

- ✅ `TurnStart`（turn≥2）插入 `── Turn N ──`；页脚 `turn:N`（见 #271）。  
- 同 Turn 多工具以 `ToolTurnGroup` 折叠组展示（P2）；`TurnEnd` 仍不汇总结构化「本轮小结」。

**后果**：多 Turn 链可分界，但单 Turn 内并行工具仍按时间线平铺。

### 5.2 `agent_busy` 与每个 `TurnEnd` 同步（多 Turn 链「假空闲」）

```rust
// lib.rs — TurnEnd 处理
AgentEvent::TurnEnd { usage, .. } => {
    self.agent_busy = false;
    self.activity = AgentActivity::Idle;
    self.footer.end_turn();
    ...
}
```

`AgentLoop` 在内层 ReAct **每一 Turn 结束**都会 `emit TurnEnd`（工具链未结束时亦然）。

**后果**：

| 现象 | 影响 |
|------|------|
| 链中间 `agent_busy = false` | 状态行消失，页脚像「已结束」 |
| 用户可再次 `submit_text` | 可能 `tokio::spawn` 第二个 `AgentLoop::run`，与仍在进行的链并发 |
| `flush_queue` 在 `TurnEnd` 触发 | 仅 drain **FollowUp**；若排队且旧 run 仍在，行为难预期 |

微观规划在引擎上是 **一次 `run` 内多 Turn**；TUI 却多次表现为 **idle**，这是当前最大的体验缺口。

### 5.3 Steering（已实现）

- **空闲**：`SubmitIntent::NewRun` → CLI 单例 `AgentLoop::run()`。  
- **忙碌**：默认 Enter → `SubmitIntent::Steer` → `AgentLoop::steer()`（同 run 内纠偏）。  
- **`/later <msg>`**：入 TUI `FollowUp` 队列，在 `SessionEnd` 后 `flush_queue` 再 `NewRun`。

Agent 层 `MessageQueued` 由 `steer()` 发射；TUI 不再为 steer 重复插入 `QueuedMessage` 行。

### 5.4 `PhaseSummary`（已实现）

每 Turn 在 `TurnEnd` 之后、若本 Turn 执行过工具，则 `emit PhaseSummary`：  
`completed` 为成功工具行（`tool(args)`），`issues` 为失败工具，`next_steps` 在内层工具链未结束时提示可能继续。  
TUI 经 `apply_phase_summary` 渲染为 TodoList / Summary 卡片。

### 5.5 缺少「本轮决策」分组

同 Turn 内多工具按事件顺序逐张卡片显示，无：

- 「本回合 · 3 个工具」折叠组  
- 步骤编号或与 plan-mode 类似的进度 widget  

长 scrollback 难以回答：**当前是第几步、整条链是否还在跑**。

### 5.6 Thinking 能力依赖模型

- 无 reasoning 流时，微观规划过程仅工具 + 短文字，「在想什么」不可见。  
- 工具开始时 `deactivate_thinking` 正确，但 UI 不强调「基于上文思考，将执行 read(…)」的衔接。

---

## 6. 用户问题对照表

| 用户问题 | 当前 TUI | 主要依据 |
|----------|----------|----------|
| 现在在干什么？ | 大多能（单 Turn 内） | 状态行 + 流式块 |
| 整条任务是否还没结束？ | **不可靠** | `TurnEnd` → `agent_busy=false` |
| 这是第几次「想→做」？ | **不能** | 无 Turn 标记 |
| 为什么选了这个工具？ | 部分能 | 工具摘要 + thinking（若有） |
| 能在同一次 run 里纠偏吗？ | **能** | busy 时 Enter → `steer` |
| 多步之间结构清晰吗？ | **弱** | 连续 scrollback |

---

## 7. 改进建议（仅 TUI / 状态机，按优先级）

| 优先级 | 建议 | 预期效果 |
|--------|------|----------|
| **P0** | ✅ 已实现（#271）：`finish_agent_run()` 仅在 run 结束；`TurnEnd` 仅 `update_usage`；`flush_queue` 不在 `TurnEnd` | 多 Turn 链期间状态行持续 |
| **P1** | ✅ 已实现（#271）：页脚 `turn:N` 来自 `TurnStart` | 多 Turn 链可数 |
| **P2** | ✅ 已实现：`ToolTurnGroup` 同 Turn 可折叠分组 | 单轮「一批决策」可扫读 |
| **P3** | ✅ 已实现：`SubmitIntent` + 单例 `AgentLoop` + `steer()` | 中途纠偏与 Pi 对齐 |
| **P4** | ✅ 已实现：每 Turn 工具批次后 `emit PhaseSummary` | 结构化步进小结 |

实现时应配 GitHub Issue；与 Plan 模式扩展（`set_active_tools` 等）正交。

---

## 8. 结论

| 问题 | 结论 |
|------|------|
| TUI 是否呈现微观规划？ | **是**，以 Thinking + Assistant + 工具 scrollback 形式 |
| 是否呈现 **多 Turn 微观规划结构**？ | **弱**；Turn 不可见、`agent_busy` 与引擎不同步 |
| 与机制文档关系 | 机制层已具备微观规划（见 [`UNCODE_MICRO_PLANNING.md`](UNCODE_MICRO_PLANNING.md)）；TUI 短板在 **状态机与 Turn 事件消费**，非 ReAct 缺失 |

uncode TUI 当前定位：**跟着日志审计 Agent 做了什么**；尚不足：**清楚看到第几步规划、整条链是否仍在进行**。

---

## 9. 相关文档与源码

| 文档 / 源码 | 说明 |
|-------------|------|
| [UNCODE_MICRO_PLANNING.md](UNCODE_MICRO_PLANNING.md) | 微观规划机制定义与判定 |
| [UNCODE_LOOP_ENGINE.md](UNCODE_LOOP_ENGINE.md) | Turn 与双环 |
| [TUI_EVENT_FLOW.md](TUI_EVENT_FLOW.md) | Agent 事件分发 |
| [UNCODE_TUI_ARCHITECTURE.md](UNCODE_TUI_ARCHITECTURE.md) | TUI 模块结构 |
| `crates/uncode-tui/src/chat.rs` | `ChatState::handle_event` |
| `crates/uncode-tui/src/lib.rs` | `agent_busy`、`render_status`、`flush_queue` |
| `crates/uncode-agent/src/loop_engine.rs` | `TurnStart` / `TurnEnd` 发射 |

---

*与源码冲突时以 `crates/uncode-tui` 为准。*
