# UNCODE_DECISION_LAYER — 决策层设计文档

> **范式**：认知显化与决策驱动设计（`docs/ai-agent-archi/cognition-decision-driven-design.md`）
> **实现层定位**：uncode 的决策层实现——源文件 `crates/uncode-agent/src/decision/`

---

## 架构图

```
                        ┌──────────────┐
                        │  uncode-ai   │  ← 认知层基础设施
                        │  (4 LLM 协议) │
                        └──────┬───────┘
                               │ StreamEvent
                               ▼
┌──────────────────────────────────────────────────────────────┐
│                      uncode-agent                            │
│                                                              │
│  ┌─────────────────────┐    ┌─────────────────────────────┐ │
│  │  cognition/          │    │  decision/                   │ │
│  │  ├ context_builder   │    │  ┌───────────────────────┐  │ │
│  │  ├ prompt_manager    │ ←→ │  │ proposal (提案接收)    │  │ │
│  │  ├ uncertainty       │    │  │ firewall (语义防火墙)  │  │ │
│  │  └ memory            │    │  │ adjudication (裁决)    │  │ │
│  └─────────────────────┘    │  │ execution (执行派发)    │  │ │
│                              │  │ audit (审计)            │  │ │
│                              │  └───────────────────────┘  │ │
│                              │                             │ │
│  治理层：                    │  ┌──────────┬─────────────┐ │ │
│  ┌──────────┬──────────────┐ │  │ 事件驱动  │ 事件溯源    │ │ │
│  │ guardrails│ EventDetail  │ │  │ 约束设计  │ 状态机      │ │ │
│  │ (shared) │ Level (core) │ │  └──────────┴─────────────┘ │ │
│  └──────────┴──────────────┘ │                             │ │
│                              └─────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

---

## 四阶段决策管线

| 阶段 | 文件 | 核心类型 | 职责 |
|:---|:---|:---|:---|
| **提案接收** | `decision/proposal.rs` | `ActionProposal` | 从 LLM 流式输出中提取工具调用 |
| **语义防火墙** | `decision/firewall.rs` | `SemanticFirewall`, `ValidationRule` | Parsing → Validation → Normalization |
| **裁决** | `decision/adjudication.rs` | `Adjudicator`, `DecisionPolicy` | Phase/Turn/Cancellation/Concurrency 检查 |
| **执行派发** | `decision/execution.rs` | `ExecutionOrchestrator` | parallel/sequential/terminate 语义 |
| **审计** | `decision/audit.rs` | `Auditor`, `DecisionRecord` | 决策轨迹记录 + AgentStep 生成 |

---

## SemanticFirewall 三层管线

```
LLM 自然语言 / ToolCall
  → Parser (ParseStrategy trait)
    → ParsedAction（结构化提取）
  → Validator (ValidationRule trait) — 三个内置规则：
    ├ SchemaCoercionRule  — 包装 ToolRegistry::prepare_and_validate()
    ├ PathSafetyRule      — 复现 resolve_path() CWD sandbox 逻辑
    └ PermissionPolicyRule — 包装 tool_permission::PermissionPolicy
    → ValidatedAction（合法性确认）
  → Normalizer (NormalizeStrategy trait)
    → NormalizedAction（消歧义后最终形式）
```

---

## Adjudicator 策略链

| Policy | 包装的现有逻辑 | 裁决逻辑 |
|:---|:---|:---|
| `PhaseGuardPolicy` | `AgentHarnessPhase` 枚举 | Idle/Turn 通过；Compaction/BranchSummary/Retry 拒绝 |
| `TurnLimitPolicy` | `MAX_TURNS=50` | `turn_number >= max_turns` 拒绝 |
| `CancellationPolicy` | `CancellationToken` | `is_cancelled()` 时拒绝 |
| `ConcurrencyPolicy` | `active_run` AtomicBool CAS | 无活跃运行拒绝 |

---

## 审计与训练数据

- **`DecisionRecord`**：每次裁决的完整快照（proposal + verdict + approved_action）
- **`AgentStep`**（`uncode-core`）：RL trajectory 模型 `{ state, action, observation, feedback? }`
- **`DecisionMade`**（`AgentEvent` 变体）：决策审计事件

---

## 与 AgentHarness 的映射

当前 `AgentHarness` 内联了部分决策逻辑。映射关系：
- Phase 守卫 → `PhaseGuardPolicy`
- MAX_TURNS → `TurnLimitPolicy`
- CancellationToken → `CancellationPolicy`
- active_run CAS → `ConcurrencyPolicy`

参见 `crates/uncode-agent/src/harness.rs` 模块文档。
