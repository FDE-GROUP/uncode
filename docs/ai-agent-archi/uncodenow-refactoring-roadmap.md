# uncode 重构路线图：打造"认知与决策驱动设计"最佳实践

> 目标：将 uncode 从"隐含地体现了范式"升级为"范式的最佳实践参考实现"  
> 依据：`cognition-decision-driven-design.md`（范式定义） + `uncodenow-architecture-evaluation.md`（现状差距）  
> 原则：每个阶段完成后，新开发者应能从代码结构中直观识别出认知层/决策层/防火墙/治理层的四层边界

---

## 零、差距总览：范式地图与 uncode 现状的对照

| 范式组件 | 范式要求 | uncode 现状（基于代码审查） | 差距等级 |
|:---|:---|:---|:---:|
| **认知-决策分离** | 两层在 crate/模块级别可视 | `uncode-ai` vs `uncode-agent` 已有编译级隔离，但模块内未命名 | 🟡 |
| **语义防火墙** | Parsing → Validation → Normalization 三层独立 | 安全逻辑分散在 `tool_permission.rs`+`permission_gate.rs`+`tools/mod.rs`+`context.rs` 四处 | 🔴 |
| **决策层四阶段** | 提案接收 → 裁决 → 执行 → 审计，各自独立模块 | `loop_engine.rs`（1616行）包含全部四阶段的 inline 逻辑 | 🔴 |
| **不确定性分类** | 生成/认知/执行三类，显式类型建模 | `ErrorCategory` 按来源分（Llm/Tool/Network/Config），不按性质分 | 🟡 |
| **决策即事件** | 每次裁决/执行/回滚 = 事件 | `AgentEvent` 28 变体覆盖全生命周期，但缺显式的 `DecisionMade` | 🟡 |
| **护栏声明式** | 策略配置文件，非代码分散常量 | `PermissionConfig` 嵌入 `AppConfig`；`PermissionPolicy` 编译自配置 | 🔴 |
| **事件分级/采样** | 关键事件 vs 可选事件，导出可控 | 无分级机制 | 🟡 |
| **铁三角治理** | 事件驱动 + 事件溯源 + 约束设计各司其职 | 事件驱动 ★★★★★，事件溯源 ★★★★☆（SurrealDB + JSONL 导出），约束设计 ★★★★☆ | 🟢 |
| **AgentStep 训练模型** | `{ state, action, observation, feedback? }` | 无对等事件类型（最接近的是 `ToolCallEndEventData`） | 🟡 |
| **工作流编排** | 声明式步骤 DAG | 双层 ReAct 循环（隐式），无声明式 | ⚪ |
| **CQRS** | 显式 Command/Query 分离 | 隐式分离（`SessionEntry` 树为读，事件追加为写） | ⚪ |
| **多 Agent 协作** | 角色分工、互相制衡 | 单 harness 双环架构（Pi 哲学有意不做） | ⚪ |

🟢 已对齐　🟡 存在但需显式化　🔴 需要重构引入　⚪ 低优先级阶段

---

## 第一阶段：决策层形式化（核心重构）

**目标**：`uncode-agent/src/` 中新增 `decision/` 目录，将当前散落在 `loop_engine.rs`（1616行）、`AgentHarness`（276行）、工具管线中的决策逻辑提取为独立模块。

**当前代码事实**：
- `loop_engine.rs`（1616行）：`AgentLoop::run_inner()` 包含 LLM 流处理、工具准备/验证/执行、turn 边界管理的全部 inline 逻辑
- `AgentHarness`（276行）：维护 `AgentHarnessPhase`（Idle/Turn/Compaction/BranchSummary/Retry）状态机、`pending_writes` 队列
- 已有安全基础设施：`PermissionPolicy`（`tool_permission.rs`）、`PermissionGate`（`permission_gate.rs`）、`ChainedToolHooks`/`PermissionToolHooks`（`hooks/`）
- 路径安全：`tools/mod.rs` 中的 `resolve_path()`；URL 安全：`uncode-core/src/context.rs` 中的 `fetch_url()` 私有主机阻断
- 事件系统：`AgentEvent` 28 变体，通过 `broadcast::Sender<AgentEvent>`（容量256）分发

### 1.1 创建 `uncode-agent/src/decision/` 模块

```
uncode-agent/src/decision/
├── mod.rs              # pub mod proposal; pub mod adjudication; ...
├── proposal.rs         # 提案接收：ProposedAction, ActionProposal, parse_llm_output
├── adjudication.rs     # 裁决：Policy, Guardrail, Adjudicator
├── execution.rs        # 执行派发：ExecutionOrchestrator, parallel/sequential/terminate
├── audit.rs            # 审计：DecisionTrail, AuditLog, ReplayEngine
├── firewall.rs         # 语义防火墙：Parser → Validator → Normalizer 三层管线
└── types.rs            # 共享类型：ApprovedAction, DeniedAction, DecisionOutcome
```

### 1.2 提案接收层（`proposal.rs`）——从 `loop_engine.rs` 提取 LLM 输出解析

**当前问题**：`AgentLoop::run_inner()` 中 LLM 流式输出的 `ToolCallStart`/`ToolCallDelta`/`ToolCallEnd` 事件处理与 turn 逻辑耦合。工具调用的累积、`ContentBlock` 构造、`arguments` 拼接全部 inline 在 1616 行的循环体中。

**重构**：提取为 `proposal.rs` 中的独立管线。

```rust
// proposal.rs — 提案接收的入口
// 从 loop_engine.rs 的 stream 处理循环中提取
pub struct ActionProposal {
    pub tool_name: String,
    pub raw_arguments: serde_json::Value,  // 尚未验证——由防火墙处理
    pub rationale: Option<String>,
    pub confidence: Option<f32>,
    pub alternatives: Vec<ActionProposal>,  // 预留多候选接口
}

// 对应 loop_engine.rs 中 ToolCallStart → ToolCallDelta → ToolCallEnd 的累积逻辑
pub fn accumulate_proposals(
    stream_events: Vec<StreamEvent>,
) -> Result<Vec<ActionProposal>, ProposalError> {
    // 提取自 run_inner() 中 match StreamEvent::ToolCall* 的分支
    todo!("extract from loop_engine.rs lines ~900-1050")
}
```

### 1.3 语义防火墙（`firewall.rs`）——收口分散的安全逻辑

**当前问题**：安全校验逻辑分散在四个位置，没有统一的管线：

| 当前位置 | 职责 | 对应防火墙层 |
|:---|:---|:---|
| `tool_permission.rs` → `PermissionPolicy::needs_confirmation()` | 危险命令匹配、受保护路径检查、安全命令白名单 | Validation |
| `permission_gate.rs` → `PermissionGate::wait_for_approval()` | 异步审批门控 | Validation（裁决前阻断） |
| `tools/mod.rs` → `resolve_path()` | CWD 范围校验 | Validation |
| `uncode-core/src/context.rs` → `fetch_url()` | 私有主机阻断 | Validation |
| `tools/registry.rs` → `prepare_and_validate()` | JSON Schema 验证 + `coerce` 类型转换 | Parsing + Validation |

**重构**：将这些逻辑包装为 `ValidationRule` trait 实现，统一编排到 `SemanticFirewall` 管线中。

```rust
// firewall.rs — 认知层与决策层之间的唯一通道
pub struct SemanticFirewall {
    parser: Box<dyn ParseStrategy>,
    validators: Vec<Box<dyn ValidationRule>>,
    normalizer: Box<dyn NormalizeStrategy>,
}

pub trait ValidationRule: Send + Sync {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError>;
    fn name(&self) -> &'static str;
}

// 包装现有 PermissionPolicy 为 ValidationRule
pub struct PermissionPolicyRule {
    policy: Arc<PermissionPolicy>,  // 已有实现，无需重写
}

impl ValidationRule for PermissionPolicyRule {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        if self.policy.needs_confirmation(&action.tool_name, &action.arguments) {
            Ok(ValidationVerdict {
                approved: false,
                reason: Some("requires user confirmation".into()),
                violations: vec!["permission policy".into()],
            })
        } else {
            Ok(ValidationVerdict::approved())
        }
    }
    fn name(&self) -> &'static str { "permission_policy" }
}

// 包装现有路径校验
pub struct PathSafetyRule;

impl ValidationRule for PathSafetyRule {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        // 复用 tools/mod.rs 中的 resolve_path() 逻辑
        // ...
    }
    fn name(&self) -> &'static str { "path_safety" }
}

// 包装现有工具白名单
pub struct ToolWhitelistRule {
    registry: Arc<ToolRegistry>,  // 已有实现
}

// 包装现有 Schema 验证
pub struct SchemaCoercionRule {
    registry: Arc<ToolRegistry>,
}
```

**可测试性收益**：每个 `ValidationRule` 可以独立单元测试；防火墙管线可以用 mock parser/normalizer 做集成测试。

### 1.4 裁决器（`adjudication.rs`）——从 `AgentHarness` Phase 守卫提取

**当前问题**：裁决逻辑分散在 `AgentHarness`（Phase 守卫、`active_run` CAS 检查）和 `AgentLoop`（`MAX_TURNS=50`、`CancellationToken` 5 个检查点）中。

**重构**：定义为 `DecisionPolicy` trait 实现，利用现有的 `AgentHarnessPhase` 枚举。

```rust
// adjudication.rs — 裁决器
pub struct Adjudicator {
    policies: Vec<Box<dyn DecisionPolicy>>,
}

pub trait DecisionPolicy: Send + Sync {
    fn evaluate(&self, context: &DecisionContext, action: &NormalizedAction) 
        -> Result<DecisionVerdict, AdjudicationError>;
    fn name(&self) -> &'static str;
}

// 包装现有的 AgentHarnessPhase 逻辑
pub struct PhaseGuardPolicy {
    current_phase: AgentHarnessPhase,  // Idle/Turn/Compaction/BranchSummary/Retry
}

// 包装现有的 MAX_TURNS 常量
pub struct TurnLimitPolicy { max_turns: u32 }  // 默认 50

// 包装现有的 CancellationToken 检查
pub struct CancellationPolicy;

// 包装现有的 CAS 检查（harness.rs 中的 active_run.compare_exchange）
pub struct ConcurrencyPolicy { active_run: Arc<AtomicBool> }

// 裁决器利用现有的 AgentHarnessPhase 枚举
impl DecisionPolicy for PhaseGuardPolicy {
    fn evaluate(&self, _ctx: &DecisionContext, _action: &NormalizedAction) 
        -> Result<DecisionVerdict, AdjudicationError> {
        if self.current_phase != AgentHarnessPhase::Idle 
           && self.current_phase != AgentHarnessPhase::Turn {
            return Ok(DecisionVerdict::denied("Agent is not in an accepting phase"));
        }
        Ok(DecisionVerdict::approved())
    }
    fn name(&self) -> &'static str { "phase_guard" }
}
```

### 1.5 审计器（`audit.rs`）——在现有 28 事件变体上增加 `DecisionMade`

**当前事实**：`AgentEvent` 已有 28 个变体，完整覆盖 session/turn/message/content/tool/compaction/queue/error 全生命周期。但缺少显式的"决策"事件类型——裁决结果目前通过 `ToolCallEnd`、`Error` 变体间接表达。

**重构**：新增 `DecisionMade` 变体（`#[non_exhaustive]` 保证兼容）：

```rust
// audit.rs — 决策审计
// 新增 AgentEvent 变体（或在 uncode-core 中扩展）
pub struct DecisionMade {
    pub turn_id: TurnId,
    pub proposal: ActionProposal,
    pub verdict: DecisionVerdict,
    pub approved_action: Option<ApprovedAction>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub adjudication_duration_ms: u64,
}

// AgentStep — 面向训练的事件模型
pub struct AgentStep {
    pub state: AgentStateSnapshot,      // 决策前的状态
    pub action: ExecutedAction,         // 被批准并执行的动作
    pub observation: ActionObservation, // 执行后环境的观察
    pub feedback: Option<Feedback>,     // 人类或自动化评价
}

pub enum Feedback {
    HumanApproval { approved: bool, comment: Option<String> },
    TestPassed { test_name: String },
    TestFailed { test_name: String, error: String },
    AutoRevert { reason: String },
}
```

### 1.6 重组 `AgentHarness`——从单体到编排器

重构后的 `AgentHarness` 不再直接包含裁决逻辑和工具执行细节，而是编排四个子模块：

```rust
pub struct AgentHarness {
    // 认知侧
    llm_api: Arc<dyn Api>,           // 保持在 uncode-ai 侧
    context_builder: ContextBuilder,  // 认知层的上下文构建（后续阶段重构）
    
    // 决策侧（第一阶段重构产物）
    firewall: SemanticFirewall,
    adjudicator: Adjudicator,
    executor: ExecutionOrchestrator,
    auditor: Auditor,
    
    // 共享
    session_store: SessionStore,
    event_tx: broadcast::Sender<AgentEvent>,
    active_run: AtomicBool,
}

impl AgentHarness {
    async fn run_turn(&self, messages: Vec<Message>) -> Result<TurnResult> {
        // 1. LLM 生成（认知层）→ 产生 ContentBlock 流
        let llm_output = self.call_llm(messages).await?;
        
        // 2. 提案接收（经过语义防火墙）→ 进入决策层
        let proposals = self.firewall.process_batch(&llm_output.tool_calls).await?;
        
        // 3. 裁决
        let approved: Vec<ApprovedAction> = futures::future::join_all(
            proposals.iter().map(|p| self.adjudicator.adjudicate(p, &context))
        ).await?.into_iter().collect::<Result<Vec<_>, _>>()?;
        
        // 4. 执行
        let results = self.executor.dispatch(approved).await?;
        
        // 5. 审计
        self.auditor.record_turn(DecisionMade { /* ... */ }, &results).await?;
        
        Ok(TurnResult { results })
    }
}
```

### 第一阶段产物检查清单

- [ ] `uncode-agent/src/decision/` 目录存在，含 6 个模块文件
- [ ] `SemanticFirewall` 三层 trait 定义 + 至少 4 个 `ValidationRule` 实现
- [ ] `Adjudicator` + 至少 5 个 `DecisionPolicy` 实现
- [ ] `DecisionMade` 作为新 `AgentEvent` 变体在 `uncode-core` 中定义
- [ ] `AgentStep` 作为面向训练的事件模型，支持 `feedback` 字段
- [ ] `AgentHarness` 不再包含裁决逻辑——`adjudicate()` 方法委托给 `Adjudicator`
- [ ] 现有测试通过（重构不改变行为，只改变组织）

---

## 第二阶段：认知层形式化

**目标**：`uncode-agent/src/cognition/` 新增模块，包含提示词管理、不确定性管理。`context_builder.rs` 已存在（可迁移）。

**当前代码事实**：
- `context_builder.rs` 已存在——实现了 `build_context()` 树感知上下文组装和 `BuiltContext` 结构体
- `system_prompt.rs` 已存在——实现了 `SystemPromptBuilder`
- `compaction.rs` 已存在——实现了 `compact_session()`、`should_compact_session()`
- `token.rs` 已存在——实现了 `estimate_tokens()`、`estimate_message_tokens()`、`estimate_cost()`

本阶段新工作：不确定性显式建模 + 提示词管理形式化 + 迁移已有文件。

### 2.1 创建 `uncode-agent/src/cognition/` 模块

```
uncode-agent/src/cognition/
├── mod.rs
├── context_builder.rs   # ← 从 ../context_builder.rs 迁移（已存在）
├── prompt_manager.rs    # ← 包装 ../system_prompt.rs 中的 SystemPromptBuilder
├── uncertainty.rs       # ★ 新增：不确定性三分类建模
└── memory.rs            # ★ 新增：压缩边界管理、摘要注入策略
```

### 2.2 不确定性显式建模（`uncertainty.rs`）

**当前问题**：`ErrorCategory` 按错误来源分（`Llm | Tool | Network | Config`），不按不确定性性质分。`is_retryable()` 检查 `LlmRateLimit` 或 `Network`，`is_context_overflow()` 检查 LLM 消息中的关键词——这些策略隐式编码了不确定性处理，但未显式类型化。

**重构**：新增 `UncertaintyClass` 作为独立领域类型，与 `ErrorCategory` 共存（不替代）。

```rust
// uncertainty.rs — 不确定性的领域建模
pub enum UncertaintyClass {
    /// LLM 采样导致的多候选差异
    Generative(GenerativeConfig),
    /// 上下文不足导致的信息缺口
    Cognitive(CognitiveGap),
    /// 外部系统/工具调用导致的失败
    Executional(ExecutionContext),
}

pub struct GenerativeConfig {
    pub candidates: Vec<String>,
    pub temperature: f32,
    pub strategy: GenerativeStrategy,
}

pub enum GenerativeStrategy {
    Rerank,
    MajorityVote,
    BestOfN(usize),
}

pub struct CognitiveGap {
    pub missing_context: Vec<ContextRequirement>,
    pub suggested_remediation: String,
}

pub enum ContextRequirement {
    FileContent(String),
    Documentation(String),
    WorkspaceStructure,
    PreviousDecision,
}

pub struct ExecutionContext {
    pub error: String,
    pub retry_count: u32,
    pub max_retries: u32,
    pub strategy: ExecutionStrategy,
}

pub enum ExecutionStrategy {
    Retry,
    FallbackTool,
    Compensate,
    Escalate,
}

// 在 AgentEvent 中新增（或重构 Error 变体）
pub enum AgentEvent {
    // ... 现有变体
    UncertaintyEncountered {
        class: UncertaintyClass,
        turn_id: TurnId,
        resolution: UncertaintyResolution,
    },
}

pub enum UncertaintyResolution {
    Resolved { strategy_used: String },
    Escalated { reason: String },
    Unresolved { attempts: u32 },
}
```

### 2.3 提示词管理（`prompt_manager.rs`）

```rust
// prompt_manager.rs — 提示词即领域语言
pub struct PromptManager {
    system_template: SystemPromptTemplate,
    tool_descriptions: ToolDescriptionGenerator,
    role_config: RoleConfiguration,
}

impl PromptManager {
    /// 认知与决策驱动设计的原则 2：提示词是认知层的领域语言
    /// 它编码了"系统期望 LLM 理解什么"——但决策层不接触它
    pub fn build_system_prompt(&self, context: &CognitiveContext) -> String {
        // ...
    }
    
    pub fn build_tool_descriptions(&self, active_tools: &[ToolDefinition]) -> String {
        // ...
    }
}
```

### 第二阶段产物检查清单

- [ ] `uncode-agent/src/cognition/` 目录存在，含 4 个模块文件
- [ ] `context_builder.rs` 已从根目录迁移到 `cognition/` 下，`BuiltContext` 路径更新
- [ ] `UncertaintyClass` 枚举 + 三种策略类型在 `uncode-core` 中定义
- [ ] `AgentEvent::UncertaintyEncountered` 或等效的事件变体（`#[non_exhaustive]`）
- [ ] `PromptManager` 包装 `SystemPromptBuilder`，封装系统提示词和工具描述生成逻辑
- [ ] 现有测试通过（迁移 `context_builder` 不改变行为）

---

## 第三阶段：治理层完善

**目标**：补齐铁三角中约束设计的最后一块（声明式策略配置），引入事件分级和 AgentStep。

### 3.1 声明式约束策略配置

**当前问题**：护栏参数散落在 `AppConfig`、常量定义、hook 逻辑中。

**重构**——引入 `guardrails.yaml`（或 `.uncode/guardrails.yaml`）：

```yaml
# .uncode/guardrails.yaml — 认知与决策驱动设计的可配置护栏
version: 1

decision:
  turn_limit: 50
  max_concurrent_tools: 8
  tool_timeout_seconds: 120
  
firewall:
  path_safety:
    mode: "cwd_only"          # cwd_only | allow_list | unrestricted
    allow_list: []
  tool_whitelist:
    mode: "builtin"           # builtin | custom | all
    blocked: []               # 额外阻断的工具
  resource_limits:
    max_file_size_mb: 10
    max_bash_output_lines: 1000
    
adjudication:
  policies:
    - name: "no_destructive_commands"
      enabled: true
      rules:
        - pattern: "rm -rf"
          action: "block"
        - pattern: "DROP TABLE"
          action: "block_and_warn"
    - name: "require_approval_for_write"
      enabled: false
      rules:
        - tools: ["write", "edit", "bash"]
          action: "ask_user"
          
audit:
  event_levels:
    critical: ["TurnStart", "TurnEnd", "ToolCallEnd", "DecisionMade", "Error"]
    standard: ["ContentDelta", "ToolCallStart", "CompactionComplete"]
    verbose: ["ToolCallProgress"]
  retention:
    critical_events: "permanent"
    standard_events: "90_days"
    verbose_events: "7_days"
```

```rust
// 在 uncode-agent 中加载
pub struct GuardrailConfig {
    pub decision: DecisionConfig,
    pub firewall: FirewallConfig,
    pub adjudication: AdjudicationConfig,
    pub audit: AuditConfig,
}

impl GuardrailConfig {
    pub fn load() -> Result<Self> {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("uncode")
            .join("guardrails.yaml");
        // 如果文件不存在，使用默认值
        // ...
    }
}
```

### 3.2 事件分级与导出

```rust
// 在 AgentEvent 上增加 detail_level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventDetailLevel {
    Critical,   // 必须记录
    Standard,   // 默认记录
    Verbose,    // 仅调试时记录
}

impl AgentEvent {
    pub fn detail_level(&self) -> EventDetailLevel {
        match self {
            Self::TurnStart | Self::TurnEnd | Self::ToolCallEnd | 
            Self::DecisionMade { .. } | Self::Error { .. } => EventDetailLevel::Critical,
            Self::ContentDelta { .. } | Self::ToolCallStart | 
            Self::CompactionComplete => EventDetailLevel::Standard,
            Self::ToolCallProgress { .. } => EventDetailLevel::Verbose,
            _ => EventDetailLevel::Standard,
        }
    }
}

// JSONL 导出时的过滤
pub async fn export_session(
    store: &SessionStore, 
    session_id: &str, 
    min_level: EventDetailLevel,
) -> Result<Vec<AgentEvent>> {
    let events = store.load_events(session_id).await?;
    Ok(events.into_iter()
        .filter(|e| e.detail_level() <= min_level)  // Critical < Standard < Verbose
        .collect())
}
```

### 3.3 AgentStep 训练模型

```rust
// uncode-core 中新增
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub step_id: Uuid,
    pub turn_id: TurnId,
    pub state_before: AgentStateSnapshot,
    pub action: ExecutedAction,
    pub observation: ActionObservation,
    pub feedback: Option<Feedback>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateSnapshot {
    pub phase: String,
    pub turn_number: u32,
    pub active_tools: Vec<String>,
    pub context_size_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionObservation {
    pub success: bool,
    pub output_summary: String,
    pub files_changed: Vec<String>,
    pub duration_ms: u64,
    pub terminate: bool,  // 是否触发了 terminate AND 语义
}
```

### 第三阶段产物检查清单

- [ ] `guardrails.yaml` 格式定义 + 默认值
- [ ] `GuardrailConfig::load()` 从文件或默认值加载
- [ ] `EventDetailLevel` 在 `AgentEvent` 上实现
- [ ] JSONL 导出支持 `--detail-level` 参数
- [ ] `AgentStep` 类型定义在 `uncode-core` 中
- [ ] 审计器在每次 turn 结束时自动生成 `AgentStep`

---

## 第四阶段：验证、文档和品牌对齐

### 4.1 文档体系

```
docs/uncode-technologies/
├── UNCODE_COGNITION_LAYER.md        # 认知层设计文档
├── UNCODE_DECISION_LAYER.md         # 决策层设计文档（含四阶段说明）
├── UNCODE_SEMANTIC_FIREWALL.md      # 语义防火墙设计文档
└── UNCODE_GOVERNANCE_LAYER.md       # 治理层（7 范式在 uncode 中的映射）

docs/ai-agent-archi/
├── cognition-decision-driven-design.md  # 范式定义（已有）
└── uncodenow-refactoring-roadmap.md     # 本文档
```

**关键**：每个文档的引言段应明确引用"认知与决策驱动设计"范式，并标注本篇是该范式在 uncode 中的实现层说明。

### 4.2 架构图

在 `UNCODE_DECISION_LAYER.md` 中提供一张 ASCII 架构图，清晰展示：

```
                    ┌──────────────┐
                    │  uncode-ai   │  ← 认知层基础设施
                    │  (4 协议)    │
                    └──────┬───────┘
                           │ StreamEvent
                           ▼
┌──────────────────────────────────────────────────┐
│              uncode-agent                        │
│                                                  │
│  ┌─────────────────┐    ┌─────────────────────┐ │
│  │  cognition/      │    │  decision/           │ │
│  │  context_builder │    │  firewall (语义防火墙) │ │
│  │  prompt_manager  │ ←→ │  adjudication (裁决)  │ │
│  │  uncertainty     │    │  execution (执行)     │ │
│  └─────────────────┘    │  audit (审计)          │ │
│                          └─────────────────────┘ │
│                                                  │
│  治理层：                                         │
│  ┌──────────┬──────────┬──────────┬───────────┐ │
│  │ 事件驱动  │ 事件溯源  │ 约束设计  │ 状态机    │ │
│  └──────────┴──────────┴──────────┴───────────┘ │
└──────────────────────────────────────────────────┘
```

### 4.3 命名对齐

| 旧名称/概念 | 新名称（范式对齐后） | 说明 |
|:---|:---|:---|
| `AgentHarness` | 保留（Harness Engineering 术语） | 在文档中标注"Harness = 决策层编排器" |
| `loop_engine.rs` | `decision/execution.rs` 中的 `ExecutionOrchestrator` | 原逻辑迁移，保留 `LoopEngine` 为入口 Facade |
| `event.rs` 中的 Error | `cognition/uncertainty.rs` 中的 `UncertaintyClass` | 错误建模从"来源"改为"性质" |
| 分散的验证逻辑 | `decision/firewall.rs` 中的 `ValidationRule` trait 实现 | 收口到防火墙 |
| 分散的护栏逻辑 | `decision/adjudication.rs` 中的 `DecisionPolicy` trait 实现 | 收口到裁决器 |

### 第四阶段产物检查清单

- [ ] 新增 4 篇 `UNCODE_*` 设计文档，每篇引用了范式定义
- [ ] 架构图出现在 `UNCODE_DECISION_LAYER.md` 和 `README.md` 中
- [ ] 术语对齐表出现在 `AGENTS.md` 或项目 README 中
- [ ] `uncode-agent/src/` 的模块注释说明认知层/决策层/治理层的划分

---

## 五、实施优先级与风险

### 绝对不能跳过的（P0）

1. **语义防火墙独立为 `decision/firewall.rs`** — 这是范式最核心的概念，也是当前最大的架构缺口
2. **裁决器独立为 `decision/adjudication.rs`** — 这是"决策作为第一公民"的工程体现
3. **`DecisionMade` 事件扩展** — 决策即事件的原则需要显式事件类型支撑

### 高价值低风险的（P1）

4. 不确定性三分类显式建模
5. 声明式 `guardrails.yaml`
6. 事件分级 + `AgentStep` 模型

### 锦上添花的（P2）

7. `cognition/` 模块的形式化（`prompt_manager`, `memory`)
8. CQRS 显式建模
9. 工作流编排声明式 DSL
10. WASM 扩展宿主

### 风险提示

- **第一阶段从提取开始**：`decision/firewall.rs` 中的 `ValidationRule` 实现应**包装**现有 `PermissionPolicy`、`PermissionGate`、`resolve_path()`、`fetch_url()` 逻辑，不是重写。每个 trait 实现写完后立即运行 `cargo test -p uncode-agent` 验证。
- **`loop_engine.rs` 重组的风险**：1616 行的 `run_inner()` 是项目最复杂的函数。提取采用"方法委托"模式——先在 `loop_engine.rs` 中创建私有方法（如 `accumulate_proposals()`），验证测试通过后，再将方法移入 `decision/proposal.rs`。每次移动一个方法，每次一个 commit。
- **不要过度设计 trait 抽象**：`DecisionPolicy` 从现有的 4 个检查点（Phase 守卫、MAX_TURNS、CancellationToken、active_run CAS）反推 trait 签名，不是预先设计万能引擎。
- **事件扩展需 `#[non_exhaustive]`**：`AgentEvent` 当前未标记 `#[non_exhaustive]`。新增 `DecisionMade` 和 `UncertaintyEncountered` 变体前，需要先加上该属性以保证下游 match 分支不会被破坏。TUI 的 `AgentEvent` match（`lib.rs:2122+`行）需要作为回归重点。
- **已有基础设施降低风险**：`PermissionPolicy`、`PermissionGate`、`ChainedToolHooks`、`ToolRegistry`、`SessionStore` 均已稳定，防火墙和裁决器的 trait 实现可以利用这些现有抽象，减少新代码量。

---

## 六、验证标准：如何判断"最佳实践"已经达成

重构完成后，以下场景应能自然成立：

1. **新开发者能在 5 分钟内画出架构图**：打开 `uncode-agent/src/`，看到 `cognition/` 和 `decision/` 两个目录，立即理解"认知生成可能性，决策约束可能性"。
2. **语义防火墙可独立测试**：`cargo test -p uncode-agent decision::firewall` 覆盖 Parsing/Validation/Normalization 的所有 `ValidationRule` 实现。
3. **裁决策略可单独替换**：新增一个 `DecisionPolicy` 不影响其他策略，不需要修改 `AgentHarness`。
4. **护栏配置可通过文件修改**：`guardrails.yaml` 中改 `turn_limit: 30` 立即可生效，不需要重新编译。
5. **训练数据可导出**：`uncode-cli export --session-id xxx --format agentstep --detail-level critical` 输出可直接用于 RL 微调的 trajectory 数据。
6. **范式文档与实现同步**：`UNCODE_DECISION_LAYER.md` 中的架构图对应到实际的 `decision/` 目录结构，新开发者点进模块注释中的 `/// 参见 docs/ai-agent-archi/cognition-decision-driven-design.md §3.2` 能找到对应的范式定义。

---

*本路线图随 uncode 重构进展和范式定义迭代应定期修订。每个阶段完成后，在阶段标题旁标注完成日期和 commit hash。*
