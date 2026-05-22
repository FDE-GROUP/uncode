# AI Harness Engineering — 内部工程架构

> **定位说明**：本文描述 Agent **内部**的五模块工程架构——编排器怎么设计、工具怎么注册、
> 记忆怎么分层、怎么观测、怎么进化。这是"认知显化与决策驱动设计"范式治理层的工程实践子层。
> 范式总览见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §4。
>
> **注意**：Anthropic 在 Claude Code 博文（2026-05）中提出的 Harness 概念侧重**开发者界面层**
> ——CLAUDE.md、hooks、skills、plugins、MCP——而非 Agent 内部架构。二者的关系见 [附注](#附注与anthropic-claude-code的对照)。

基于学术论文和工程实践的综合分析，本文的五模块覆盖 Agent 内部 Harness 的完整工程范围。
学术论文《AI Harness Engineering》将其形式化为 11 项组件责任，工程落地中聚合为以下五大模块。

---

## 一、核心编排与状态管理

这是 Harness 的"大脑"，负责控制 Agent 的执行流程和状态转换。

**关键组件：**
- **Orchestrator（编排器）**：管理任务从创建到完成的全生命周期，独占决策权（何时执行、何时停止、何时重试）
- **状态机**：维护 Agent 的显式状态（idle/working/waiting/failed），确保状态转换的确定性
- **执行计划裁决**：Planner Agent 输出声明式计划，由 Harness 接管执行

**核心原则：** Agent 负责局部智能，Harness 负责全局控制。

> **uncode 对应**：`decision/adjudication.rs`（`Adjudicator` + `PhaseGuardPolicy` + `TurnLimitPolicy`）、
> `AgentHarnessPhase`（Idle/Turn/Compaction/BranchSummary/Retry）

---

## 二、工具治理与安全边界

工具是 Agent 能力的来源，也是风险的入口。Harness 必须将工具从"普通函数调用"提升为"受治理的生产资源"。

**关键组件：**
- **Tool Registry（工具注册表）**：每个工具登记九项元信息（名称、描述、JSON Schema、允许调用的 Agent 列表、超时、速率限制、风险等级、是否需要人工确认、审计策略）
- **Tool Executor**：统一执行入口，支持参数校验、超时控制、结果截断
- **权限与沙箱**：路径保护、敏感操作需人工确认

**设计要点：** 工具设计要守住几条线——输入有 schema、输出适合模型消费、大输出要截断、有副作用的工具返回 diff。

> **uncode 对应**：`decision/firewall.rs`（`SemanticFirewall` + `PermissionPolicyRule` + `PathSafetyRule` +
> `SchemaCoercionRule`）、`tools/registry.rs`（`ToolRegistry`）、`tool_permission.rs`（`PermissionPolicy`）

---

## 三、分层记忆与上下文管理

"记忆"在 AI Agent 中不是浪漫概念，而是工程问题。Harness 必须区分状态与记忆，并分层管理。

**分层架构**：

| 层级 | 生命周期 | 理想存储 | 用途 | uncode 实现 |
|:---|:---|:---|:---|:---|
| Working State | 当前步骤 | 内存 | 临时 scratchpad，turn 结束即丢 | `cognition/working_memory.rs` |
| Session State | 单次会话 | Redis + TTL | 多 Agent 共享信息 | `SessionStore`（SurrealDB） |
| Execution Log | 永久 | JSONL / 事件流 | 审计、回放、评估 | `SessionStore` + JSONL 导出 |
| Episodic Memory | 跨会话 | 向量数据库 | 踩过的坑、用户偏好 | `cognition/episode.rs`（按重要性评分，无向量检索） |
| Semantic Memory | 持久 | 知识库 | 领域概念、业务规则 | 当前空缺 |

**上下文压缩（Compaction）**：当上下文过长时，将早期内容交给模型总结为摘要，保留近期消息。
完整历史仍保存在事件存储中，模型看到的是压缩视图。

> **uncode 对应**：`cognition/episode.rs` + `cognition/working_memory.rs` + `cognition/memory.rs` +
> `session/store.rs` + `compaction.rs`

---

## 四、可观测性与评估体系

Harness 必须让 Agent 的执行过程可审计、可调试、可优化，而非黑盒。

**关键组件：**
- **事件溯源**：每次用户输入、模型输出、工具调用、文件修改都记录为不可变事件
- **执行轨迹捕获**：timestamps、I/O snapshots、failures、LLM 调用次数与成本
- **评估框架**：学术论文《AI Harness Engineering》提出 H0-H3 四级评估阶梯，从"仅输出 patch"到"输出可复现的验证报告"
- **失败归因**：将失败分类（tool_missing、retry_exhausted、verification_failed），并触发对应的 Harness Mutation

> **uncode 对应**：`uncode-core/event.rs`（`AgentEvent` 32 变体 + `EventDetailLevel`）、
> `decision/evaluator.rs`（H0-H3 评估阶梯 + `BasicEvaluator` + `VerifiedEvaluator`）、
> `decision/feedback.rs`（`FeedbackBridge` — 决策→认知上行通道）

---

## 五、自适应与进化机制

这是 Harness 区别于传统工作流引擎的核心特征——能够根据执行反馈自我修正。

**关键组件：**
- **Harness Evolution Loop**：Worker Agent 执行 → Evaluator 评估 → Evolution Engine 识别模式 → 建议 Mutation
- **Harness Mutation**：结构化的配置修改类型——TightenTurnLimit、BlockTool、ReduceConcurrency、IncreaseToolTimeout、EnablePolicy
- **Guardrails（硬闸）**：max_steps、cost_limit_usd、timeout 三道硬限制，超限时拒绝继续执行

**设计示例（Lasso 项目）：**
```
执行 trace → deriveMutationsFromTrace() → mutateHarness(spec, mutations)
→ 自动替换昂贵模型为更便宜的版本 → 新版本继续执行
```

> **uncode 对应**：`uncode-shared/src/evolution.rs`（`EvolutionEngine` — 模式识别 + `HarnessMutation` 建议）、
> `uncode-shared/src/guardrails.rs`（`GuardrailConfig` — 静态配置，Mutation 可动态修改）

---

## 总结：Harness 的完整视图

```text
                    ┌──────────────────────────┐
                    │    AI Harness 工程         │
                    │                          │
                    │  O: Orchestrator          │
                    │     编排与状态管理          │
                    │  T: Tool Registry         │
                    │     工具治理与安全          │
                    │  M: Memory                │
                    │     分层记忆系统           │
                    │  Obs: Observability       │
                    │     可观测性与评估          │
                    │  A: Adaptation            │
                    │     自适应进化             │
                    └──────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
         LLM 模型         工具/API         用户/审计
```

这五个部分共同构成了 Agent 的"操作系统"——它不是简单的 Prompt 模板拼盘，
而是让 Agent 从"即兴表演"变成"稳定每晚演两场，连演一年"的工程底座。

---

## 附注：与 Anthropic Claude Code 的对照

Anthropic 在 *How Claude Code works in large codebases*（2026-05-14）中提出 Harness
的另一层含义——**开发者界面层**。本文描述的是**内部工程架构层**。

| Anthropic 组件 | 定义 | 与本文的关系 |
|:---|:---|:---|
| **CLAUDE.md** | 每次会话自动加载的上下文文件 | 对应本文 §一、三——编排器读取的上下文来源 |
| **Hooks** | 事件触发脚本（停止钩子捕获 learnings） | 对应本文 §四——可观测性的事件系统 |
| **Skills** | 按需加载的专业工作流（渐进披露） | 对应本文 §一——编排器的任务路由 |
| **Plugins** | 可分发的能力包（skills+hooks+MCP） | 本文未覆盖——属于分发层 |
| **MCP servers** | 连接外部工具/数据源 | 对应本文 §二——工具注册的扩展协议 |
| **LSP** | 符号级代码导航 | 对应本文 §三——上下文构建的精度提升 |
| **Subagents** | 隔离实例（读写分离） | 对应本文 §一——编排器的并行调度 |

**关系**：Anthropic 的六组件是开发者**可配置**的控制面。本文的五模块是 Agent**内部**执行面。
一个完整的 Agent 系统两者都需要——内部架构决定"能做什么"，开发者界面决定"怎么用好它"。
