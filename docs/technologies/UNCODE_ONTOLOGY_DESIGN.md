# uncode-ontology 详细技术方案

> **定位**：对 `UNCODE_REFACTORING_PLAN.md` Phase 1 的展开，聚焦本体 crate 的完整设计。
> **基准**：当前工具系统代码分析 + `docs/agent-archi/06-ontology.md` 范式定义 + Palantir 动态本体设计模式

---

## 一、问题定义：本体解决什么

### 1.1 当前代码中的"本体债务"

当前 9 个工具的以下知识分散在各处，没有统一建模：

| 知识 | 当前位置 | 问题 |
|:---|:---|:---|
| 工具参数 Schema | 每个工具的 `definition()` 方法 | 手写 JSON，LLM 输出和 Schema 无关联 |
| 权限分类 | `tool_permission.rs` 硬编码 match 工具名 | 无法扩展，扩展工具权限需改核心代码 |
| 路径字段识别 | `prepare_arguments` 中隐式约定 `"path"` / `"workdir"` | 本体不知道哪些字段是路径 |
| 副作用分类 | 无声明 | Adjudicator 无法区分只读/修改操作 |
| 大小限制 | 分散在各工具的常量中 | 无统一大小策略 |
| 字段名别名 | 无 | LLM 输出 `"filepath"` / `"file_path"` 无法归一化为 `"path"` |
| 默认值 | 各工具 Executor 内硬编码 | Normalizer 无法自动填充 |

### 1.2 本体的四重使命（对齐 `06-ontology.md`）

```
1. 类型注册表  →  回答"系统中有哪些事物可以操作"
2. 约束公理    →  回答"哪些操作是合法的"
3. Action 元数据 →  回答"每个工具做什么、需要什么"
4. 映射表      →  回答"LLM 的输出如何映射到规范形式"
```

---

## 二、核心类型设计

### 2.1 Crate 结构

```
crates/uncode-ontology/
├── Cargo.toml              # [dependencies] serde, serde_json, uuid, chrono
└── src/
    ├── lib.rs              # 统一导出 + Ontology struct
    ├── types.rs            # TypeId, EntityDef, ValueDef, ActionDef, LinkDef
    ├── fields.rs           # FieldDef, JsonSchema
    ├── constraints.rs      # Constraint, ConstraintLevel, ConstraintResult
    ├── effects.rs          # Effect, EffectCategory
    ├── mapping.rs          # FieldMapping, alias resolution, defaults
    ├── version.rs          # OntologyVersion, EvolutionLog, VersionMigration
    ├── builtin.rs          # coding_agent_ontology() — uncode 领域本体
    ├── evaluate.rs         # Constraint 求值引擎
    └── serde_helpers.rs    # JSON Schema 构造辅助
```

### 2.2 `TypeId` — 一切类型的基石

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeId(pub String);
```

**设计决定**：用 `String` 而非整数 ID。理由：
- 可读性：`TypeId("File")` 比 `TypeId(42)` 更利于调试和 LLM 上下文
- 去中心化：扩展可自行声明 TypeId，无冲突风险（由注册时的唯一性检查保证）
- 序列化：天然可读的 JSON key

**内置元类型**：
```rust
impl TypeId {
    pub const STRING:  Self = TypeId(String::from("string"));
    pub const NUMBER:  Self = TypeId(String::from("number"));
    pub const BOOLEAN: Self = TypeId(String::from("boolean"));
    pub const UNIT:    Self = TypeId(String::from("unit"));
    pub const ANY:     Self = TypeId(String::from("any"));
}
```

### 2.3 `EntityDef` — 实体类型（≈ Palantir Object Type）

```rust
pub struct EntityDef {
    pub id: TypeId,
    pub fields: Vec<FieldDef>,
    pub invariants: Vec<Constraint>,
    pub extends: Option<TypeId>,
    pub description: Option<String>,
}
```

**案例**：`File` 实体
```rust
EntityDef {
    id: TypeId("File"),
    fields: vec![
        FieldDef { name: "path",    value_type: TypeId::STRING, required: true,  default: None },
        FieldDef { name: "content", value_type: TypeId::STRING, required: false, default: None },
        FieldDef { name: "exists",  value_type: TypeId::BOOLEAN,required: false, default: Some(json!(false)) },
    ],
    invariants: vec![
        Constraint::RequiredField { field: "path" },
    ],
    extends: None,
    description: Some("Filesystem file"),
}
```

### 2.4 `ActionDef` — 动作类型（≈ Palantir Action Type）

```rust
pub struct ActionDef {
    pub name: String,
    pub input_schema: JsonSchema,
    pub output_type: TypeId,
    pub preconditions: Vec<Constraint>,
    pub effects: Vec<Effect>,
    pub execution_category: ExecutionCategory,
    pub description: Option<String>,
}

pub enum ExecutionCategory {
    ReadOnly,
    Destructive,
    Network,
    Shell,
    Unknown,
}
```

**`ExecutionCategory` 替代硬编码权限分类**：当前 `tool_permission.rs` 中按工具名硬编码的只读/破坏性/网络/Shell 四类，在本体中成为 `ActionDef` 的显式字段。

### 2.5 `FieldDef` — 字段定义

```rust
pub struct FieldDef {
    pub name: String,
    pub value_type: TypeId,
    pub required: bool,
    pub default: Option<serde_json::Value>,
    pub aliases: Vec<String>,        // "filepath", "file_path" → canonical "path"
    pub description: Option<String>,
    pub constraints: Vec<Constraint>, // 字段级约束
}
```

**关键设计：`aliases` 字段**。当前 `DefaultNormalizer` 是空操作，因为不知道 LLM 可能输出哪些字段名变体。`aliases` 使 Normalizer 可以声明式地处理字段名规约。

### 2.6 `LinkDef` — 关系类型（新增，Palantir 模式）

当前 `06-ontology.md` 和代码中均缺失。Palantir 的 "objects + properties + **links**" 中，links 是最关键的建模能力。

```rust
pub struct LinkDef {
    pub id: TypeId,
    pub source_type: TypeId,
    pub target_type: TypeId,
    pub cardinality: Cardinality,
    pub inverse: Option<TypeId>,       // 逆向关系 ID
    pub description: Option<String>,
}

pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToMany,
}
```

**案例**：工具间的依赖关系
```rust
LinkDef {
    id: TypeId("uses_hashline"),
    source_type: TypeId("edit"),
    target_type: TypeId("read"),
    cardinality: Cardinality::ManyToMany,
    inverse: Some(TypeId("provides_hashline")),
    description: Some("edit 的 hashline 模式依赖 read 的 hashline 输出"),
}
```

### 2.7 `Constraint` — 约束公理

```rust
pub enum ConstraintLevel {
    Hard,  // 违反 → 拒绝
    Soft,  // 违反 → 告警但放行
}

pub enum Constraint {
    TypeCheck      { field: String, expected: TypeId, level: ConstraintLevel },
    RangeCheck     { field: String, min: Option<f64>, max: Option<f64>, level: ConstraintLevel },
    RequiredField  { field: String },
    EnumCheck      { field: String, allowed: Vec<String>, level: ConstraintLevel },
    Referential    { field: String, target_type: TypeId },
    RegexMatch     { field: String, pattern: String, description: String, level: ConstraintLevel },
    CustomRule     { name: String, description: String, level: ConstraintLevel },
}
```

**求值引擎**（`evaluate.rs`）：
```rust
pub struct ConstraintEvaluator {
    registry: Arc<TypeRegistry>,
}

impl ConstraintEvaluator {
    pub fn evaluate(
        &self,
        constraint: &Constraint,
        field_values: &HashMap<String, serde_json::Value>,
    ) -> ConstraintResult { /* ... */ }
}

pub enum ConstraintResult {
    Pass,
    Warn { constraint: String, field: String, detail: String },
    Fail { constraint: String, field: String, detail: String },
}
```

### 2.8 `Effect` — 副作用声明

```rust
pub enum Effect {
    Read    { target: String, fields: Vec<String> },
    Create  { entity: TypeId },
    Modify  { entity: TypeId, fields: Vec<String> },
    Delete  { entity: TypeId },
    Exec    { command: String },
    Network { destination: String },
}
```

`Effect` 在裁决器中启用"效应检查"——如果一个 Action 的 effects 都是 `Read`，裁决器可以自动放行（对应 `auto_allow_readonly`）。

---

## 三、与工具系统的集成

### 3.1 当前工具注册流程 vs 本体驱动流程

```
── 当前 ──                           ── 本体驱动 ──
#[tool] 宏生成 ToolDefinition      ActionDef.to_tool_definition()
       │                                      │
       ▼                                      ▼
ToolRegistry.register(name, exec)    TypeRegistry.register_action(def)
       │                                      │
       ▼                                      ▼
      执行                              从 TypeRegistry 生成 ToolDefinition
                                                 │
                                         同时注册到 ToolRegistry
```

### 3.2 `ActionDef → ToolDefinition` 生成

```rust
impl ActionDef {
    pub fn to_tool_definition(&self, label: Option<String>) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.build_llm_description(),
            parameters: self.input_schema.to_json_schema_object(),
            label,
            execution_mode: match self.execution_category {
                ExecutionCategory::Shell => ExecutionMode::Sequential,
                _ => ExecutionMode::Parallel,
            },
        }
    }

    fn build_llm_description(&self) -> String {
        let mut desc = self.description.clone().unwrap_or_default();
        if !self.effects.is_empty() {
            desc.push_str(" Effects: ");
            for e in &self.effects {
                desc.push_str(&format!("[{}] ", e));
            }
        }
        desc
    }
}
```

### 3.3 Normalizer 驱动

当前 `DefaultNormalizer`（`firewall.rs:146-153`）：
```rust
// 当前：空操作
fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError> {
    Ok(NormalizedAction {
        tool_name: action.tool_name.clone(),
        arguments: action.arguments.clone(),
        normalized_fields: vec![], // ← 永远是空的
    })
}
```

**本体驱动的 `DeclarativeNormalizer`**：
```rust
pub struct DeclarativeNormalizer {
    mapping: FieldMapping,
    evaluator: ConstraintEvaluator,
}

impl NormalizeStrategy for DeclarativeNormalizer {
    fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError> {
        let mut args = action.arguments.clone();
        let mut log = Vec::new();

        // 1. 字段名规约：别名 → 规范名
        if let Some(action_def) = self.mapping.registry.get_action(&TypeId(action.tool_name.clone())) {
            for field_def in &action_def.fields() {
                for alias in &field_def.aliases {
                    if let Some(val) = args.get(alias) {
                        if alias != &field_def.name {
                            args[&field_def.name] = val.clone();
                            log.push(format!("{alias} → {}", field_def.name));
                        }
                    }
                }
            }
        }

        // 2. 默认值填充
        if let Some(defaults) = self.mapping.defaults.get(&action.tool_name) {
            for (field, default) in defaults {
                if args.get(field).is_none() {
                    args[field] = default.clone();
                    log.push(format!("{field} = default"));
                }
            }
        }

        // 3. 引用解析：外部引用 → 内部 TypeId
        for (field, value) in args.clone() {
            if let Some(resolved) = self.mapping.resolve_external(&value.to_string()) {
                args[&field] = json!(resolved.0);
                log.push(format!("{value} → {}", resolved.0));
            }
        }

        Ok(NormalizedAction {
            tool_name: action.tool_name.clone(),
            arguments: args,
            normalized_fields: log, // ← 现在非空！
        })
    }
}
```

**配置来源**：从 `TypeRegistry` 中提取
```rust
impl DeclarativeNormalizer {
    pub fn from_registry(registry: &TypeRegistry) -> Self {
        let mut field_aliases = HashMap::new();
        let mut defaults = HashMap::new();
        for (type_id, action) in &registry.actions {
            for field in &action.fields() {
                for alias in &field.aliases {
                    field_aliases.insert((action.name.clone(), alias.clone()), field.name.clone());
                }
                if let Some(default) = &field.default {
                    defaults
                        .entry(action.name.clone())
                        .or_insert_with(HashMap::new)
                        .insert(field.name.clone(), default.clone());
                }
            }
        }
        Self {
            mapping: FieldMapping { field_aliases, defaults, /* ... */ },
            evaluator: ConstraintEvaluator::new(Arc::new(registry.clone())),
        }
    }
}
```

### 3.4 Firewall Validator 驱动

当前 `ValidationRule` 实现（`firewall.rs`）有两个方式：实现 `ValidationRule` trait 的手写规则。本体驱动后，新增：

```rust
/// 从本体 Constraint 列表生成验证规则
pub struct OntologyConstraintRule {
    evaluator: ConstraintEvaluator,
    constraints: Vec<Constraint>,
}

impl ValidationRule for OntologyConstraintRule {
    fn validate(&self, parsed: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        let field_values = parsed.arguments.as_object()
            .map(|m| m.iter().map(|(k,v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let mut violations = Vec::new();
        for constraint in &self.constraints {
            match self.evaluator.evaluate(constraint, &field_values) {
                ConstraintResult::Fail { detail, .. } => {
                    violations.push(detail);
                }
                ConstraintResult::Warn { detail, .. } => {
                    tracing::warn!("ontology soft constraint: {detail}");
                }
                ConstraintResult::Pass => {}
            }
        }

        if violations.is_empty() {
            Ok(ValidationVerdict { approved: true, violations: vec![] })
        } else {
            Err(ValidationError::Blocked { reasons: violations })
        }
    }
}
```

### 3.5 裁决器驱动

当前 4 个 `DecisionPolicy`（PhaseGuard / TurnLimit / Cancellation / Concurrency）是独立的。本体驱动后新增：

```rust
/// 基于 Effect 的裁决策略：ReadOnly → 自动放行
pub struct EffectBasedPolicy {
    registry: Arc<TypeRegistry>,
}

impl DecisionPolicy for EffectBasedPolicy {
    fn evaluate(
        &self, _ctx: &DecisionContext, action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        if let Some(action_def) = self.registry.get_action(&TypeId(action.tool_name.clone())) {
            if action_def.effects.iter().all(|e| matches!(e, Effect::Read { .. })) {
                return Ok(DecisionVerdict::approved());
            }
        }
        Ok(DecisionVerdict::approved()) // 非只读不在此策略中拒绝
    }

    fn name(&self) -> &'static str { "effect_based_policy" }
}
```

---

## 四、Ground Truth / Operational Truth 架构

### 4.1 二元真相模型

```
                Operational Truth
                (LLM 概率输出)
                      │
                      ▼
              ┌───────────────┐
              │  语义防火墙     │
              │               │
              │  Parser  ─────┤  ← 本体类型定义
              │  Validator ───┤  ← 本体约束公理
              │  Normalizer ──┤  ← 本体映射表
              │               │
              └───────┬───────┘
                      │
                      ▼
                Ground Truth
                (确定性结构)
                      │
                      ▼
                决策层执行
                      │
                      ▼
              执行结果事件流
          (Ground Truth 写回 Operational 层)
```

### 4.2 Rust 类型表示

```rust
/// Operational Truth — LLM 的输出，概率性的、暂定的
#[derive(Debug, Clone)]
pub struct OperationalTruth {
    pub raw_output: String,
    pub confidence: f32,
    pub alternatives: Vec<OperationalTruth>,
    pub source: TruthSource,
}

/// Ground Truth — 经过本体验证的确定性结构
#[derive(Debug, Clone)]
pub struct GroundTruth {
    pub validated: ValidatedAction,
    pub constraint_results: Vec<ConstraintResult>,
    pub normalization_log: Vec<String>,
    pub ontology_version: OntologyVersion,
}

/// 写回机制：执行结果验证/修正 Operational 层的假设
#[derive(Debug, Clone)]
pub struct TruthFeedback {
    pub expected_effect: Vec<Effect>,
    pub actual_outcome: ExecutionResult,
    pub ground_truth_updated: bool,
}
```

**核心方法**：
```rust
impl SemanticFirewall {
    /// 语义防火墙的核心语义：
    /// 将 Operational Truth（概率输出）转化为 Ground Truth（确定性结构）
    pub fn operational_to_ground(
        &self,
        operational: &OperationalTruth,
    ) -> Result<GroundTruth, FirewallError> {
        // 1. Parsing：文本 → 结构（使用本体类型定义）
        let proposal = ActionProposal {
            tool_name: self.classify_intent(operational)?,
            raw_arguments: json!(operational.raw_output),
            confidence: Some(operational.confidence),
            /* ... */
        };
        // 2. Parsing：提取结构化字段
        let parsed = self.parser.parse(&proposal)?;
        // 3. Validation：本体约束检查
        let validated = self.validate_all(&parsed)?;
        // 4. Normalization：字段规约 + 默认值填充
        let normalized = self.normalizer.normalize(&validated)?;
        Ok(GroundTruth { /* ... */ })
    }
}
```

---

## 五、uncode 领域本体：9 个工具的完整编码

### 5.1 工具本体的分层结构

```
TypeRegistry 中 actions 的编码 = 对 9 个 builtin tools 的本体描述

每个 ActionDef 包含：
├── name:              工具名（与 ToolRegistry 中的注册名一致）
├── input_schema:      参数 JSON Schema（对齐现有手写 Schema）
├── output_type:       输出类型（STRING / JSON / UNIT）
├── preconditions:     前置条件约束
│   ├── RequiredField  必填字段检查
│   ├── TypeCheck      类型检查
│   └── CustomRule     领域规则（如 "file must exist"）
├── effects:           副作用声明
│   ├── Read           只读
│   ├── Modify         修改
│   ├── Exec           执行外部命令
│   └── Network        网络访问
└── execution_category: 执行分类（ReadOnly / Destructive / Shell / Network）
```

### 5.2 9 个工具的 ActionDef 编码

**Read（只读文件）**：
```rust
ActionDef {
    name: "read",
    input_schema: json_schema!({
        "path":    { type: string,  required },
        "offset":  { type: integer, optional, default: 0 },
        "limit":   { type: integer, optional },
        "hashline":{ type: boolean, optional, default: false },
    }),
    output_type: TypeId::STRING,
    preconditions: vec![
        RequiredField("path"),
        CustomRule("file exists", "path must refer to existing file", Hard),
    ],
    effects: vec![Read { target: "File", fields: vec!["content"] }],
    execution_category: ReadOnly,
}
```

**Write（覆盖写入文件）**：
```rust
ActionDef {
    name: "write",
    input_schema: json_schema!({
        "path":    { type: string, required },
        "content": { type: string, required, max_bytes: 10MB },
    }),
    output_type: TypeId::STRING, // diff output
    preconditions: vec![
        RequiredField("path"),
        RequiredField("content"),
    ],
    effects: vec![Modify { entity: TypeId("File"), fields: vec!["content"] }],
    execution_category: Destructive,
}
```

**Grep（内容搜索）**：
```rust
ActionDef {
    name: "grep",
    input_schema: json_schema!({
        "pattern": { type: string, required },
        "path":    { type: string, optional, default: ".", path: true },
        "include": { type: string, optional },
    }),
    output_type: TypeId::STRING,
    preconditions: vec![RequiredField("pattern")],
    effects: vec![Read { target: "Workspace", fields: vec!["files"] }],
    execution_category: ReadOnly,
}
```

**Bash（执行命令）**：
```rust
ActionDef {
    name: "bash",
    input_schema: json_schema!({
        "command":     { type: string, required },
        "description": { type: string, optional },
        "workdir":     { type: string, optional, default: ".", path: true },
        "timeout":     { type: integer, optional, default: 120, min: 1, max: 86400 },
    }),
    output_type: TypeId::STRING,
    preconditions: vec![
        RequiredField("command"),
        CustomRule("no destructive", "禁止 rm -rf/DELETE/DROP 等破坏性命令", Hard),
    ],
    effects: vec![Exec { command: "[dynamic]" }],
    execution_category: Shell,
}
```

（其余 5 个工具同理编码：edit、find、ls、web_fetch、web_search）

### 5.3 `PathField` — 路径字段的统一建模

当前代码中"path 是路径"的知识隐式存在于 `prepare_arguments_path()` 调用中。本体将其显式化：

```rust
/// 标记哪些参数是文件系统路径
#[derive(Debug, Clone)]
pub struct PathField {
    pub field_name: String,
    pub path_type: PathType,
    pub default: Option<String>,       // "." 表示默认当前目录
    pub must_exist: bool,              // 文件必须存在（read/edit/write）
}

pub enum PathType {
    FilePath,       // 文件路径（read, write, edit）
    DirectoryPath,  // 目录路径（grep, find, ls）
    WorkDir,        // 工作目录（bash）
}
```

这使得 `resolve_path()` 可以从本体的 `PathField` 声明自动生成，而非在每个工具的 `execute()` 中手动调用。

---

## 六、LinkDef：关系建模

### 6.1 工具间的依赖关系

```
read  ──[provides_hashline]──→  edit
 │                                │
 │     edit 的 hashline 模式       │
 │     依赖 read 的 hashline 输出  │
 └────────────────────────────────┘

grep  ──[may_produce_paths]──→  read/edit
 │                                  │
 │     grep 输出文件路径，          │
 │     LLM 可能用这些路径           │
 │     调用 read/edit               │
 └──────────────────────────────────┘
```

```rust
// 工具依赖关系
LinkDef {
    id: TypeId("provides_hashline"),
    source_type: TypeId("read"),
    target_type: TypeId("edit"),
    cardinality: OneToMany,
    inverse: Some(TypeId("uses_hashline")),
    description: Some("read 的 hashline 输出是 edit hashline 模式的输入"),
}

// 文件-工具关系
LinkDef {
    id: TypeId("reads_file"),
    source_type: TypeId("read"),
    target_type: TypeId("File"),
    cardinality: ManyToMany,
    inverse: Some(TypeId("read_by")),
    description: Some("read 工具操作 File 实体"),
}
```

### 6.2 Link 的用途

1. **影响分析**：修改 read 工具的 Schema 时，自动报告 edit 工具受影响（通过 `provides_hashline` 链接）
2. **权限推导**：如果一个文件被 `deleted_by` 工具链接，该工具自动归为 Destructive 类别
3. **链式验证**：edit 的 `pos` 字段格式（hashline）是否与 read 的 `hashline` 输出格式一致

---

## 七、动态本体演化

### 7.1 版本化

```rust
pub struct OntologyVersion {
    pub major: u32,  // 不兼容变更
    pub minor: u32,  // 向前兼容新增
    pub patch: u32,  // 修复/优化
}

pub struct OntologySnapshot {
    pub version: OntologyVersion,
    pub registry: TypeRegistry,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub parent_version: Option<OntologyVersion>,
}
```

### 7.2 向前兼容规则

| 变更 | 兼容性 | 示例 |
|:---|:---|:---|
| 新增 EntityDef / ActionDef | ✅ 向前兼容 | 新增 `git_commit` 工具 |
| 新增可选字段 | ✅ 向前兼容 | read 新增 `encoding` 可选字段 |
| 新增必填字段 | ❌ 不兼容 | edit 新增必填 `author` 字段 |
| 修改字段类型 | ❌ 不兼容 | `timeout` 从 integer → string |
| 删除字段 | ⚠️ 需迁移 | 移除已弃用的 `old_string` 参数 |
| 修改 Constraint | ⚠️ 视情况 | Hard → Soft ✅ / Soft → Hard ❌ |

### 7.3 演化机制

```rust
pub struct EvolutionEngine {
    history: EvolutionLog,
    current: OntologyVersion,
}

impl EvolutionEngine {
    /// 提议本体变更（来自决策层反馈）
    pub fn propose_mutation(
        &self,
        reason: EvolutionReason,
        mutation: OntologyMutation,
    ) -> OntologyVersion { /* ... */ }

    /// 检查向前兼容性
    pub fn check_compatibility(
        &self,
        from: &OntologyVersion,
        to: &OntologyVersion,
    ) -> CompatibilityReport { /* ... */ }

    /// 迁移历史数据到新版本
    pub fn migrate(
        &self,
        from: &OntologyVersion,
        to: &OntologyVersion,
    ) -> Result<(), MigrationError> { /* ... */ }
}

pub enum EvolutionReason {
    ToolAdded { name: String },
    ToolDeprecated { name: String },
    SchemaChanged { tool: String, field: String, change: String },
    ConstraintRefined { constraint: String },
    FeedbackFromDecision { decision_id: String, insight: String },
}
```

---

## 八、实现路线图

### Phase 1a：crate 基础（1 天）

- 创建 `crates/uncode-ontology/Cargo.toml`
- 实现 `types.rs`（TypeId, EntityDef, ValueDef, ActionDef, LinkDef, FieldDef）
- 实现 `constraints.rs`（Constraint, ConstraintLevel, ConstraintResult）
- 实现 `effects.rs`（Effect）
- 实现 `lib.rs`（Ontology struct, 注册/查询方法）
- 单元测试：TypeRegistry 注册/查询

### Phase 1b：工具本体编码（1 天）

- 实现 `builtin.rs`（`coding_agent_ontology()`）
- 为 9 个 builtin tools 各编写完整的 `ActionDef`
- 为 `File`、`Workspace`、`Module` 编写 `EntityDef`
- 验证：从 `ActionDef` 生成的 `ToolDefinition` 与当前手写版本语义等价

### Phase 1c：Normalizer 集成（1 天）

- 实现 `mapping.rs`（FieldMapping, `DeclarativeNormalizer`）
- 在 `firewall.rs` 中替换 `DefaultNormalizer` → `DeclarativeNormalizer`
- 测试：验证 `normalized_fields` 非空

### Phase 1d：Firewall 集成（1 天）

- 实现 `evaluate.rs`（`ConstraintEvaluator`）
- 实现 `OntologyConstraintRule`（包装 `Constraint` 为 `ValidationRule`）
- 集成到 `build_default_firewall()` 中

### Phase 1e：裁决器集成（0.5 天）

- 实现 `EffectBasedPolicy`（基于 Effect 的自动放行）
- 集成到 `Adjudicator` 策略链

### Phase 1f：#[tool] 宏扩展（1 天，可选）

- 扩展 proc-macro 支持 `#[ontology(action = "...", effects = [...])]`
- 生成 `__ontology_action_*()` 函数返回 `ActionDef`

---

## 九、验证标准

| 测试场景 | 预期结果 |
|:---|:---|
| 本体注册 9 个工具 | `TypeRegistry.actions.len() == 9` |
| `ActionDef::to_tool_definition()` | 生成的 `ToolDefinition` 与当前手写等价 |
| `DeclarativeNormalizer.normalize()` 遇 `filepath` → `path` | `normalized_fields` 包含 `"filepath → path"` |
| `DeclarativeNormalizer.normalize()` 遇缺失可选字段 | 自动填充默认值 |
| `OntologyConstraintRule` 遇 `rm -rf /` 作为 bash 命令 | `ValidationVerdict.approved == false` |
| `OntologyConstraintRule` 遇 `read` 无故 `content` 字段 | 硬约束拒绝 |
| `EffectBasedPolicy` 遇 `read` 工具的 proposal | 自动放行（只读效应） |
| `EffectBasedPolicy` 遇 `write` 工具的 proposal | 不放行（需走完整裁决链） |
| 本体版本升级：新增可选字段 | ✅ 向前兼容检查通过 |
| 本体版本升级：新增必填字段 | ❌ 向前兼容检查失败 |
| 影响分析：修改 `read` ActionDef | 自动报告 `edit`（通过 `provides_hashline` link）受影响 |

---

## 十、参考资料

| 资料 | 位置 | 用途 |
|:---|:---|:---|
| 旧版本体论基础 | `docs/references/ontology-foundations.md` | GT/OT 二元真相、Palantir 三层架构、本体四层模型（方案 §四 的理论来源） |
| 旧版本体-决策映射 | `docs/references/ontology-decision-mapping.md` | 本体在决策四阶段的角色分析、本体反馈演化机制（方案 §三 Validator/Normalizer/裁决器 的设计来源） |
| 旧版本体 vs DDD 对比 | `docs/references/ontology-vs-ddd.md` | 九维理论对比（战略/语义互补，方案中领域第一公民的理论正当性） |
| Microsoft Ontology Playground | `https://github.com/microsoft/Ontology-Playground` | 可视化本体编辑器 + RDF/XML 双向导出 + 自然语言→本体查询。**Entity/Property/Relationship 的 TypeScript 类型定义验证了我们方案中 EntityDef/FieldDef/LinkDef 的设计模式一致性**。NL2Ontology 查询引擎与我们的 `operational_to_ground()` 概念同构，可作为 Constraint 求值引擎 `evaluate.rs` 的概念验证。项目使用标准 OWL/RDF 格式——本方案未直接采用（对 Agent 运行时过重），但如需与外部本体互操作，其 RDF 序列化实现是参考模板。 |
