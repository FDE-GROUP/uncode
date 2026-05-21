# 从 AI Agent 架构治理系列看 uncode 项目：架构评价

> 评价对象：uncodenow（本仓库），Rust 实现的 AI Coding Agent 系统
> 评价框架：系列文章《AI Agent 架构治理》三篇（DDD 深度版、博客版、7 种架构范式）
> 评价方法：将 uncode 的架构实现与系列核心论点做对照，识别已对齐、部分实现和缺口

---

## 一、总体判断

**uncodenow 是一个在架构设计上高度自觉的项目。** 它对 AI Agent 架构治理的核心问题——概率性与确定性的隔离、事件作为一等公民、聚合从"数据一致"到"决策合法"的升级——都有明确的工程回应，且实现深度远超概念讨论层面。在开源 Rust Agent 项目中，uncodenow 可能是 DDD 意识最强、分层最清晰的之一。

但与此同时，项目目前处于 **"内核已稳固、生态在起步"** 的过渡期，部分模式（扩展 WASM、Platform 后端）仍属增量，若干治理范式的覆盖还不完整。

---

## 二、DDD 适应性的逐项评估

### 2.1 限界上下文作为"概率-确定性"隔离墙 ★★★★★

**系列论点**：AI Agent 系统应严格分离概率性区域（LLM 调用）和确定性区域（状态机、工具调度），通过防腐层交互。

**uncodenow 实践**：

```
概率性区域：uncode-ai        — Api trait + 4 协议实现，LLM 流式响应
         ↕ 防腐层 (StreamEvent + transform_context)
确定性区域：uncode-agent     — LoopEngine 双层循环、AgentHarness 编排器、状态机
纯类型地带：uncode-core       — AgentEvent 枚举、SessionEntry 树、ToolExecutor trait
```

分拆得极为干净。`uncode-agent` 不直接依赖 `uncode-ai` 的具体实现——它通过 `uncode-core` 中定义的 trait 边界（`Api` trait、`ToolExecutor` trait）交互，实现了编译级隔离。

**结论**：**对齐度极高**。这是 uncode 架构最突出的优点——比 Pi 原版（TypeScript 包边界虽然也存在，但因 JS 动态性天然更弱）更"硬"。

### 2.2 防腐层升级：语义防火墙 ★★★★☆

**系列论点**：ACL 应升级为 Parsing → Validation → Normalization 三层语义防火墙，领域层永不接触自然语言。

**uncodenow 实践**：

| 系列要求 | uncode 实现 | 评价 |
|:---|:---|:---|
| **Parsing 层** | `uncode-ai` 将 LLM 流式输出解析为 `StreamEvent`（`ContentDelta::Text` / `ContentDelta::Thinking` / `ToolCall`），结构化为 `ContentBlock` | ✅ 已实现，类型安全 |
| **Validation 层** | 工具调用参数通过 JSON Schema（由 `#[tool]` 宏生成）验证 + `coerce` 自动类型转换；`prepare_and_validate` 管线 | ✅ 已实现 |
| **Normalization 层** | `normalize_path` + `resolve_path` 做路径消歧和安全范围限定；`transform_context` 回调在发送 LLM 前可修改消息 | ⚠️ 部分实现，路径标准化完整，但缺乏统一的输出到领域命令的规范化层 |

Normalization 层的不完整是这个评分的主要扣分项。当前 uncode 的 LLM 输出到确定性命令的转换散落在 `agent/loop_engine.rs` 中的 `handle_llm_stream` 和工具管线里，没有一个独立的、可组合的"规范化调度器"。这意味着如果未来要支持更多工具或更复杂的语义消歧（如依赖分析、引用解析），需要重构此处。

**结论**：**语义防火墙已存在**，但三层的职责边界不如理论中那样显式分开。Parsing 和 Validation 做得很好，Normalization 可以更清晰。

### 2.3 聚合从"一致性边界"到"决策边界" ★★★★☆

**系列论点**：聚合不再保证数据一致，而保证决策合法——LLM 负责"想"，聚合负责"裁决"。

**uncodenow 实践**：

`AgentHarness` 就是聚合根。它的核心职责不是管理事务，而是**裁决**：
- `Phase` 守卫（`Idle/Turn/Compaction/BranchSummary/Retry`）禁止并发运行 → 决策排他性
- `MAX_TURNS=50` → 决策容量硬限
- `CancellationToken` 在 5 个检查点中断 → 决策可终止性
- 工具调用前 `before_hook` 可返回 `HookResult::Block` → 决策否决权

```rust
// uncode 的聚合 = 决策授权单元
impl AgentHarness {
    // Phase 守卫保证"一次只有一个决策者在运行"
    pub async fn run(&self, ...) -> Result<()> {
        if !self.active_run.compare_exchange(false, true, ...) {
            return Err(HarnessError::Busy);  // 裁决：不允许并发
        }
        // ...
    }
}
```

**与理论对应的"护栏"（Guardrails）实现**：

| 护栏维度 | 实现 | 机制 |
|:---|:---|:---|
| 权限边界 | `normalize_path` 限制 CWD 内 | 文件系统护栏 |
| 动作边界 | `set_active_tools` 动态过滤工具 | 工具集护栏 |
| 容量边界 | `MAX_TURNS=50`, `compact_if_needed` | 资源护栏 |
| 时间边界 | `CancellationToken`, LLM 重试指数退避 | 超时护栏 |
| 质量边界 | `terminate` AND 语义, 工具返回验证 | 完成条件护栏 |

**结论**：**护栏系统已经相当完善**，编码为工程实践的深度很好。"决策授权单元"的概念在 uncode 中有实质性的实现，只是缺少一个显式的 DSL 或声明式策略层来配置这些护栏。目前的护栏参数散落在 `AppConfig`、常量定义和 hook 逻辑中，不便非程序员调整。

### 2.4 不确定性分层处理 ★★★★☆

**系列论点**：不确定性应分为生成/认知/执行三层，各自有不同的适配策略。

**uncodenow 实践**：

| 不确定性类型 | 系列建议 | uncode 实现 |
|:---|:---|:---|
| **生成不确定性** | 约束+验证（Schema、规则、类型） | `StreamOptions.temperature` 控制采样；JSON Schema 验证工具参数；`#[tool]` 宏保证编译时注册 |
| **认知不完全性** | 记忆与检索建模 | `build_context()` 从 SurrealDB 重建压缩后的会话历史；`compact_session()` 自动生成摘要注入上下文；workspace graph 注入文件结构 |
| **执行不确定性** | 补偿事务/事件溯源 | `CancellationToken` 中断链；指数退避重试（最大 3 次）；`PlanFailed`/`PlanRevised` 事件追加而非回滚；`terminate` 的 AND 语义 |

**但不足之处**：这三种不确定性在 uncode 的代码中并未被显式建模为独立的领域类型。`AgentEvent` 有 `Error { category }` 变体，但 category 是 `Network | Api | Cancelled | NonRetryable`——这是按错误来源分的，不是按不确定性性质分的。

**结论**：**策略上已覆盖**三种不确定性，但**领域建模上未显式表达**。如果要达到理论中 `AgentFailure` 的 `kind` 多态建模水平，需要重构 `AgentEvent::Error` 的 category 枚举，或者引入独立的 `ResolutionStrategy` 抽象。

### 2.5 事件作为一等公民 ★★★★★

**系列论点**：事件链不仅是通知机制，而是核心产品——可观测性、离线优化、指标计算的基础。

**uncodenow 实践**：

这是 uncode 做得最彻底的一个维度。

| 系列要求 | uncode 实现 | 力度 |
|:---|:---|:---|
| **完整的事件模型** | `AgentEvent` 18 个变体，覆盖 session/turn/message/content/tool/compaction/queue/error 全生命周期 | 极细粒度 |
| **事件驱动 UI** | `broadcast::Sender<AgentEvent>` (容量 256) 广播到 TUI/Platform；UI 不直接驱动循环 | 教科书级解耦 |
| **事件顺序可验证** | `validate_pi_turn_lifecycle_order()` 可在测试中验证 Turn 内事件是否符合线性秩约束 | 自我校验 |
| **事件持久化** | `SessionEntry` 树 + SurrealDB 主存储（非事件流本身持久化，但会话等价于事件链的物化） | 已实现 |
| **事件回放** | `SessionStore::load_entries` + `build_context()` 可完整重建 Agent 思考上下文 | 已实现 |
| **训练数据潜力** | JSONL 导入/导出格式兼容 Pi，可直接作为训练语料 | ⚠️ 有接口但未产品化 |

**唯一缺口——事件结构的"训练友好性"**：系列文章提出的 `AgentStep { state, action, observation, feedback }` 模型在 uncode 中尚未对等实现。当前的 `AgentEvent` 偏向**在线系统**（流式消费），缺乏显式的 `feedback` 字段和 `reward` 信号。如果要用于 RL 训练，需要后处理脚本从事件流中提取 trajectory。

**结论**：**在线事件体系已是业界一流**。离线训练链路可作为下一阶段的明确方向。

### 2.6 过度事件化风险 ★★★☆☆

**系列论点**：不是所有东西都值得成为事件，需要采样策略。

**uncodenow 现状**：

当前 uncode 的 18 个事件变体在粒度上已算合理——没有出现 token 级别的事件（那是 Pi 的 `message_update` 风格，uncode 将其聚合为 `ContentDelta` 增量流）。但以下方面仍需警惕：

- **工具进度事件**（`ToolCallProgress`）：在大文件编辑或长时间 bash 执行时，可能产生高频事件。当前未见节流机制。
- **扩展钩子事件**：如果未来 WASM 扩展注册了大量 `ToolCallAfter` 钩子并产生额外事件，可能在批量工具场景下形成事件风暴。
- **JSONL 导出**：全量事件导出在长会话下文件体积可能失控。当前未见"关键决策 vs 中间步骤"的分级导出策略。

**结论**：**当前不在风险区**，但缺少显式的采样/分级策略。建议在设计文档中标注"关键事件（TurnStart/TurnEnd/ToolCallEnd/Error/CompactionComplete）vs 可选事件（ContentDelta/ToolCallProgress）"的划分，并在 JSONL 导出接口上提供 `--detail-level` 参数。

### 2.7 有状态服务的视角转换 ★★★★☆

**系列论点**：服务依然无状态，只是状态被显式外化为一等输入——不是 DDD 被打破，而是终于被迫正确使用它。

**uncodenow 实践**：

```rust
// uncode 的服务函数签名：状态显式参数化
async fn determine_next_action(
    agent_state: AgentState,
    event_history: Vec<DomainEvent>,
    workspace_delta: WorkspaceDiff,
    tool_registry: &ToolRegistry,
) -> Action { ... }
```

`AgentHarness` 在每次 `turn` 中调用 `build_context()` 从外部存储（SurrealDB）重建上下文，服务本身不保存跨 turn 的可变状态。状态完全通过参数或外部存储传入。这完美符合"服务无状态 + 状态一等输入"的架构模式。

**一个小小的工程遗憾**：`AgentHarness` 内部维护了 `pending_writes` 队列用于延迟写入，这虽然不算"有状态服务"（它是写入缓冲而非决策状态），但增加了测试复杂度。

**结论**：**贯彻得很好**。参数化的状态传递是可测试性的关键保证。

---

## 三、7 种架构范式的覆盖度评估

| # | 范式 | 覆盖状态 | uncodenow 实现 |
|:---:|:---|:---:|:---|
| 1 | **工作流编排** | ★★★☆☆ | 未显式实现。双层 Agent 循环是隐式工作流（turn 粒度），但缺少声明式的步骤 DAG 和可视化编排。可考虑未来引入 Skill Pipeline。 |
| 2 | **有限状态机** | ★★★★☆ | `Phase` 枚举（Idle/Turn/Compaction/BranchSummary/Retry）提供了简洁的状态机；`AgentState`（idle/working/waiting）管理生命周期。 |
| 3 | **事件驱动架构** | ★★★★★ | `broadcast` channel + 18 种 `AgentEvent` + `EventRouter` 双通道（sync/hook handler），TUI/Platform 通过订阅消费。教科书级实现。 |
| 4 | **CQRS** | ★★★☆☆ | 读写隐式分离但未显式建模。`SessionEntry` 树 + 路径查询可视为读模型；事件追加为写模型。但缺少独立的 Command/Query 抽象和专门的读存储。 |
| 5 | **事件溯源** | ★★★★☆ | `SessionEntry` 追加即事件溯源；`build_context()` 从事件流重建状态；JSONL 导入/导出支持完整回放。缺口：缺少从事件流重建"任一历史时刻"的 API。 |
| 6 | **约束式设计** | ★★★★☆ | 护栏体系（PATH 限制、ACTIVE_TOOLS 过滤、MAX_TURNS、PHASE 锁）构成多维度约束。缺口：约束声明分散，缺少集中式的"约束策略文件"。 |
| 7 | **多 Agent 协作** | ★☆☆☆☆ | 未实现。当前为单 harness 双环架构，Pi 哲学明确"无内置子 Agent"。如果有方向探索团队 Agent 场景，需单独设计。 |

**范式组合评价**：uncodenow 在**事件驱动 + 事件溯源 + 约束设计**三条线上做了深度融合，形成一个"可观测、可审计、可约束"的稳定三角形。工作流和 CQRS 的缺席不构成架构缺陷——它们解决的是"步骤编排"和"读写分离"问题，在 uncode 当前规模下不是瓶颈。

---

## 四、核心论点的验证

### "确定性内核 + 概率性外延 + 事件作为桥梁"

| 组件 | 映射 | 状态 |
|:---|:---|:---|
| **确定性内核** | `uncode-agent` (LoopEngine, AgentHarness, Phase, MAX_TURNS) | ✅ 稳固 |
| **概率性外延** | `uncode-ai` (Api trait, 4 协议, StreamEvent, CompatConfig) | ✅ 稳固 |
| **事件作为桥梁** | `AgentEvent` 18 变体 + broadcast + EventRouter 双通道 | ✅ 稳固 |

**三点之间唯一的薄弱环节**：事件模型缺少明确的 **feedback/reward** 通道。当前 Agent 可以从工具输出中推断成功/失败（`terminate`, `Error`），但没有一个显式的"人类评价"入口可以以事件的形式注入到事件流中。如果未来要支持 "RLHF 风格的 Agent 微调"，这是一个明确的架构缺口。

### "LLM 负责生成可能性，DDD 负责约束可能性"

uncode 是这句话的活例：
- LLM 通过 `uncode-ai` 的自由 API 生成任何文本 / 工具调用计划
- `uncode-agent` 通过 JSON Schema 验证、护栏、Phase 守卫和 turn 上限，确保只有合法的行动被执行

**唯一偏离**：uncode 没有实现"反事实验证"——即同时对多个 LLM 生成的候选方案进行评估和裁决。当前架构假设 LLM 单次响应即为最终方案。多候选 rerank 模式是后续可探索的方向。

---

## 五、亮点与问题

### 架构亮点（值得其他项目学习）

1. **限界上下文的 Rust 编译级隔离**。`uncode-ai` 和 `uncode-agent` 之间的 trait 边界比 Pi 的 JS 包边界更硬，编译时就能捕获耦合。

2. **事件系统的完整性**。18 个变体覆盖全生命周期，`EventRouter` 的双通道设计（观察型 vs 控制型）是实用的区分。

3. **护栏系统的工程化深度**。不是停留在"加个 validation"的程度，而是深入到 Phase 状态机、terminate AND 语义、CancellationToken 检查点多层配合。

4. **会话模型的逻辑-物理分离**。逻辑层与 Pi 同构（保证互操作性），物理层选 SurrealDB（保证伸缩性）——这是对"尊重原文 + 工程务实"的良好平衡。

### 架构短板（需要在后续迭代中解决）

1. **Normalization 层不独立**。LLM 输出到确定性命令的转换分散在多处，缺少一个统一的"语义规范化"调度器。

2. **不确定性领域建模缺失**。三种不确定性（生成/认知/执行）在代码中未显式类型化，导致错误处理和恢复策略的领域语义不够清晰。

3. **缺乏事件分级/采样策略**。全量事件输出在长会话场景下可能面临存储和传输压力。

4. **WASM 扩展生态尚未成熟**。Hook 生命周期已定义，但实际可加载的 WASM 扩展和开发者体验（打包、分发、热加载）仍在早期。

5. **缺少声明式约束策略层**。护栏参数散落在各处（AppConfig、常量、hook 逻辑），不利于非开发人员或不熟悉代码的用户配置安全策略。

---

## 六、演进建议（按优先级）

### P0：巩固内核

- 将不确定性分类（generation/cognition/execution）显式建模到 `AgentEvent::Error` 或独立的 `FailureClass` 类型中
- 引入统一的 `CommandNormalizer` 调度器，收口 LLM 输出到领域命令的转换逻辑
- 在事件系统文档中标注"关键事件 vs 可选事件"的分级，支持 JSONL 导出时的 `--detail-level` 控制

### P1：完善护栏

- 实现声明式约束策略文件（YAML/TOML），将分散的护栏参数集中配置
- 增加 `AgentStep { state, action, observation, feedback? }` 事件模型，为离线 RL 训练预留数据接口

### P2：扩展治理

- WASM 扩展宿主的最小可用版本（文件/子进程/HTTP 白名单内的基本能力）
- CQRS 的显式建模（独立的读存储 + Command/Query 分离），利好 Platform 多客户端场景
- 多候选 rerank 模式（对同一 turn 生成多个方案，护栏层裁决最优）

---

## 七、总结

uncodenow 在 AI Agent 架构治理的实践中，处于一个非常有趣的位置：

**它不是 DDD 理论的应用者——它是 DDD 理论的验证者。**

系列文章提出的"确定性内核 + 概率性外延 + 事件作为桥梁"的核心公式，在 uncode 中不是理论假设，而是**已编译运行的 Rust crate 分层**。很多在文章中讨论的概念——语义防火墙、决策授权单元、护栏系统——在 uncode 的代码中都有实质性的工程实现。

当然，uncode 也有它的阶段局限：WASM 扩展生态、feedback channel、声明式策略配置、多 Agent 协作——这些在系列文章中提到的治理手段，uncode 尚未覆盖或尚在起步。

但正如系列文章的核心立场：**"不是 DDD 死了，而是它换了一个更难的战场。"** uncode 正在那个战场上，用 Rust 语言的力量，将 DDD 的"边界治理"哲学编译成可运行的软件。这不是一个完美的项目，但它清晰地表明了：**"AI Agent 架构治理"不是纸上谈兵，而是可以落地的工程实践。**

---

*本文档随 uncode 架构演进与系列文章更新应定期修订。*
