# 可组合扩展与 Plan 模式：设计理念与技术方案

> 以 [Pi](https://github.com/earendil-works/pi) 源码（含 `packages/coding-agent/examples/extensions/plan-mode/`）为**事实参照**，说明「小内核 + 扩展自行拼装工作流（如 Plan 模式）」的设计哲学，并给出 uncode 的**现状评估**与**演进技术方案**。  
> 与 [`UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) §2、§5 互补：该文评价对齐度，本文聚焦**扩展能力面**与**落地路径**。

| 项 | 说明 |
|----|------|
| **文档类型** | 设计说明 / 技术方案 |
| **路径** | `docs/technologies/EXTENSION_COMPOSABLE_HARNESS_DESIGN.md` |
| **状态** | 草案（与 `crates/` 同步至 2026-05） |
| **关联 Issue** | 实现阶段应单独建 Issue（本文不替代 Issue 跟踪） |

---

## 1. 问题陈述

用户与产品层常希望具备 **Plan 模式**：规划阶段只读探索，确认后再开放 `edit`/`write` 等写操作。Pi 明确将此类能力**排除在核心之外**，由 TypeScript Extension 在运行时拼装；uncode 在架构哲学上同样采用「**不做内置 Plan 模式**」，但需回答：

1. uncode 是否已具备 Pi 扩展所需的**内核钩子**（如 `setActiveTools`、`registerCommand`、`on("tool_call")`）？
2. 若没有，应以何种**分层 API** 在 Rust 侧复现「扩展可组合」而不破坏现有 Harness 分层？
3. Plan 模式应作为**一等扩展样例**还是**短期内置模块**？
4. **每个 Turn 是否应内建 Plan 能力？**（见 §2.3：**否** — Turn 与 Plan 模式粒度不同，不可混为一谈。）

本文基于仓库源码给出结论与方案，避免与 Pi 文档或记忆混谈。

---

## 2. 设计哲学

### 2.1 小内核、外置工作流（对齐 Pi）

Pi `coding-agent` README 与 `docs/usage.md` 声明：核心**故意不包含** MCP、子 Agent、权限弹窗、**plan mode**、内置 todo、后台 bash 等；这些通过 **Extension**、**Pi Package**、Skill、外部工具（tmux/容器）组合实现。

uncode 在 [`UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) §2 采用相同立场：

| 条款 | 含义 |
|------|------|
| 内核 | 双环 `AgentLoop`、会话树、事件广播、工具执行、LLM 协议层 |
| 外置 | 规划/实现分轨、斜杠命令、权限 UI、子 Agent、MCP 等均非默认标配 |
| 扩展 | 能力应通过**稳定宿主 API** 注入，而非 fork `uncode-agent` |

**设计原则（冻结）**：

1. **Harness 不内置 Plan 模式产品逻辑**（无 `PlanMode` 枚举进主循环）。
2. **扩展通过声明式钩子改变行为**（工具可见性、命令、上下文、工具拦截），而非复制 `loop_engine.rs`。
3. **观察与控制分离**：UI/Platform 订阅 `AgentEvent`（观察）；扩展通过 Harness Hook 返回 `Block` / `PatchMessages` 等（控制）。
4. **L2 API 保持 Rust 惯用命名**（`set_active_tools`、`before_tool_call`），Pi 名仅出现在 `/// **Pi:**` 与机制对照表。

### 2.2 与 OpenCode 的差异（边界）

OpenCode 等产品将 build/plan 等模式**产品化**为 Agent 变体或配置。uncode 与 Pi 同轨：**不**在核心复制 OpenCode 双模式，若需要可由扩展或 Platform 配置实现。参见 [`OPENCODE_VS_PI.md`](OPENCODE_VS_PI.md)。

### 2.3 Turn 与 Plan 模式的粒度澄清（设计冻结）

本节回答常见误解：**「每一个 Turn 是否都应该具有 Plan 能力？」** —— 在 uncode / Pi 架构下，答案为 **否**。Plan 模式是**跨多个 Turn 的会话级工作流策略**，不是 Turn 类型上的内建子能力。

#### 2.3.1 Turn 是什么（机制层）

在 [`UNCODE_LOOP_ENGINE.md`](../uncode-technologies/UNCODE_LOOP_ENGINE.md) 与 Pi `agentLoop` 中，**一个 Turn** 指：

```text
TurnStart →（可选 pending / steering 消息）→ 一次 LLM 调用 → 工具调用批次 → TurnEnd → drain steering
```

Turn 是 **ReAct 内层循环的一格**：模型在该格内决定「回复什么、调用哪些工具」；工具执行完毕后，由 `has_more_tool_calls` 或 `pending_messages` 决定是否进入下一格。Turn 由 `TurnStart` / `TurnEnd` 事件标定，与 `validate_pi_turn_lifecycle_order` 所校验的生命周期一致。

**Turn 不负责**：会话是否处于「只读规划相」、是否在等待用户点击「执行计划」、是否在全局隐藏 `write` 工具 —— 这些属于 Harness / 扩展层策略。

#### 2.3.2 Plan 模式是什么（产品 / 扩展层）

**Plan 模式**（以 Pi `examples/extensions/plan-mode/` 为事实参照）指在**一次用户目标**下，跨**多个 Turn** 施加的约束与交互，典型包括：

| 维度 | 单 Turn 内的 LLM 推理 | Plan 模式（扩展拼装） |
|------|------------------------|------------------------|
| 时间范围 | 当前这一轮 | 多轮，直至用户选择「执行 / 留在规划 / 细化」 |
| 工具策略 | 默认：注册表中的可见工具全集 | **规划相**：全局 `setActiveTools` 只读子集 + `tool_call` 拦截 bash；**执行相**：恢复含 `edit`/`write` |
| 输出形态 | 任意助手回复 | 鼓励结构化 `Plan:` + 编号步骤、`[DONE:n]` 进度 |
| 持久状态 | 无专门「模式位」 | 扩展模块内 `planModeEnabled`、`executionMode` + 会话 `Custom` 条目 |
| 人机闸门 | 每 Turn 正常对话 | `agent_end` 后 UI 选择是否进入执行相 |

Pi **没有** `PlanMode` 枚举进入 `agentLoop`；uncode 同样 **不在** `AgentLoop` 增加 `enum RunMode { Normal, Plan, … }`（见 §2.1 原则 1、§5.3）。

#### 2.3.3 两种「规划」用语（避免术语混用）

| 用语 | 层级 | 含义 | 是否每个 Turn 都有 |
|------|------|------|-------------------|
| **微观规划**（micro-planning） | Turn 内 | 模型在单轮内选择工具、组织回复；ReAct 的「想一步做一步」 | **是**（凡有 LLM 调用的 Turn 均如此） |
| **Plan 模式**（plan mode workflow） | 会话 / 扩展 | 只读探索 → 结构化计划 → 用户确认 → 全工具执行 | **否**；仅当用户或扩展显式开启 |

文档与评审中应写清所指：讨论 **Turn 计数、事件顺序、steering 注入时机** 时用「Turn」；讨论 **只读阶段、bash 白名单、`/plan` 命令** 时用「Plan 模式 / 规划相」。

#### 2.3.4 为何不能把 Plan 内建进每一个 Turn

1. **语义冲突**：若每个 Turn 都强制只读 + 必须输出 `Plan:`，则简单问答、单行修复、steering 纠偏都会被错误流程绑架。
2. **分层破坏**：Turn 是循环引擎的**机制单元**；Plan 是**策略叠加**在 `run_inner` 与扩展宿主 API 之上。合并会导致 `loop_engine.rs` 膨胀且难以与 Pi 机制对齐。
3. **能力 ≠ 模式**：模型可在任意 Turn「先分析再动手」，那是 prompt / thinking；Plan 模式额外需要**可验证约束**（工具不可见、会话状态、阶段切换 UI），属于工程与产品，非 Turn 定义的一部分。

#### 2.3.5 推荐分层（与 §5.1 一致）

```text
┌────────────────────────────────────────────────────────────┐
│  扩展 / 会话策略：是否处于规划相、active_tools、Custom 状态   │  ← Plan 模式在此
├────────────────────────────────────────────────────────────┤
│  AgentLoop：外层 follow-up + 内层 Turn（LLM + 工具 + steering）│  ← Turn 在此
├────────────────────────────────────────────────────────────┤
│  单次工具调用：ToolHooks / 沙箱 / bash 白名单                  │
└────────────────────────────────────────────────────────────┘
```

**可接受、且与 Pi 同构的做法**（均不改变 Turn 定义）：

- **规划相连续 N 个 Turn**：扩展在规划相内每 Turn 注入 `[PLAN MODE ACTIVE]` 上下文并限制工具集；用户确认后切换执行相，后续 M 个 Turn 使用全量工具。
- **Turn 0 偏重探索**：产品约定上的习惯，仍是「会话策略」，不是 `Turn` 枚举变体。
- **单 Turn 内 planner→executor 双子 Agent**：子 Agent / 外部进程能力，Pi / uncode **不内置**；由扩展或 Platform 实现。

**不推荐**：

- 在 `Turn` 或 `SessionEntry` 上增加 `kind: PlanTurn | ExecTurn` 作为核心类型（等同于内置 Plan 模式，违反 §2.1）。
- 将「每个复杂任务自动先规划」理解为「每个 Turn 自带 Plan」；应实现为**显式两阶段会话**（扩展或 preset），而非改 Turn 语义。

#### 2.3.6 冻结结论（评审与 Issue 引用）

| 问题 | 结论 |
|------|------|
| 每个 Turn 是否应内建 Plan 能力？ | **否** |
| 每个 Turn 是否都会做局部决策？ | **是**（微观规划 / ReAct） |
| Plan 模式应落在哪一层？ | **Extension Host / 扩展**（`set_active_tools`、上下文 hook、Custom 条目、TUI 命令） |
| 若需「先规划再执行」？ | **会话级两阶段**：规划相若干 Turn → 用户闸门 → 执行相若干 Turn |

---

## 3. Pi 事实：Plan 模式如何由扩展拼装

Pi **未**在 `packages/coding-agent/src/` 核心实现 Plan 模式。参考实现位于：

`packages/coding-agent/examples/extensions/plan-mode/`

### 3.1 扩展使用的宿主 API（事实表）

| Pi Extension API | plan-mode 中的用途 | 源码锚点 |
|------------------|-------------------|----------|
| `pi.setActiveTools(names)` | 规划阶段仅 `read/bash/grep/find/ls/questionnaire`；执行阶段恢复含 `edit`/`write` | `index.ts` `PLAN_MODE_TOOLS` / `NORMAL_MODE_TOOLS` |
| `pi.registerCommand("plan", …)` | `/plan` 切换模式 | `registerCommand` |
| `pi.registerFlag("plan", …)` | `--plan` 启动即规划 | `registerFlag` |
| `pi.registerShortcut(…)` | `Ctrl+Alt+P` | `registerShortcut` |
| `pi.on("tool_call", …)` | 规划阶段拦截非白名单 `bash` | `return { block: true, reason }` |
| `pi.on("before_agent_start", …)` | 注入 `[PLAN MODE ACTIVE]` 系统上下文 | `customType: plan-mode-context` |
| `pi.on("context", …)` | 退出规划时剥离陈旧 plan 上下文 | `messages` 过滤 |
| `pi.on("agent_end", …)` | 解析 `Plan:` 步骤、UI 选择执行/细化 | `ctx.ui.select` |
| `pi.appendEntry("plan-mode", …)` | 持久化 enabled/todos/executing | 会话树 custom entry |
| `pi.setStatus` / `pi.setWidget` | 页脚 `⏸ plan`、`📋 2/5`、待办 widget | TUI 扩展面 |

辅助逻辑在 `utils.ts`：`isSafeCommand`（bash 白名单）、`extractTodoItems`、`markCompletedSteps`（`[DONE:n]`）。

### 3.2 Pi 核心为扩展提供什么

- **ExtensionAPI**：注册命令/快捷键/Flag、订阅事件、改工具集、写会话条目、调 TUI。
- **单进程 TS 运行时**：扩展与 Harness 同址，无 WASM 边界。
- **不设 Plan 枚举**：模式状态完全在扩展模块内（`planModeEnabled`、`executionMode`）。

结论：**Pi 已实现的是「可组合扩展平台」+「plan-mode 示例」**；**没有**名为 `PlanMode` 的核心模块。

---

## 4. uncode 现状：能力矩阵与接线状态

以下以 `~/EA/uncodenow` 源码为准（`uncode-agent` **不依赖** `uncode-extensions`）。

### 4.1 对照表

| 能力（Pi 扩展语义） | uncode 对应物 | 实现位置 | 主路径已接线 |
|---------------------|---------------|----------|--------------|
| 运行时缩小 LLM 工具集 | `ToolRegistry::set_active_tools` / `configure_active_tools`；`definitions()` 过滤；未激活工具执行返回 `tool not active` | `uncode-agent/src/tools/registry.rs`、`builtin.rs`、`loop_engine.rs`；CLI `--tools` / `--no-tools` / `--no-builtin-tools` | ✅（P0） |
| 执行前参数校验 | `prepare_and_validate`：prepare → coerce → validate（对齐 Pi 顺序） | `registry.rs`、`loop_engine.rs` | ✅ |
| 并行批次 | prepare/before **串行**，execute **并发**（无 sequential 工具时） | `loop_engine.rs` | ✅ |
| bash 批次串行 | `BashTool` → `ExecutionMode::Sequential`；任一批次含 sequential 则整批串行 | `bash.rs`、`loop_engine.rs` | ✅ |
| 工具调用前拦截 | `ToolHooks::before_tool_call` → `Option<String>` 阻止；TUI 经 `PermissionGate` + `PermissionToolHooks` 阻塞至用户确认 | `uncode-core/src/tool.rs`、`permission_gate.rs`、`uncode-tui` | ✅（TUI 已接线；`ChainedToolHooks` 可叠加多 hook） |
| 工具结果改写 | `ToolHooks::after_tool_call` | 同上 | ✅ |
| 事件级 Hook（Block/Patch） | `EventRouter::dispatch_hooks` → `HookResult` | `uncode-core/src/event.rs` | ❌（仅单测） |
| 生命周期扩展总线 | `HookRegistry` + `Extension::on_hook` | `uncode-extensions/src/hooks.rs` | ❌ |
| WASM / 目录加载扩展 | `ExtensionLoader::load_from_dir` | `uncode-extensions/src/loader.rs` | ❌（占位，返回 0） |
| 发 LLM 前改上下文 | `transform_context` 回调 | `AgentLoop::set_transform_context` | ✅（编程式，非扩展注册） |
| 斜杠命令 | `SlashCommands::register` | `uncode-tui/src/slash.rs` | ⚠️ 仅 TUI 内置表，扩展不可注册 |
| 会话 custom 条目 | `SessionEntry::Custom` | `uncode-core/src/session.rs` | ⚠️ 类型有，无扩展 `append` API |
| UI 状态/控件 | TUI 内部 | `uncode-tui` | ❌ 未暴露给扩展层 |

### 4.2 已接线且可用的机制

```text
用户输入 → AgentLoop::run_inner
         → build_context + 可选 transform_context
         → LLM（tools = tool_registry.definitions()，受 active_tools 过滤）
         → 工具批次（含 bash 则整批串行，否则 prepare/before 串行 + execute 并行）
              → prepare_and_validate → ToolHooks::before_tool_call（可 block）
              → ToolExecutor
              → ToolHooks::after_tool_call（可 patch / terminate）
         → AgentEvent 广播 → TUI / Platform
```

**可复刻 plan-mode 的子集**：仅用 `ToolHooks` 在 `bash` 上做白名单（类似 Pi 的 `on("tool_call")`），**无法**在不改内核的情况下隐藏 `edit`/`write`（LLM 仍可见全量工具定义）。

### 4.3 未接线但已存在的类型

- **`EventRouter`**：与 Pi `Harness.on` 类似，支持 `HookResult::Block`、`PatchMessages`、`PatchToolResult`、`CancelCompaction`；**`uncode-agent` 未调用**。
- **`HookRegistry`**：8 个 `LifecycleHook`，但 `on_hook` 仅 `Result<()>`，**不能**返回 block；且与 `AgentLoop` **无依赖关系**。
- **`uncode-extensions`**：在 workspace 图中有位置（见 [`UNCODE_OVERVIEW.md`](../uncode-technologies/UNCODE_OVERVIEW.md)），**运行时未加载**。

### 4.4 结论（现状）

| 维度 | 判定 |
|------|------|
| 哲学（小内核、外置 Plan） | ✅ 与 Pi 一致 |
| 平台（扩展可拼装 Plan） | ⚠️ **部分**；`set_active_tools` 已落地（P0）；仍缺扩展命令注册、多路 Hook 总线、扩展↔Agent 接线 |
| 短期变通 | Rust 模块 + `set_tool_hooks` + 改 `ToolRegistry` 或过滤 `definitions()`；或 TUI 硬编码 `/plan` |

---

## 5. 目标架构：可组合扩展宿主（Extension Host）

### 5.1 分层

```text
┌─────────────────────────────────────────────────────────┐
│  L3 交付面：uncode-cli / uncode-tui / uncode-platform    │
│  - 扩展注册斜杠命令、快捷键、状态栏、Widget（TUI trait）   │
└───────────────────────────┬─────────────────────────────┘
                            │ ExtensionHost（新，或扩展现有 runner）
┌───────────────────────────▼─────────────────────────────┐
│  L2 编排：uncode-agent（AgentHarness / AgentLoop）       │
│  - active_tool_names: Option<Vec<String>>                │
│  - extension_hooks: 合并 ToolHooks + EventRouter hooks   │
│  - transform_context 链（多扩展顺序执行）                 │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  L1 类型：uncode-core（ToolHooks、HookResult、AgentEvent）│
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  L0 扩展运行时：uncode-extensions                        │
│  - 阶段 A：内置 Extension（Rust trait，与 HookRegistry 接线）│
│  - 阶段 B：WASM 沙箱 + 能力白名单（HTTP/FS/子进程）       │
└─────────────────────────────────────────────────────────┘
```

**依赖方向**：`uncode-agent` 依赖 `uncode-extensions`（或抽象 `uncode-extension-api` 叶子 trait crate），`uncode-cli` 在启动时 `load_extensions()` 并注入 `AgentLoop` / `TuiEngine`。

### 5.2 核心 API（拟议，对齐 Pi 能力面）

以下为 **L2 宿主面** 建议公开能力（Rust 命名，非 Pi 原名）：

| API | 行为 | Pi 对照 |
|-----|------|---------|
| `set_active_tools(&[impl AsRef<str>])` | 仅将子集 `ToolDefinition` 传给 LLM；未列工具不可被模型调用 | `setActiveTools` |
| `register_tool_interceptor(ext)` | 链式 `before_tool_call` / `after_tool_call` | `on("tool_call")` / `on("tool_result")` |
| `register_context_hook(ext)` | `before_agent_start` 注入消息；`context` 过滤/补丁 | `before_agent_start` / `context` |
| `register_command`（TUI/CLI） | 动态 `/plan` 等 | `registerCommand` |
| `register_cli_flag` | `--plan` | `registerFlag` |
| `append_custom_entry(type, data)` | 会话树持久化扩展状态 | `appendEntry` |
| `extension_ui.set_status / set_widget` | TUI 页脚与侧栏（trait 抽象） | `setStatus` / `setWidget` |

**合并规则**：

- 多个扩展的 `before_tool_call`：**顺序执行**，首个返回 `Some(reason)` 即 block。
- `set_active_tools`：**最后一次**扩展调用生效，或显式 `ExtensionPriority`（后续细化）。
- `HookResult::Block` 与 `ToolHooks` **统一**为一个 `ToolInterceptor` trait，避免双轨。

### 5.3 Plan 模式作为「参考扩展」

实现路径（推荐顺序）：

1. **阶段 0（验证）**：在 `uncode-agent` 内增加 `active_tool_names: Option<Vec<String>>`，`definitions()` 过滤；用单元测试 + 手工 `ToolHooks` 验证「只读工具集 + bash 白名单」。
2. **阶段 1（Rust 扩展）**：`crates/uncode-extensions` 或 `extensions/plan-mode/`（Rust）实现与 Pi plan-mode 同构状态机；CLI `--extension plan-mode` 加载。
3. **阶段 2（TUI）**：`SlashCommands` 支持运行时 `register`；`TuiEngine` 暴露 `set_status` / `set_widget` 给 `ExtensionHost`。
4. **阶段 3（WASM）**：将 plan-mode 逻辑迁 WASM（可选），宿主仅保留能力白名单。

**不采用**：在 `AgentLoop` 增加 `enum RunMode { Normal, Plan, ExecutePlan }` 作为产品内置（违反 §2.1 原则 1）。

---

## 6. 分阶段实施计划

| 阶段 | 交付 | 验收标准 |
|------|------|----------|
| **P0** | `AgentLoop` 支持 `set_active_tools` + 文档更新 | 测试：仅 `read`/`grep` 时 LLM tools schema 不含 `write`；`write` 调用返回 tool not found 或前置 block |
| **P1** | `ExtensionHost` 接入 `ToolHooks` 链 + `HookRegistry::ToolCallBefore` 可 block | 两个测试扩展同时注册，block 顺序确定 |
| **P2** | `EventRouter::dispatch_hooks` 在 `turn_end` / `tool_call_end` 等点调用 | `HookResult::PatchMessages` 有集成测试 |
| **P3** | CLI/TUI 动态命令 + `append_custom_entry` API | `/plan` 由扩展注册，会话 resume 恢复状态 |
| **P4** | `plan-mode` 参考扩展（Rust）行为对齐 Pi 示例 | 手测：规划→选择执行→`[DONE:n]`→完成 |
| **P5** | WASM 加载器（可选） | `load_from_dir` 加载 `.wasm` 并注册 ≥1 hook |

每阶段独立 PR，引用 GitHub Issue；**先文档/ Issue，再编码**（与仓库「重要约定」一致，纯测试/修复除外）。

---

## 7. 与现有模块的关系

| 模块 | 调整要点 |
|------|----------|
| `uncode-core` | 保持 `ToolHooks` / `HookResult`；可增加 `ToolInterceptor` 别名或默认链式实现 |
| `uncode-agent` | `LoopEngine` 过滤 tools；构造时注入 `ExtensionHost` |
| `uncode-extensions` | `Extension::on_hook` 扩展为可返回 `HookAction`；实现 `load_from_dir` |
| `uncode-tui` | `SlashCommands` 与扩展宿主绑定；状态栏 API |
| `uncode-cli` | 启动参数 `--extension`、`--plan`（由扩展注册 flag） |
| 文档 | [`UNCODE_EVENT_SYSTEM.md`](../uncode-technologies/UNCODE_EVENT_SYSTEM.md) 增加「扩展宿主」节；[`UNCODE_PI_MECHANISM_MAP.md`](../uncode-technologies/UNCODE_PI_MECHANISM_MAP.md) 增 Pi Extension API 行 |

**不改动**：LLM 四层协议、会话 SurrealDB 主存、双环语义、Steering 三通道。

---

## 8. 风险与取舍

| 风险 | 缓解 |
|------|------|
| Rust 扩展生态弱于 Pi npm 包 | 阶段 1 用 Rust 动态库或编译期 feature；WASM 后置 |
| `set_active_tools` 与 Skill 注入工具冲突 | 文档约定：active set = 注册表 ∩ 扩展允许 ∩ 配置允许 |
| 多扩展 hook 顺序不确定 | 显式 `ExtensionManifest.priority` |
| TUI 与 Platform 行为分叉 | `ExtensionHost`  trait 在 core 或 agent 定义，TUI/Platform 各实现 UI 端口 |
| 安全（bash 白名单绕过） | Plan 扩展仅作参考；生产需容器/只读 FS 纵深防御 |

---

## 9. 总结

| 问题 | 答案 |
|------|------|
| Pi 是否内置 Plan 模式？ | **否**；`examples/extensions/plan-mode` 为扩展拼装 |
| 每个 Turn 是否应内建 Plan 能力？ | **否**；Turn = 单轮 LLM+工具；Plan = 跨 Turn 的扩展/会话策略（§2.3） |
| uncode 是否已具备微观规划？ | **是**；由 ReAct `AgentLoop` 提供，详见 [`UNCODE_MICRO_PLANNING.md`](../uncode-technologies/UNCODE_MICRO_PLANNING.md) |
| uncode 是否已有 Pi 式扩展平台？ | **否**；有 `ToolHooks`、`EventRouter`、`HookRegistry` 等**零件**，主路径**未接线** |
| uncode 哲学是否一致？ | **是**；不做内置 Plan，靠外置扩展 |
| 推荐技术路线？ | **P0 `set_active_tools` → P1 扩展宿主接线 → P4 plan-mode 参考扩展** |

---

## 10. 相关文档

| 文档 | 说明 |
|------|------|
| [UNCODE_LOOP_ENGINE.md](../uncode-technologies/UNCODE_LOOP_ENGINE.md) | Turn 定义与双环语义；§「Turn 与 Plan 模式」交叉引用 §2.3 |
| [UNCODE_MICRO_PLANNING.md](../uncode-technologies/UNCODE_MICRO_PLANNING.md) | 微观规划能力判定、机制层与 Plan 模式区分 |
| [UNCODE_PI_ALIGNMENT_AND_EVALUATION.md](UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) | Pi 哲学与 uncode 对齐评价 |
| [TERMINOLOGY_ALIGNMENT_STRATEGY.md](TERMINOLOGY_ALIGNMENT_STRATEGY.md) | 策略 C：L2 不改名、文档映射 |
| [UNCODE_EVENT_SYSTEM.md](../uncode-technologies/UNCODE_EVENT_SYSTEM.md) | AgentEvent 与 Hook |
| [UNCODE_PI_MECHANISM_MAP.md](../uncode-technologies/UNCODE_PI_MECHANISM_MAP.md) | 机制对照 |
| [UNCODE_OVERVIEW.md](../uncode-technologies/UNCODE_OVERVIEW.md) | Crate 分层 |
| Pi 源码 `packages/coding-agent/examples/extensions/plan-mode/` | Plan 扩展参考实现（外部仓库） |

---

*本文档随 `uncode-extensions` 与 `uncode-agent` 扩展宿主落地进度更新；与源码冲突时以 `crates/` 为准。*
