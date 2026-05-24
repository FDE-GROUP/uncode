# Phase 4 工程设计：PhaseStateMachine 激活

> **对应重构方案**：`UNCODE_REFACTORING_PLAN.md` Phase 4（domain-first refinement 的子任务）
> **依赖**：Phase 3（PhaseStateMachine 构建 + EventRouter 集成）
> **预计工期**：1 天

---

## 一、目标

将 `PhaseStateMachine` 从"已构建但未接入"推进到"运行时在 AgentLoop 主循环中激活"：

1. PhaseStateMachine 嵌入 `AgentLoop`，在 6 个关键决策点触发状态转换
2. 每次转换通过 `PhaseTransition` 事件广播，TUI/Platform 可实时观测
3. 转换失败仅 log warning，绝不阻塞主执行路径

---

## 二、现状分析

### 2.1 PhaseStateMachine（`governance/state_machine.rs`）

六状态已完整实现：Init → Cognizing → Adjudicating → Executing → WaitingForUser → Terminated。

合法转换表：

| from | to |
|:---|:---|
| Init | Cognizing |
| Cognizing | Adjudicating, WaitingForUser, Terminated |
| Adjudicating | Executing, Cognizing, Terminated |
| Executing | Cognizing, WaitingForUser, Terminated |
| WaitingForUser | Cognizing, Terminated |
| Terminated | （终态） |

单元测试覆盖：完整生命周期、非法转换拒绝、终态不可退出、ReAct 多轮循环。

**当前状态**：仅在 `governance/` 模块内自测，`loop_engine.rs` 和 `harness.rs` 中零引用。

### 2.2 AgentLoop（`loop_engine.rs`）

已有 `event_router: std::sync::Mutex<EventRouter>` 字段和 `emit()` 方法中的 sync dispatch。决策层集成点已标注（`#339, #385, #387` 注释）。

### 2.3 AgentEvent（`event.rs`）

36 个变体，`#[non_exhaustive]`，新增变体不破坏下游 match。

---

## 三、改动清单

### 3.1 新增 PhaseTransition 事件

**文件**：`crates/uncode-core/src/event.rs`

新增数据结构和事件变体：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTransitionEventData {
    pub from: String,
    pub to: String,
    pub trigger: String,
    pub turn: u64,
}

// AgentEvent 新增变体
PhaseTransition {
    #[serde(flatten)]
    data: Box<PhaseTransitionEventData>,
},
```

使用 `String` 而非 `AgentPhase` 枚举——避免 uncode-core ↔ uncode-agent 跨 crate 依赖。

更新三处：

| 函数 | 改动 |
|:---|:---|
| `agent_event_tag()` | `"phase_transition"` |
| `detail_level()` | `Standard` |
| `turn_lifecycle_rank()` | `Some(2)`（与 `content_delta` / `tool_call_start` 同级） |

### 3.2 AgentLoop 集成

**文件**：`crates/uncode-agent/src/loop_engine.rs`

#### 3.2.1 结构体新增字段

```rust
pub struct AgentLoop {
    // ... 现有字段 ...
    event_router: std::sync::Mutex<EventRouter>,
    phase_sm: std::sync::Mutex<PhaseStateMachine>,  // 新增
}
```

位于 `event_router` 之后，构造函数中初始化为 `PhaseStateMachine::new()`。

#### 3.2.2 try_transition_phase helper

```rust
fn try_transition_phase(&self, to: AgentPhase, trigger: &str, turn: u64) {
    let mut sm = self.phase_sm.lock().unwrap();
    match sm.transition(to, trigger) {
        Ok(()) => {
            let last = sm.history().last().unwrap();
            drop(sm); // release lock before emit
            self.emit(AgentEvent::PhaseTransition { ... });
        }
        Err(e) => {
            warn!("phase transition ignored: {e}");
        }
    }
}
```

所有转换调用只经过此方法——single choke point 确保"永不阻塞"。

#### 3.2.3 六个转换插入点

| # | 转换 | loop_engine.rs 位置 | trigger | 条件 |
|:---|:---|:---|:---|:---|
| 1 | Init → Cognizing | `TurnStart` emit 之后 | `"turn_start"` | 先 reset 再 transition |
| 2 | Cognizing → Adjudicating | 决策层防火墙注释之前 | `"tool_calls_received"` | 仅当 `!executions.is_empty()` |
| 3 | Adjudicating → Executing | `denied_tool_names` 构建之后、工具执行之前 | `"adjudication_approved"` | 仅当有 approved 工具 |
| 4 | Executing → Cognizing | 工具结果写入 messages 之后 | `"tool_execution_complete"` | 仅当 `has_more_tool_calls` |
| 5 | → WaitingForUser | inner turn loop 退出之后 | `"inner_loop_complete"` | 无条件，非法转换由状态机拒绝 |
| 6 | → Terminated | `emit_session_end` 内 | `"session_end"` | 无条件 |

**Point 1 细节**：每次 turn 开始先 reset（`*sm = PhaseStateMachine::new()`），再 transition 到 Cognizing。

**Point 5 细节**：无条件调用。如果状态已经是 Terminated，`ALLOWED_TRANSITIONS` 表自动拒绝（warn log）。

**Point 6 细节**：使用 `emit_session_end` 的 `total_turns` 参数作为 turn 值。

---

## 四、与 AgentHarnessPhase 的关系

PhaseStateMachine **不替代** `AgentHarnessPhase`（5 态）。两者共存：

| 维度 | AgentHarnessPhase | AgentPhase |
|:---|:---|:---|
| 粒度 | Session 级 | Turn 级 |
| 职责 | 编排（Compaction/BranchSummary/Retry） | 可观测性（认知→裁决→执行循环） |
| 拥有者 | AgentHarness | AgentLoop |
| 消费者 | PhaseGuardPolicy | TUI/Platform 事件流 |

映射关系：

| AgentHarnessPhase | AgentPhase 子状态 |
|:---|:---|
| Idle | WaitingForUser |
| Turn | Cognizing → Adjudicating → Executing（循环） |
| Compaction | Executing |
| BranchSummary | Executing |
| Retry | Cognizing |

`PhaseGuardPolicy` 仍基于 `AgentHarnessPhase` 判断，不受本次改动影响。

---

## 五、测试计划

### 5.1 单元测试

| 测试 | 验证点 |
|:---|:---|
| PhaseTransition 事件序列（text-only turn） | Init→Cognizing, Cognizing→WaitingForUser 事件存在 |
| PhaseTransition 事件序列（tool-call ReAct） | 完整 6 态循环 |
| 每 turn 重置 | 两 turn mock，turn 2 的 PhaseTransition 以 Init 为起点 |
| 取消路径 | Terminated 事件存在，不 panic |

### 5.2 回归测试

现有 1,051 个测试全部通过——新增字段有构造函数初始化，不破坏现有代码路径。

---

## 六、文件变更总览

| 文件 | 改动类型 | 说明 |
|:---|:---|:---|
| `uncode-core/src/event.rs` | 修改 | PhaseTransitionEventData + AgentEvent::PhaseTransition + tag/detail/rank |
| `uncode-agent/src/loop_engine.rs` | 修改 | phase_sm 字段 + try_transition_phase + 6 个插入点 |

**零改动文件**：`harness.rs`、`governance/state_machine.rs`、`governance/mod.rs`。

---

## 七、风险与缓解

| 风险 | 缓解 |
|:---|:---|
| 转换失败阻塞主路径 | `try_transition_phase` 捕获 Err 仅 warn |
| Mutex 跨 await | lock 仅在 helper 内短暂持有，drop 后才 emit |
| 事件序列破坏 turn_lifecycle_rank | `turn_lifecycle_rank` 赋予固定秩，不影响排序 |
| 向后兼容 | `#[non_exhaustive]` + Standard 级别 = 下游无感 |
