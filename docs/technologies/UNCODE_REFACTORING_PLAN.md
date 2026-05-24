# uncode 重构技术方案：对齐范式文档

> **基准**：`docs/agent-archi/` 系列（9 篇范式定义） vs 当前源码实现
> **审计日期**：2026-05-24
> **总测试覆盖**：800+ tests，14/15 核心组件实现

---

## 一、差距总览

### 对齐度矩阵

| 范式层 | 类型定义 | 逻辑实现 | 主线集成 | 综合 |
|:---|:---:|:---:|:---:|:---:|
| 认知层 | ✅ 100% | ✅ 100% | ✅ 100% | **95%** |
| 语义防火墙 | ✅ 100% | ✅ 100% | ⚠️ 部分 | **70%** |
| 决策层 | ✅ 100% | ✅ 100% | ⚠️ 部分 | **75%** |
| 治理层 | ✅ 90% | ⚠️ 60% | ⚠️ 30% | **55%** |

### 关键差距清单

| ID | 差距 | 严重度 | 位置 |
|:---|:---|:---:|:---|
| G-1 | **无本体模块** — TypeRegistry / ConstraintAxioms / ActionMetadata 均不存在 | 🔴 | 需新建 crate |
| G-2 | **DefaultNormalizer 是空操作** — `normalized_fields: vec![]`，无字段规约、默认值填充、引用解析 | 🔴 | `firewall.rs:146-153` |
| G-3 | **GuardrailConfig 未运行时加载** — 类型定义完整但从未从 `.uncode/guardrails.yaml` 加载 | 🟡 | `guardrails.rs:10` |
| G-4 | **EventRouter 未在 AgentLoop 中使用** — 事件系统定义了但主循环不使用 | 🟡 | `event.rs:788` |
| G-5 | **不确定模型仅用于日志分类** — `UncertaintyClass` 三种类型在 `loop_engine.rs:1912-1916` 已用于工具错误的分类标签（generative_uncertainty / cognitive_gap / execution_error），但未驱动行为决策（如恢复策略选择、回退路径） | 🟡 | `cognition/uncertainty.rs`, `loop_engine.rs:1912` |
| G-6 | **决策管线缺乏完整提案上下文和细粒度事件** — 管线已是前门控模式（`loop_engine.rs:1592-1636`：firewall.process → adjudicator.adjudicate → allowed=true 才执行），但 `ActionProposal` 缺少 `proposal_id`/`intent`/`alternatives`/`trace`（G-7），且未发射 `ProposalReceived`/`FirewallCheck`/`DecisionAudited` 等细粒度事件 | 🟡 | `loop_engine.rs:1592-1636` |
| G-7 | **ActionProposal 缺少范式定义的字段** — 无 `proposal_id`(Uuid)、`intent`(IntentType)、`alternatives`、`trace` | 🟢 | `decision/types.rs:20-25` |
| G-8 | **无状态机定义** — 范式定义的六状态机（初始→认知中→裁决中→执行中→等待→终止）未在代码中显式建模 | 🟢 | — |
| G-9 | **Tool 定义非本体驱动** — 工具 Schema 由 `#[tool]` 宏手写，非从类型注册表生成 | 🟢 | `macros/src/lib.rs` |

---

## 二、重构阶段

### Phase 0：快速连线（1-2 天，低风险）

**目标**：激活已实现但未使用的组件，消除空操作。

#### 0.1 激活 GuardrailConfig 运行时加载

**文件**：`uncode-shared/src/guardrails.rs`, `uncode-cli/src/main.rs`

```rust
// 在 CLI 入口加载 guardrails.yaml
let guardrail_path = workspace_root.join(".uncode/guardrails.yaml");
let guardrail_config = if guardrail_path.exists() {
    let content = std::fs::read_to_string(&guardrail_path)?;
    serde_yaml::from_str::<GuardrailConfig>(&content).unwrap_or_default()
} else {
    GuardrailConfig::default()
};
```

将 `GuardrailConfig` 注入 `AgentLoop::new()` 或通过 `set_guardrail_config()` 方法传递。

#### 0.2 替换 DefaultNormalizer 为可配置版本

**文件**：`uncode-agent/src/decision/firewall.rs:144-153`

当前 `DefaultNormalizer` 是空操作。在无本体模块的情况下，至少改为参数化版本：

```rust
pub struct DeclarativeNormalizer {
    /// 字段名映射表：LLM 输出字段名 → 规范字段名
    field_mapping: HashMap<String, String>,
    /// 默认值表：工具名 → 字段名 → 默认值
    defaults: HashMap<String, HashMap<String, serde_json::Value>>,
}

impl NormalizeStrategy for DeclarativeNormalizer {
    fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError> {
        let mut args = action.arguments.clone();
        let mut normalized_fields = Vec::new();
        // 字段名规范化
        if let serde_json::Value::Object(ref map) = args {
            for (key, _) in map.clone() {
                if let Some(canonical) = self.field_mapping.get(&key) {
                    if canonical != &key {
                        normalized_fields.push(format!("{key} → {canonical}"));
                        if let Some(val) = map.get(&key) {
                            args[canonical] = val.clone();
                        }
                    }
                }
            }
        }
        // 默认值填充
        if let Some(tool_defaults) = self.defaults.get(&action.tool_name) {
            for (field, default) in tool_defaults {
                if args.get(field).is_none() {
                    args[field] = default.clone();
                    normalized_fields.push(format!("{field} = default"));
                }
            }
        }
        Ok(NormalizedAction {
            tool_name: action.tool_name.clone(),
            arguments: args,
            normalized_fields,
        })
    }
}
```

配置来源：`FirewallConfig` 中新增 `normalizer` 段。

#### 0.3 将 GuardrailConfig 接入防火墙和裁决器

**文件**：`uncode-agent/src/loop_engine.rs:1570-1578`

当前防火墙使用 `build_default_firewall()` 硬编码三种策略。改为从 `GuardrailConfig` 构建：

```rust
let firewall = SemanticFirewall::from_config(&guardrail_config, registry, cwd)?;
let adjudicator = Adjudicator::from_config(&guardrail_config, phase, cancel_token)?;
```

在 `AgentLoop` 上新增 `set_guardrail_config()` 方法，供 CLI 入口注入。

---

### Phase 1：本体基础（3-5 天，中等风险）

**目标**：建立 `uncode-ontology` crate，实现类型注册表和约束公理的基础形式。

#### 1.1 新建 `uncode-ontology` crate

**目录结构**：

```
crates/uncode-ontology/
├── Cargo.toml
└── src/
    ├── lib.rs              # crate 入口
    ├── registry.rs         # TypeRegistry
    ├── types.rs            # EntityDef, ValueDef, ActionDef
    ├── constraints.rs      # Constraint, ConstraintLevel
    ├── mapping.rs          # 字段名映射表
    └── version.rs          # 本体版本管理
```

**核心类型**（对齐 `06-ontology.md`）：

```rust
// types.rs
pub struct TypeRegistry {
    pub entities: HashMap<TypeId, EntityDef>,
    pub values: HashMap<TypeId, ValueDef>,
    pub actions: HashMap<TypeId, ActionDef>,
}

pub struct EntityDef {
    pub id: TypeId,
    pub fields: Vec<FieldDef>,
    pub invariants: Vec<Constraint>,
    pub extends: Option<TypeId>,
}

pub struct ActionDef {
    pub name: String,
    pub input_schema: JsonSchema,
    pub output_type: TypeId,
    pub preconditions: Vec<Constraint>,
    pub effects: Vec<Effect>,
}

// constraints.rs
pub enum ConstraintLevel {
    Hard,   // 违反则拒绝
    Soft,   // 违反则告警但通过
}

pub enum Constraint {
    TypeCheck { field: String, expected: String },
    RangeCheck { field: String, min: Option<f64>, max: Option<f64> },
    RequiredField { field: String },
    ReferentialIntegrity { field: String, target_type: TypeId },
    CustomRule { name: String, description: String },
}
```

**依赖**：仅 `serde` + `serde_json`（叶子 crate，不依赖 `uncode-shared`）。

#### 1.2 编码 uncode 的领域本体

**文件**：`uncode-ontology/src/builtin.rs`（新增）

定义 uncode 的顶层领域类型——反映"这是一个 coding agent"的领域认知：

```rust
pub fn coding_agent_ontology() -> TypeRegistry {
    TypeRegistry {
        actions: {
            // 每个工具对应一个 ActionDef
            "read".into() => ActionDef {
                input_schema: json_schema!({ "path": "string" }),
                preconditions: vec![
                    Constraint::RequiredField("path".into()),
                    Constraint::CustomRule("file exists".into(), "path must exist".into()),
                ],
                effects: vec![Effect::ReadOnly { target: "file".into() }],
            },
            // ... 其他工具
        },
        entities: {
            "File".into() => EntityDef { /* ... */ },
            "Workspace".into() => EntityDef { /* ... */ },
        },
    }
}
```

#### 1.3 从本体生成 Normalizer 配置

**文件**：`uncode-agent/src/decision/firewall.rs`

```rust
impl DeclarativeNormalizer {
    /// 从 TypeRegistry 的 ActionDef 生成字段映射和默认值
    pub fn from_ontology(registry: &TypeRegistry) -> Self {
        let mut field_mapping = HashMap::new();
        let mut defaults = HashMap::new();
        for (name, action) in &registry.actions {
            // 从 input_schema 提取字段映射和默认值
            // ...
        }
        Self { field_mapping, defaults }
    }
}
```

#### 1.4 本体→Tool Schema 生成

**文件**：`uncode-macros/src/lib.rs`（扩展）

扩展 `#[tool]` 宏或提供辅助宏，使 ActionDef 可以直接生成 LLM 的 function calling Schema：

```rust
#[tool]
#[ontology(action = "FileRead", effects = ["ReadOnly"])]
async fn read(path: String) -> ToolResult { /* ... */ }
```

---

### Phase 2：决策管线完成（3-4 天，中等风险）

**目标**：补全提案上下文字段，发射细粒度决策事件，完善审计记录。

#### 2.1 扩展 ActionProposal 结构

**文件**：`uncode-agent/src/decision/types.rs`

```rust
pub struct ActionProposal {
    pub proposal_id: Uuid,              // 新增
    pub tool_name: String,
    pub raw_arguments: serde_json::Value,
    pub intent: Option<IntentType>,     // 新增：从本体中选取的意图类型
    pub rationale: Option<String>,
    pub confidence: Option<f32>,
    pub alternatives: Vec<Alternative>, // 新增：多条候选
    pub trace: Vec<CognitiveTrace>,     // 新增：认知路径溯源
}
```

#### 2.2 完善决策管线上下文

**文件**：`uncode-agent/src/loop_engine.rs:1592-1636`

当前管线已是前门控模式——`firewall.process(proposal)` → `adjudicator.adjudicate(normalized, ctx)` → `allowed=true` 才构建执行、`allowed=false` 直接拒绝。这一机制运作正常，无需修改执行顺序。

需要改进的是：

1. **扩展提案流**：在 `ProposalAccumulator` 阶段填充 `ActionProposal` 的新增字段（`proposal_id`、`intent`、`trace`）
2. **增强被拒绝提案的反馈**：当前拒绝后仅记录 `DecisionMade` 事件和日志，需要将拒绝原因和防火墙详情作为 `tool_result`（错误）回注到对话流，使 LLM 能在下一轮修正行为
3. **审计记录持久化**：`PendingAudit` 目前在内存中，turn 结束后丢弃。改为写入 SurrealDB 的 `decision_records` 表，支持事后回溯

#### 2.3 决策事件发射

**文件**：`uncode-agent/src/loop_engine.rs`

在管线各阶段发射事件（对齐 `04-decision-path.md` 的事件流设计）：

```
AgentEvent::ProposalReceived   → 提案接收
AgentEvent::FirewallCheck      → 防火墙检查结果
AgentEvent::DecisionMade       → 裁决结果（已存在）
AgentEvent::ActionExecuted     → 执行完成
AgentEvent::DecisionAudited    → 审计持久化完成
```

---

### Phase 3：治理激活（2-3 天，低风险）

**目标**：事件驱动架构和状态机投入运行。

#### 3.1 EventRouter 接入主循环

**文件**：`uncode-core/src/event.rs`, `uncode-agent/src/loop_engine.rs`

```rust
// 在 AgentLoop 上增加 event_router 字段
event_router: std::sync::Mutex<EventRouter>,

// 每次 emit 后同时路由到 EventRouter
self.event_router.lock().unwrap().route(&event);
```

EventRouter 对接：
- SurrealDB 持久化（事件溯源）
- TUI 实时渲染（事件→UI 状态更新）
- Extension 生命周期钩子（事件→WASM 回调）

#### 3.2 状态机显式建模

**新文件**：`uncode-agent/src/governance/state_machine.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentPhase {
    Init,
    Cognizing,       // 认知中：LLM 正在生成
    Adjudicating,    // 裁决中：防火墙+裁决器运行
    Executing,       // 执行中：工具正在运行
    WaitingForUser,  // 等待人机交互
    Terminated,
}

pub struct PhaseStateMachine {
    current: AgentPhase,
    allowed_transitions: HashMap<AgentPhase, Vec<AgentPhase>>,
    history: Vec<PhaseTransition>,
}
```

集成到 `harness.rs`，替换当前的 `AgentHarnessPhase`。

#### 3.3 治理约束生效

**文件**：`uncode-shared/src/guardrails.rs` → `uncode-agent/src/harness.rs`

GuardrailConfig 中的约束在运行时实际生效：
- `DecisionConfig.turn_limit` → 替换硬编码 MAX_TURNS=50
- `FirewallConfig.path_safety.mode` → 驱动 PathSafetyRule
- `AdjudicationConfig` 的 `auto_approve_safe_tools` → 驱动裁决器短路逻辑

---

### Phase 4：领域优先精炼（持续进行，低风险）

**目标**：工程实践对齐"领域第一公民"原则。

#### 4.1 类型系统审计

- 检查核心领域概念是否编码为 Rust 类型（而非 `String`/`serde_json::Value`）
- `ToolId`、`SessionId`、`FilePath` 等是否有 newtype 包装
- 领域不变量是否作为类型约束而非 assert

#### 4.2 测试聚焦调整

- 当前测试以组件行为为中心 → 以领域行为为中心
- "read tool 返回文件内容" → "读取不存在文件的行为满足领域规范"
- 为每个 ActionDef 的 precondition 提供单元测试

#### 4.3 命名一致性

- 代码命名、文档术语、对话表达使用同一套领域语言
- 对照 `ontology/builtin.rs` 中的 EntityDef 名称

---

## 三、Crate 依赖变更

### 当前依赖图

```
uncode-cli → uncode-tui, uncode-agent, uncode-extensions, uncode-rpc, uncode-platform
uncode-agent → uncode-ai, uncode-core, uncode-shared
uncode-core → uncode-ai (re-export), uncode-shared (re-export)
```

### Phase 1 后

```
uncode-ontology (新) — 叶子 crate，仅依赖 serde
uncode-shared → uncode-ontology  (GuardrailConfig 引用 TypeRegistry)
uncode-agent → uncode-ontology   (防火墙引用 Normalizer 配置)
```

### Phase 2 后

无新增依赖，仅内部类型扩展。

### Phase 3 后

无新增依赖。事件路由和状态机在 `uncode-agent` 内部新增模块。

---

## 四、风险与回滚

| 阶段 | 风险 | 回滚策略 |
|:---|:---|:---|
| Phase 0 | 低 — 激活已有代码，不改变核心路径 | 每个 commit 可通过 CI |
| Phase 1 | 中 — 新建 crate 引入新类型 | 本体 crate 独立，不破坏现有 API；Normalizer 可降级为 DefaultNormalizer |
| Phase 2 | 中 — 扩展提案类型和审计持久化 | 新增字段均为 `Option`/`Vec`，不破坏现有构造；审计持久化可独立开关 |
| Phase 3 | 低 — 事件路由是附加的，不改变核心路径 | EventRouter 出错时仅日志警告，不影响流程 |
| Phase 4 | 低 — 纯工程改进，无行为变更 | — |

---

## 五、验证标准

每个 Phase 完成后验证：

| Phase | 验证 |
|:---|:---|
| Phase 0 | `cargo test` 全部通过；手动运行 uncode CLI 确认 GuardrailConfig 加载日志 |
| Phase 1 | `cargo test -p uncode-ontology` 通过；`DeclarativeNormalizer` 单元测试 `normalized_fields` 非空 |
| Phase 2 | 端到端测试：危险命令被拒绝，安全命令被允许，审计记录包含完整管线信息 |
| Phase 3 | `EventRouter` 集成测试：事件流在 SurrealDB 中可查询；PhaseStateMachine 不允许的转换被拒绝 |
| Phase 4 | 领域行为测试覆盖率 > 80%；命名一致性审计报告 |
