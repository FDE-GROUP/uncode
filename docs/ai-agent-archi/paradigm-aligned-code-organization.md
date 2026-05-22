# 认知显化与决策驱动设计：代码组织与命名规范

> 本文是从范式出发的理想代码组织结构定义，不受 uncode 当前实现约束。  
> 目标：让目录树本身讲述范式故事——开发者在 30 秒内通过 `ls` 就能画出架构图。

---

## 一、核心原则

| # | 原则 | 含义 |
|:---:|:---|:---|
| 1 | **目录即架构图** | `ls crates/` 的输出应直接映射到范式地图的四层结构 |
| 2 | **层间编译隔离** | 每一层是独立 crate；层间通过 shared crate 中的 trait 交互 |
| 3 | **防火墙是唯一通道** | 认知层与决策层不直接依赖；所有跨层通信经过 `firewall` crate |
| 4 | **决策管线文件化** | 提案 → 裁决 → 执行 → 审计四阶段，各为一个文件 |
| 5 | **治理层正交于范式层** | 事件驱动、事件溯源等治理模式各自为独立模块，可单独替换 |

---

## 二、Crate 组织结构

```
crates/
├── shared/                    # 全项目共享：错误、配置、基础类型
│   ├── Cargo.toml             # [package] name = "shared"
│   └── src/
│       ├── lib.rs
│       ├── error.rs           # CogError, DecisionError, FirewallError, GovernanceError
│       ├── config.rs          # AppConfig
│       └── types.rs           # 共享 newtype：TurnId, SessionId, ToolName, ...
│
├── cognition/                 # 认知层 crate 组
│   ├── cognition-core/        # 认知层的领域模型和上下文构建
│   │   ├── Cargo.toml         # [package] name = "cognition-core"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── context.rs     # ContextBuilder — 从事件流重建认知上下文
│   │       ├── prompt.rs      # PromptManager — 提示词模板、工具描述生成
│   │       ├── memory.rs      # ConversationMemory — 压缩边界、摘要注入
│   │       ├── uncertainty.rs # UncertaintyClass (Generative/Cognitive/Executional)
│   │       └── proposal.rs    # ActionProposal, ProposedAction — 认知层输出类型
│   │
│   └── cognition-llm/         # LLM 供应商层（当前 uncode-ai）
│       ├── Cargo.toml         # [package] name = "cognition-llm"
│       └── src/
│           ├── lib.rs
│           ├── api.rs         # LlmApi trait（认知层与 LLM 的边界）
│           ├── stream.rs      # StreamEvent, ContentBlock, ContentDelta
│           ├── protocols/     # 协议实现，按协议组织（API-first）
│           │   ├── openai.rs
│           │   ├── anthropic.rs
│           │   ├── gemini.rs
│           │   └── ollama.rs
│           └── models.rs      # 内置模型表 + CompatConfig
│
├── firewall/                  # 语义防火墙 — 认知层与决策层之间的唯一通道
│   ├── Cargo.toml             # [package] name = "firewall"
│   └── src/
│       ├── lib.rs
│       ├── pipeline.rs        # FirewallPipeline — 三层管线的编排
│       ├── parser.rs          # ParseStrategy trait + 默认实现
│       ├── validator.rs       # ValidationRule trait + 内置规则集
│       ├── normalizer.rs      # NormalizeStrategy trait + 默认实现
│       └── types.rs           # ParsedAction, ValidatedAction, NormalizedAction
│
├── decision/                  # 决策层 crate 组
│   ├── decision-core/         # 决策层的领域模型
│   │   ├── Cargo.toml         # [package] name = "decision-core"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs       # ApprovedAction, DeniedAction, DecisionOutcome
│   │       └── context.rs     # DecisionContext (turn, phase, resources, ...)
│   │
│   ├── decision-adjudication/ # 裁决器
│   │   ├── Cargo.toml         # [package] name = "decision-adjudication"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── adjudicator.rs # Adjudicator + DecisionPolicy trait
│   │       ├── policies/      # 内置策略实现
│   │       │   ├── phase_guard.rs      # PhasePolicy
│   │       │   ├── turn_limit.rs       # TurnLimitPolicy
│   │       │   ├── concurrency.rs      # ConcurrencyPolicy
│   │       │   ├── resource_limit.rs   # ResourcePolicy
│   │       │   └── cancellation.rs     # CancellationPolicy
│   │       └── verdict.rs     # DecisionVerdict
│   │
│   ├── decision-execution/    # 执行派发器
│   │   ├── Cargo.toml         # [package] name = "decision-execution"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── orchestrator.rs # ExecutionOrchestrator
│   │       ├── dispatch.rs     # 并行/串行/terminate 语义
│   │       ├── tools/          # 工具实现
│   │       │   ├── registry.rs     # ToolRegistry
│   │       │   ├── bash.rs
│   │       │   ├── read.rs
│   │       │   ├── write.rs
│   │       │   ├── edit.rs
│   │       │   ├── grep.rs
│   │       │   ├── glob.rs
│   │       │   └── web.rs
│   │       └── env.rs          # ExecutionEnv (FileSystem + Shell trait)
│   │
│   └── decision-audit/        # 审计器
│       ├── Cargo.toml         # [package] name = "decision-audit"
│       └── src/
│           ├── lib.rs
│           ├── auditor.rs     # Auditor + AuditTrail
│           ├── trail.rs       # DecisionTrail, DecisionRecord
│           ├── step.rs        # AgentStep { state, action, observation, feedback? }
│           └── replay.rs      # ReplayEngine — 从事件流重建历史时刻
│
├── governance/                # 治理层 — 运行时治理模式
│   ├── governance-event/      # 事件驱动
│   │   ├── Cargo.toml         # [package] name = "governance-event"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── event.rs       # AgentEvent 枚举（全生命周期事件）
│   │       ├── bus.rs         # EventBus (broadcast channel)
│   │       ├── router.rs      # EventRouter (sync handlers + hook handlers)
│   │       └── levels.rs      # EventDetailLevel (Critical/Standard/Verbose)
│   │
│   ├── governance-store/      # 事件溯源 + 会话持久化
│   │   ├── Cargo.toml         # [package] name = "governance-store"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── session.rs     # SessionStore trait + SurrealDB 实现
│   │       ├── entry.rs       # SessionEntry 树
│   │       ├── snapshot.rs    # SessionSnapshot, TaskSnapshot
│   │       └── export.rs      # JSONL 导入/导出
│   │
│   ├── governance-constraint/ # 约束式设计
│   │   ├── Cargo.toml         # [package] name = "governance-constraint"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── guardrail.rs   # GuardrailConfig (来自 guardrails.yaml)
│   │       ├── schema.rs      # ToolParameterSchema (JSON Schema 生成)
│   │       ├── path.rs        # PathSafety — normalize_path, resolve_path
│   │       └── resource.rs    # ResourceLimiter
│   │
│   └── governance-state/      # 有限状态机
│       ├── Cargo.toml         # [package] name = "governance-state"
│       └── src/
│           ├── lib.rs
│           ├── phase.rs       # AgentPhase (Idle/Turn/Compaction/BranchSummary/Retry)
│           └── lifecycle.rs   # LifecycleStateMachine
│
├── harness/                   # Harness — 编排所有层次的入口
│   ├── Cargo.toml             # [package] name = "harness"
│   └── src/
│       ├── lib.rs
│       ├── engine.rs          # AgentEngine — 双层循环（ReAct + follow_up）
│       ├── turn.rs            # TurnContext, TurnResult
│       ├── compaction.rs      # compact_if_needed, should_compact
│       ├── steering.rs        # MessageQueue (steering/follow_up/next_turn)
│       └── skill.rs           # SkillRegistry
│
├── interface/                 # 入口点 — 只订阅事件，不驱动循环
│   ├── tui/                   # 终端 UI
│   │   ├── Cargo.toml         # [package] name = "tui"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs         # TuiApp — 订阅 AgentEvent，渲染
│   │       └── renderers/     # 各工具类型的渲染器
│   │
│   ├── platform/              # Web 服务
│   │   ├── Cargo.toml         # [package] name = "platform"
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs      # Axum HTTP server
│   │       └── routes/        # REST/SSE routes
│   │
│   └── cli/                   # 命令行入口
│       ├── Cargo.toml         # [package] name = "cli"
│       └── src/
│           ├── main.rs
│           ├── args.rs        # Clap 参数定义
│           └── modes/         # 三种运行模式
│               ├── interactive.rs
│               ├── oneshot.rs
│               └── daemon.rs
│
└── extensions/                # WASM 扩展运行时（可选层）
    ├── Cargo.toml             # [package] name = "extensions"
    └── src/
        ├── lib.rs
        ├── runtime.rs         # WASM 宿主
        └── hooks.rs           # LifecycleHook (8 个钩子)
```

---

## 三、依赖关系（编译级隔离）

```
                        shared
                       ╱  │  ╲
                      ╱   │   ╲
              cognition-core │  governance-state
                  │          │
          ┌───────┤          │
          │       │          │
   cognition-llm  │          │
          │       │          │
          └───┬───┘          │
              │              │
           firewall          │
              │              │
    ┌─────────┼─────────┐    │
    │         │         │    │
decision-  decision-  decision-
core       execution  audit
    │         │         │
    └────┬────┘         │
         │              │
   decision-            │
   adjudication         │
         │              │
         └──────┬───────┘
                │
    ┌───────────┼───────────┐
    │           │           │
governance-  governance-  governance-
event        store        constraint
    │           │           │
    └───────────┼───────────┘
                │
             harness
                │
        ┌───────┼───────┐
        │       │       │
       tui   platform   cli
```

**关键约束**：

| 规则 | 含义 |
|:---|:---|
| `cognition-*` 不依赖 `decision-*` | 认知层完全不知道决策层的存在 |
| `decision-*` 不依赖 `cognition-*` | 决策层不接触 LLM 或自然语言 |
| `firewall` 依赖 `cognition-core`（输出类型）和 `decision-core`（输入类型） | 防火墙是唯一的双向依赖点 |
| `harness` 依赖所有层 | 编排器是唯一知晓全貌的模块 |
| `interface/*` 只依赖 `governance-event` | UI 只看事件，不依赖任何内部实现 |

---

## 四、命名规范

### 4.1 Crate 命名

| 范式层 | Crate 名称 | 命名规则 |
|:---|:---|:---|
| 认知层 | `cognition-core`, `cognition-llm` | `cognition-{职责}` |
| 防火墙 | `firewall` | 单层单 crate |
| 决策层 | `decision-core`, `decision-adjudication`, `decision-execution`, `decision-audit` | `decision-{阶段}` — 四阶段各一个 crate |
| 治理层 | `governance-event`, `governance-store`, `governance-constraint`, `governance-state` | `governance-{范式}` — 每个治理范式一个 crate |
| 编排 | `harness` | 单 crate，职责是"编排"而不是"做" |
| 入口 | `tui`, `platform`, `cli` | 以交付面命名 |

### 4.2 类型命名

**核心规则**：类型名应让开发者一眼看出它属于认知层还是决策层。

#### 认知层类型 — 前缀或用词体现"生成/候选/提议"

| 类型 | 职责 | 命名理由 |
|:---|:---|:---|
| `ActionProposal` | LLM 输出的原始提案 | "Proposal"明确表示这是 LLM 的提议，不是系统的决定 |
| `ProposedAction` | 经过防火墙初步解析后的提案 | "Proposed"表示仍在候选状态 |
| `CognitionContext` | 认知层的上下文输入 | "Cognition"显式标注层次归属 |
| `ContextBuilder` | 从事件流重建认知上下文 | 动词 "Build" 表示构造过程 |
| `PromptTemplate` | 系统提示词模板 | "Template"表示可变 |
| `UncertaintyClass` | 不确定性分类（Generative/Cognitive/Executional） | "Class"而非"Error"——不确定性不一定是错误 |
| `ConversationMemory` | 会话记忆管理 | "Memory"是认知心理学标准术语 |
| `LlmApi` | LLM 供应商接口 | "Api"而非"Driver"——避免暗示这层做决策 |

#### 防火墙类型 — 用词体现"管线/过滤"

| 类型 | 职责 |
|:---|:---|
| `SemanticFirewall` | 防火墙入口 Facade |
| `FirewallPipeline` | 三层管线编排器 |
| `ParseStrategy` | Parsing 策略 trait |
| `ValidationRule` | 验证规则 trait |
| `NormalizeStrategy` | 规范化策略 trait |
| `ParsedAction` | Parsing 之后的结构化动作 |
| `ValidatedAction` | Validation 之后的合法动作 |
| `NormalizedAction` | Normalization 之后的最终形式 |

**关键**：`ParsedAction` → `ValidatedAction` → `NormalizedAction` 的状态机命名让数据在管线中的位置一目了然。

#### 决策层类型 — 前缀或用词体现"裁决/批准/执行/审计"

| 类型 | 职责 | 命名理由 |
|:---|:---|:---|
| `DecisionContext` | 裁决时所需的上下文快照 | "Context"在决策语义下 |
| `DecisionPolicy` | 裁决策略 trait | "Policy"——裁决的核心抽象 |
| `Adjudicator` | 裁决器 | "Adjudicator"而非"Validator"——验证是防火墙的事，裁决是决策的事 |
| `DecisionVerdict` | 裁决结果（允许/拒绝 + 理由） | "Verdict"是裁决的自然结果 |
| `ApprovedAction` | 被批准的可执行动作 | "Approved"——经过了裁决 |
| `DeniedAction` | 被拒绝的动作 + 拒绝原因 | 与 ApprovedAction 形成对偶 |
| `ExecutionOrchestrator` | 执行编排器 | "Orchestrator"——管理并行/串行/terminate |
| `AuditTrail` | 审计轨迹 | "Trail"——留下痕迹 |
| `DecisionRecord` | 单次决策的完整记录 | 比 `DecisionMade` 更中性，不预设结果 |
| `AgentStep` | 面向训练的决策步骤 | "Step"——RL trajectory 的标准术语 |
| `ReplayEngine` | 决策回放引擎 | "Replay"——事件溯源的标准动词 |

#### 治理层类型 — 保持行业标准术语

| 类型 | 范式 | 
|:---|:---|
| `AgentEvent` | 事件驱动 |
| `EventBus` | 事件驱动 |
| `EventRouter` | 事件驱动 |
| `EventDetailLevel` | 事件驱动（分级） |
| `SessionStore` | 事件溯源 |
| `SessionSnapshot` | 事件溯源 |
| `GuardrailConfig` | 约束设计 |
| `PathSafety` | 约束设计 |
| `ResourceLimiter` | 约束设计 |
| `AgentPhase` | 状态机 |
| `LifecycleStateMachine` | 状态机 |

### 4.3 Trait 命名

**规则**：以 `-able` 或能力动词结尾的 trait 描述"能做什么"；以名词结尾的 trait 描述"是什么角色"。

| Trait | 位置 | 命名理由 |
|:---|:---|:---|
| `LlmApi` | `cognition-llm` | 描述"是什么"——LLM API 的角色 |
| `ParseStrategy` | `firewall` | 描述"是什么"——解析策略的角色 |
| `ValidationRule` | `firewall` | 描述"是什么"——验证规则的角色 |
| `NormalizeStrategy` | `firewall` | 描述"是什么"——规范化策略的角色 |
| `DecisionPolicy` | `decision-adjudication` | 描述"是什么"——裁决策略的角色 |
| `ToolExecutor` | `decision-execution` | 描述"是什么"——工具执行器的角色 |
| `FileSystem` | `decision-execution` | 描述"是什么"——文件系统抽象的角色 |
| `Shell` | `decision-execution` | 描述"是什么"——Shell 抽象的角色 |
| `SessionStore` | `governance-store` | 描述"是什么"——会话存储的角色 |

### 4.4 模块文件命名

**规则**：文件名 = 单一职责的名词。不用 `mod.rs` 以外的 `util`、`common`、`helper` 之类模糊名称。

**禁止使用的文件名**：
- `utils.rs` / `common.rs` / `helpers.rs` — 无法从文件名推断职责
- `types.rs` — 可以，但只在它是"该模块的共享类型集合"时，不是"不知道放哪的类型"

**推荐的命名模式**：

| 职责 | 文件名 | 说明 |
|:---|:---|:---|
| 该模块的入口和 re-export | `mod.rs` | Rust 惯例 |
| 该模块的共享类型定义 | `types.rs` | 仅在类型足够多、值得单独文件时 |
| 错误类型定义 | `error.rs` | 每个 crate 一个 |
| 管线编排逻辑 | `pipeline.rs` | 而不是 `orchestrator.rs`（后者留给执行层） |
| 配置加载 | `config.rs` | 或 `guardrail.yaml` 外部文件 |
| 策略/规则集合 | `policies/` 目录 + 各文件以策略名命名 | 如 `phase_guard.rs`, `turn_limit.rs` |
| 协议实现 | `protocols/` 目录 + 各文件以协议名命名 | 如 `openai.rs`, `anthropic.rs` |

---

## 五、一个决策的完整旅程——通过代码组织追踪

在这个组织结构下，追踪"Agent 说 '修改 auth.ts' → 系统执行 → 审计记录"的完整路径：

```
1. cognition-llm/src/protocols/openai.rs
   └─ LlmApi::stream() 返回 StreamEvent::ToolCall
        │
2. cognition-core/src/proposal.rs
   └─ ActionProposal::from_stream_event()   # 原始提案
        │
3. firewall/src/pipeline.rs
   └─ FirewallPipeline::process(proposal)   # 三层处理
        ├─ parser.rs    → ParsedAction
        ├─ validator.rs → ValidatedAction
        └─ normalizer.rs → NormalizedAction
        │
4. decision-adjudication/src/adjudicator.rs
   └─ Adjudicator::adjudicate(normalized, context) → ApprovedAction
        │
5. decision-execution/src/orchestrator.rs
   └─ ExecutionOrchestrator::dispatch(approved) → Vec<ToolResult>
        │
6. decision-audit/src/auditor.rs
   └─ Auditor::record(decision, results) → DecisionRecord
        │
7. governance-event/src/bus.rs
   └─ EventBus::publish(DecisionMade { ... })
        ├─ → tui/src/app.rs  (TUI 更新)
        └─ → governance-store/src/session.rs (持久化)
```

**新开发者只需沿着文件路径走，就能理解一个决策的完整生命周期。**

---

## 六、与当前 uncode 结构的对照迁移

| 当前 crate | 目标 crate(s) | 说明 |
|:---|:---|:---|
| `uncode-shared` | → `shared` | 改名 |
| `uncode-macros` | → 保留在 `macros/` 或合并到 `decision-execution` | `#[tool]` 宏主要服务工具注册 |
| `uncode-ai` | → `cognition-llm` | 重新定位：不是"AI 层"，是"认知层的 LLM 供应商" |
| `uncode-core` | → 拆分到 `shared` + `cognition-core` + `decision-core` + `governance-event` + `governance-store` | 当前 `uncode-core` 承担了太多职责 |
| `uncode-agent` | → 拆分到 `harness` + `decision-execution` + `decision-adjudication` + `decision-audit` + `cognition-core` | 当前 `uncode-agent` 是决策层的主要载体，需要拆 |
| `uncode-extensions` | → `extensions` | 改名 |
| `uncode-tui` | → `interface/tui` 或 `tui` | 重新定位为入口点 |
| `uncode-platform` | → `interface/platform` 或 `platform` | 同上 |
| `uncode-cli` | → `interface/cli` 或 `cli` | 同上 |
| `uncode-rpc` | → 合并到 `interface/` 或保留独立 | 规划中 |

---

## 七、为什么这个结构是"最佳实践"

1. **目录即文档**：`ls crates/` 输出 `cognition/ firewall/ decision/ governance/ harness/ interface/` ——这与范式地图的四层结构一一对应。不需要读代码，不需要看文档，目录树已经讲完了架构故事。

2. **编译级强制执行范式**：`cognition-*` 的 `Cargo.toml` 中不出现 `decision-*` 依赖。任何跨层耦合在编译时暴露，不需要代码审查。

3. **语义防火墙是独立 crate**：不是"decision 里的一个模块"，而是独立的 `firewall` crate。它的地位和认知层、决策层平等——三者共同构成范式核心。

4. **四阶段决策管线文件化**：`decision-adjudication/` `decision-execution/` `decision-audit/` 各为一个 crate。需要改裁决逻辑？只打开 `decision-adjudication/`。需要加工具？只打开 `decision-execution/src/tools/`。

5. **治理模式可替换**：`governance-store` 中 `SessionStore` 是 trait。想从 SurrealDB 换成 SQLite？实现同一个 trait，替换 crate 依赖即可。`governance-constraint` 同理。

6. **入口点零耦合**：`tui` 只依赖 `governance-event`。它不知道认知层用什么 LLM，不知道决策层用什么策略，不知道防火墙有几层——它只订阅事件。这是 Harness 与 UI 解耦的极致。

7. **命名自文档化**：`ApprovedAction` vs `DeniedAction`、`ActionProposal` vs `ApprovedAction`、`ParsedAction` → `ValidatedAction` → `NormalizedAction`——这些类型名组成了一条自然语言叙事链，读代码的人不需要查字典就知道数据处在管线的哪个阶段。

---

*本规范随范式定义演进同步更新。实现时应优先对齐命名和目录结构，再逐步迁移代码逻辑。*
