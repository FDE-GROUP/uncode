# UNCODE_GOVERNANCE_LAYER — 治理层设计文档

> **范式**：认知与决策驱动设计（`docs/ai-agent-archi/cognition-decision-driven-design.md` §4）
> **实现层定位**：治理模式在 uncode 中的映射

---

## 7 种治理范式在 uncode 中的映射

| # | 范式 | 覆盖度 | uncode 实现 |
|:---:|:---|:---:|:---|
| 1 | 事件驱动 | ★★★★★ | `AgentEvent` 30 变体 + `broadcast` channel + `EventRouter` 双通道 |
| 2 | 事件溯源 | ★★★★☆ | `SessionStore` (SurrealDB) + `SessionEntry` 树 + JSONL 导入/导出 |
| 3 | 约束设计 | ★★★★☆ | `GuardrailConfig` + `PermissionPolicy` + `SemanticFirewall` + `Adjudicator` |
| 4 | 有限状态机 | ★★★★☆ | `AgentHarnessPhase` (Idle/Turn/Compaction/BranchSummary/Retry) |
| 5 | 工作流编排 | ★★★☆☆ | 双层 ReAct 循环（隐式），无声明式 DAG |
| 6 | CQRS | ★★★☆☆ | 隐式分离：`SessionEntry` 树（读）+ 事件追加（写） |
| 7 | 多 Agent 协作 | ★☆☆☆☆ | 单 harness 双环架构（Pi 哲学有意不做） |

---

## 治理铁三角

```
                    事件驱动
                    (通信骨干)
                      ╱  ╲
                     ╱    ╲
                    ╱      ╲
              事件溯源      约束设计
             (持久化)      (安全边界)
```

- **没有事件驱动** → 系统是盲的（不知道发生了什么）
- **没有事件溯源** → 系统是失忆的（不知道为什么这么做）
- **没有约束设计** → 系统是危险的（什么都能做）

---

## GuardrailConfig — 声明式护栏

类型定义：`uncode-shared/src/guardrails.rs`

| 配置段 | 职责 | 关键字段 |
|:---|:---|:---|
| `decision` | 决策层参数 | `turn_limit: 50`, `max_concurrent_tools: 8` |
| `firewall` | 防火墙配置 | `path_safety.mode: cwd_only`, `tool_whitelist` |
| `adjudication` | 裁决策略 | `policies[]` 含 `no_destructive_commands`（默认启用） |
| `audit` | 审计策略 | `event_levels` (critical/standard/verbose), `retention` |

加载方式：`.uncode/guardrails.yaml` → `GuardrailConfig::default()`（回退）

---

## EventDetailLevel — 事件分级

类型定义：`uncode-core/src/event.rs`

| 级别 | 事件 | 导出策略 |
|:---|:---|:---|
| **Critical** | TurnStart/End, ToolCallEnd, DecisionMade, Error, SessionStart/End, CompactionComplete | 永久保留 |
| **Standard** | ContentDelta, ToolCallStart, CompactionStart, ModelChanged 等 | 90 天 |
| **Verbose** | ToolCallProgress, ToolCallAwaitingApproval | 7 天 |

```rust
let level = event.detail_level();
// 导出时过滤: events.filter(|e| e.detail_level() <= min_level)
```

---

## AgentStep — 训练数据模型

类型定义：`uncode-core/src/agent_step.rs`

```rust
AgentStep {
    step_id, turn_id,
    state_before: AgentStateSnapshot { phase, turn_number, active_tools, context_size_tokens },
    action: ExecutedAction { tool_name, arguments_summary, duration_ms },
    observation: ActionObservation { success, output_summary, files_changed, duration_ms, terminate },
    feedback?: Feedback::HumanApproval | TestPassed | TestFailed | AutoRevert,
    timestamp,
}
```

事件流 = 在线系统 + 离线训练数据的统一接口。
