# uncode-ontology 详细技术方案

> **定位**：对 `UNCODE_REFACTORING_PLAN.md` Phase 1 的展开，聚焦本体 crate 的完整设计。
> **基准**：当前工具系统代码分析 + `docs/agent-archi/06-ontology.md` 范式定义 + Palantir 动态本体设计模式

---

## 一、问题定义：本体解决什么

### 1.1 当前代码中的"本体债务"

当前 9 个内置工具（后续新增 question/skill/task 三个工具未在本体建模，走 `ToolRegistry` 直接注册）的以下知识分散在各处，没有统一建模：

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
1. 类型注册表  →  回答"系统中有哪些事物可以操作"           ✅ 完成
2. 约束公理    →  回答"哪些操作是合法的"                   ✅ 完成 (RegexMatch接入regex, CustomRule回调)
3. Action 元数据 →  回答"每个工具做什么、需要什么"           ✅ 完成 (to_tool_definition桥接)
4. 映射表      →  回答"LLM 的输出如何映射到规范形式"         ✅ 完成 (DeclarativeNormalizer + PathField)
```

---

## 二、核心类型设计

### 2.1 Crate 结构

> **实现状态**：✅ 已完成。文件数从设计的 10 个合并为 6 个（`fields.rs`/`constraints.rs`/`effects.rs` 合并入 `types.rs`；`mapping.rs` 的逻辑迁入 `registry.rs`；`version.rs` 的 VersionMigration/EvolutionLog 未实现；`serde_helpers.rs` 因 `ActionDef.input_schema` 被 `fields: Vec<FieldDef>` 替代而取消）。新增 `reasoner.rs`（推理引擎）和更丰富的 `registry.rs`。

```
crates/uncode-ontology/
├── Cargo.toml              # [dependencies] serde, serde_json
└── src/
    ├── lib.rs              # 统一导出
    ├── types.rs            # 所有核心类型：TypeId, EntityDef, ActionDef, FieldDef, LinkDef,
    │                       #   Constraint, ConstraintResult, Effect, ReasoningRule, DerivationExpr,
    │                       #   OntologyVersion, EntityCategory, ExecutionCategory, Cardinality
    ├── registry.rs         # TypeRegistry：注册/查询/合并/字段别名/默认值汇总
    ├── builtin.rs          # 3 个 LazyLock 缓存的构建函数
    │                       #   coding_agent_ontology() / system_resource_ontology() / full_ontology()
    ├── evaluate.rs         # Constraint 求值（FieldLookup trait + evaluate_constraint 自由函数）
    └── reasoner.rs         # ReasoningRule 推理引擎（traversal + derivation + 求值）
```

### 2.2 `TypeId` — 一切类型的基石

> **实现状态**：✅ 已完成。

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeId(pub String);
```

**额外的 trait 实现**（设计文档未覆盖）：
```rust
impl Deref for TypeId { type Target = str; ... }       // 自动强制为 &str
impl From<String> for TypeId { ... }                    // 从 String 构造
impl From<&str> for TypeId { ... }                      // 从 &str 构造
```

`Deref<Target=str>` 使得 `TypeId` 可以在绝大多数需要 `&str` 的上下文中自动强制转换，避免手动 `.0` 访问。

**设计决定**：用 `String` 而非整数 ID。理由：
- 可读性：`TypeId("File")` 比 `TypeId(42)` 更利于调试和 LLM 上下文
- 去中心化：扩展可自行声明 TypeId，无冲突风险（由注册时的唯一性检查保证）
- 序列化：天然可读的 JSON key

**内置常量实现现状**（设计为 `const`，实现为 `fn`——返回新 `TypeId` 的方法）：
```rust
impl TypeId {
    pub fn string()  -> Self { TypeId("string".into()) }
    pub fn integer() -> Self { TypeId("integer".into()) }
    pub fn number()  -> Self { TypeId("number".into()) }
    pub fn boolean() -> Self { TypeId("boolean".into()) }
    pub fn unit()    -> Self { TypeId("unit".into()) }
    pub fn any()     -> Self { TypeId("any".into()) }
}
```

### 2.3 `EntityDef` — 实体类型（≈ Palantir Object Type）

> **实现状态**：✅ 已完成。`invariants` 和 `extends` 均已实现，`TypeRegistry::resolve_entity()` 负责继承链合并。

```rust
pub struct EntityDef {
    pub id: TypeId,
    pub fields: Vec<FieldDef>,
    pub category: EntityCategory,   // 🆕 设计未覆盖：区分 Domain / System
    pub description: Option<String>,
}

/// 🆕 实体分类：领域语义 vs 系统资源语义
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EntityCategory {
    Domain,   // File, Workspace, Module — 领域第一公民
    System,   // LLM, Provider, Capability — 系统基础设施
}
```

> `invariants` 和 `extends` 均已实现（P0-2、P0-3），`TypeRegistry::resolve_entity()` 负责继承链合并，`OntologyConstraintRule` 自动追加实体 invariants 到 Action 校验。

**案例**：`File` 实体
```rust
EntityDef {
    id: TypeId::from("File"),
    category: EntityCategory::Domain,
    fields: vec![
        FieldDef { name: "path",    value_type: TypeId::string(),  required: true,  default: None, aliases: vec!["filepath".into()], description: Some("Filesystem path".into()) },
        FieldDef { name: "content", value_type: TypeId::string(),  required: false, default: None, aliases: vec!["body".into()],    description: Some("File content".into()) },
    ],
    invariants: vec![Constraint::RequiredField { field: "path".into() }],
    extends: None,
    description: Some("Filesystem file".into()),
}
```

### 2.4 `ActionDef` — 动作类型（≈ Palantir Action Type）

> **实现状态**：✅ 已完成。`input_schema: JsonSchema` 被 `fields: Vec<FieldDef>` 替代。

```rust
pub struct ActionDef {
    pub name: String,
    pub fields: Vec<FieldDef>,                          // 🆕 替代原设计的 input_schema: JsonSchema
    pub output_type: TypeId,
    pub preconditions: Vec<Constraint>,
    pub effects: Vec<Effect>,
    pub execution_category: ExecutionCategory,
    pub category: EntityCategory,                       // 🆕 Domain / System 分类
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

**设计变更说明**：原设计用 `input_schema: JsonSchema`（预构建 JSON Schema 对象），实现改为 `fields: Vec<FieldDef>`。这使得 `to_tool_definition()`（将 ActionDef 转化为 ToolDefinition）需要通过 `FieldDef` 列表动态构造 JSON Schema，而非使用预构建 Schema。

> **实现方式**：通过 `ActionDef::to_json_schema()` 从 `fields: Vec<FieldDef>` 动态构造 JSON Schema（见 §3.2）。

**`ExecutionCategory` 替代硬编码权限分类**：当前 `tool_permission.rs` 中按工具名硬编码的只读/破坏性/网络/Shell 四类，在本体中成为 `ActionDef` 的显式字段。

### 2.5 `FieldDef` — 字段定义

> **实现状态**：✅ 已完成。`value_type` 已使用 `TypeId`（Phase 1 修补），字段级 `constraints` 未实现。

```rust
pub struct FieldDef {
    pub name: String,
    pub value_type: TypeId,     // ✅ P1 修补：从 String 升级为 TypeId
    pub required: bool,
    pub default: Option<serde_json::Value>,
    pub aliases: Vec<String>,   // "filepath", "file_path" → canonical "path"
    pub description: Option<String>,
}
```

> **TODO**：`constraints: Vec<Constraint>` 字段级约束未实现。目前约束只能在 `ActionDef.preconditions` 中声明，无法在字段级别声明类型检查/范围检查/枚举检查。

**关键设计：`aliases` 字段**。当前 `DefaultNormalizer` 是空操作，因为不知道 LLM 可能输出哪些字段名变体。`aliases` 使 Normalizer 可以声明式地处理字段名规约。

### 2.6 `LinkDef` — 关系类型（新增，Palantir 模式）

> **实现状态**：✅ 完全实现，与设计完全对齐。

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

> **实现状态**：✅ 已完成（7/7 变体）。

```rust
pub enum ConstraintLevel {
    Hard,  // 违反 → 拒绝（#[serde(default = "default_hard")]）
    Soft,  // 违反 → 告警但放行
}

pub enum Constraint {
    TypeCheck      { field: String, expected: String, level: ConstraintLevel },
    RangeCheck     { field: String, min: Option<f64>, max: Option<f64>, level: ConstraintLevel },
    RequiredField  { field: String },
    EnumCheck      { field: String, allowed: Vec<String>, level: ConstraintLevel },
    RegexMatch     { field: String, pattern: String, description: String, level: ConstraintLevel },
    CustomRule     { name: String, description: String, level: ConstraintLevel },
}
```

**Referential 求值现状**：已实现。检查字段值是否为非空字符串（非空即认为通过——未做运行时引用解析）。

**求值引擎**（`evaluate.rs`）——实现为**自由函数**而非设计中的 struct wrapper：

```rust
/// 泛型约束求值（任意实现 FieldLookup 的对象均可求值）
pub fn evaluate_constraint(
    constraint: &Constraint,
    fields: &impl FieldLookup,
) -> ConstraintResult { /* ... */ }

/// 抽象字段查找：统一 HashMap<String, Value> 和 serde_json::Map
pub trait FieldLookup {
    fn get_field(&self, name: &str) -> Option<&serde_json::Value>;
}

pub enum ConstraintResult {
    Pass,
    Warn { constraint: String, field: String, detail: String },
    Fail { constraint: String, field: String, detail: String },
}
```

**额外实现**：
```rust
impl ConstraintResult {
    pub fn is_pass(&self) -> bool { ... }
    pub fn severity(&self) -> Option<ConstraintLevel> { ... }
}
impl BitOr for ConstraintResult { ... }     // Pass|Warn=Warn, Warn|Fail=Fail
impl BitOrAssign for ConstraintResult { ... }
```

> **实现细节**：`RegexMatch` 已接入 `regex` crate，`evaluate_constraint_with_rules()` 支持 `CustomRuleFn` 回调映射表。两处均已在生产路径中运行。

### 2.8 `Effect` — 副作用声明

> **实现状态**：✅ 已完成。entity 字段为 `String` 而非设计中的 `TypeId`。额外添加了 `is_read_only()` 便利方法。

```rust
pub enum Effect {
    Read    { target: String, fields: Vec<String> },
    Create  { entity: String },           // ⚠️ 设计中为 TypeId，实现降级为 String
    Modify  { entity: String, fields: Vec<String> },
    Delete  { entity: String },
    Exec    { command: String },
    Network { destination: String },
}

impl Effect {
    pub fn is_read_only(&self) -> bool { matches!(self, Effect::Read { .. }) }
}
```

`Effect` 的 `is_read_only()` 和 `ActionDef::is_read_only()`（检查所有 effect 均为 Read）在裁决器中启用"效应检查"——如果一个 Action 的 effects 都是 `Read`，裁决器可以自动放行（对应 `auto_allow_readonly`）。

> **TODO**：`entity` 字段应提升为 `TypeId` 以保持类型一致性。

### 2.9 `EntityCategory` — 二元本体分类（🆕 设计文档未覆盖）

> **实现状态**：✅ 已完成。在所有 `EntityDef` 和 `ActionDef` 中附加。

当前的二元本体采用 **Domain Semantic（领域语义）** 和 **System Resource Semantic（系统资源语义）** 的两类划分：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EntityCategory {
    Domain,   // File, Workspace, Module — 领域第一公民
    System,   // LLM, Provider, Capability — 系统基础设施
}

impl Display for EntityCategory {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain => f.write_str("domain"),
            Self::System => f.write_str("system"),
        }
    }
}

impl EntityCategory {
    pub fn is_system(&self) -> bool { matches!(self, Self::System) }
}
```

**用途**：
- 在 `TypeRegistry` 中支持按分类过滤查询：`entities_by_category()`、`actions_by_category()`
- 本体合并时，Domain 和 System 本体独立维护，通过 `merge(&TypeRegistry)` 组合
- 防火墙模型中 `FirewallModelInfo` 的 `set_model_id()` 仅重算 System 本体的推理规则

**对应的 `TypeRegistry` 查询方法**：
```rust
impl TypeRegistry {
    pub fn entities_by_category(&self, category: EntityCategory) -> Vec<&EntityDef>;
    pub fn actions_by_category(&self, category: EntityCategory) -> Vec<&ActionDef>;
    pub fn field_aliases_by_category(&self, category: EntityCategory) -> HashMap<...>;
    pub fn entity_field_aliases_by_category(&self, category: EntityCategory) -> HashMap<...>;
    pub fn defaults_by_category(&self, category: EntityCategory) -> HashMap<...>;
}
```

### 2.10 `ReasoningRule` — 推理引擎（🆕 设计文档未覆盖）

> **实现状态**：✅ 已完成。`reasoner.rs` 实现了一个基于 `ReasoningRule` 的确定性单步推理引擎。

`ReasoningRule` 提供声明式知识推导——无需手写 Rust 逻辑，即可从已有字段推导出新知识：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReasoningRule {
    /// 遍历：沿 LinkDef 从源实体到目标实体
    Traversal {
        id: TypeId,
        /// 要遍历的 link ID
        link_id: TypeId,
        /// 起始实体类型
        source_type: TypeId,
        /// 目标实体类型
        target_type: TypeId,
        #[serde(default)]
        description: Option<String>,
    },

    /// 推导：在实体上计算新字段
    Derivation {
        id: TypeId,
        /// 该规则适用的实体类型
        entity_type: TypeId,
        /// 源字段列表
        source_fields: Vec<String>,
        /// 推导出的字段名
        derived_field: String,
        /// 推导逻辑
        expression: DerivationExpr,
        #[serde(default)]
        description: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DerivationExpr {
    /// 如果 source field == expected value → 输出 result value
    FieldEquals {
        field: String,
        expected: serde_json::Value,
        result: serde_json::Value,
    },

    /// 如果 source field 为 true → 输出 result value
    FieldIsTrue {
        field: String,
        result: serde_json::Value,
    },

    /// 两个字段的算术运算
    Arithmetic {
        left_field: String,
        operator: ArithmeticOp,
        right_field: String,
    },

    /// 直接复制另一个字段的值
    Alias { source: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ArithmeticOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}
```

**推理引擎**（`reasoner.rs`）：
```rust
/// 遍历求值：验证 link 在注册表中存在且类型匹配
pub fn evaluate_traversal(
    registry: &TypeRegistry,
    rule: &ReasoningRule,
) -> Option<TraversalResult>;

/// 推导求值：在字段值上计算 DerivationExpr
pub fn evaluate_derivation(
    rule: &ReasoningRule,
    fields: &HashMap<String, serde_json::Value>,
) -> Option<DerivationResult>;

/// 在一次传递中对字段值求值所有适用的推导规则
pub fn evaluate_all_derivations(
    rules: &[ReasoningRule],
    fields: &HashMap<String, serde_json::Value>,
) -> Vec<DerivationResult>;
```

**内置规则示例**（5 条，`builtin.rs` 中定义。**注意**：实际代码使用 `TypeId::from(...)` 而非字符串 `"..."`，字段名与设计图不同）：

```rust
// 规则 1：如果 LLM 支持 vision → 标记 has_image_input_modality
ReasoningRule::Derivation {
    id: TypeId("vision_implies_image_input"),
    entity_type: TypeId("LLM"),
    source_fields: vec!["supports_vision".into()],
    derived_field: "has_image_input_modality".into(),
    expression: DerivationExpr::FieldIsTrue {
        field: "supports_vision".into(),
        result: serde_json::json!(true),
    },
    description: Some("If supports_vision is true, the model has Image input modality".into()),
}

// 规则 2：LLM 总成本 = input pricing + output pricing
ReasoningRule::Derivation {
    id: TypeId("total_cost_per_million"),
    entity_type: TypeId("LLM"),
    source_fields: vec!["pricing_input_per_million".into(), "pricing_output_per_million".into()],
    derived_field: "total_cost_per_million".into(),
    expression: DerivationExpr::Arithmetic {
        left_field: "pricing_input_per_million".into(),
        operator: ArithmeticOp::Add,
        right_field: "pricing_output_per_million".into(),
    },
    description: Some("Total cost per million tokens = input + output pricing".into()),
}

// 规则 3：Provider → LLM link traversal
// 规则 4：LLM → Capability link traversal
// 规则 5：Workspace → File link traversal
```

> **设计决策**：`ReasoningRule` 是单步推理（single-step），不实现前向链接（forward chaining）或不动点（fixpoint）。每条规则独立求值，结果不注入注册表。

---


## 三、与工具系统的集成

> **总体状态**：Normalizer（§3.3）、Firewall Validator（§3.4）、Adjudicator（§3.5）、`to_json_schema()` 桥接（§3.2）四条集成路径均已实现，本体驱动工具注册闭环已打通。

### 3.1 当前工具注册流程 vs 本体驱动流程

```
── 当前 ──                           ── 本体驱动（实现） ──
#[tool] 宏生成 ToolDefinition      ActionDef.to_json_schema()             ✅ 已实现
       │                                      │
       ▼                                      ▼
ToolRegistry.register(name, exec)    TypeRegistry.register_action(def)    ✅ 实现
       │                                      │
       ▼                                      ▼
       执行                              TypeRegistry 生成 JSON Schema    ✅ 闭环
                                                  │
                                          通过 ToolBridge 注册到 ToolRegistry
```

### 3.2 `ActionDef → ToolDefinition` 生成

> **实现状态**：✅ 已实现。通过 `ActionDef::to_json_schema()` 从 `fields: Vec<FieldDef>` 动态构造 JSON Schema。

```rust
// 实现现状（types.rs）：
impl ActionDef {
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for field in &self.fields {
            let mut prop = serde_json::Map::new();
            prop.insert("type".into(),
                serde_json::Value::String(field.value_type.0.clone()));
            if let Some(ref desc) = field.description {
                prop.insert("description".into(),
                    serde_json::Value::String(desc.clone()));
            }
            if field.required {
                required.push(serde_json::Value::String(field.name.clone()));
            }
            properties.insert(field.name.clone(), serde_json::Value::Object(prop));
        }

        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), serde_json::Value::String("object".into()));
        schema.insert("additionalProperties".into(), serde_json::Value::Bool(false));
        schema.insert("properties".into(), serde_json::Value::Object(properties));
        if !required.is_empty() {
            schema.insert("required".into(), serde_json::Value::Array(required));
        }
        serde_json::Value::Object(schema)
    }
}

### 3.3 Normalizer 驱动

> **实现状态**：✅ 已实现。位于 `uncode-agent/src/decision/firewall.rs:DeclarativeNormalizer`。不使用 `ConstraintEvaluator` wrapper——Normalizer 仅负责别名规约和默认值填充，约束校验由 `OntologyConstraintRule` 独立处理。

```rust
pub struct DeclarativeNormalizer {
    field_mapping: HashMap<String, String>,  // alias → canonical field name
    defaults: FieldDefaults,                 // (tool_name, field_name) → default value
    ontology: Option<Arc<TypeRegistry>>,     // 🆕 用于 path_fields 路径解析
    cwd: PathBuf,                            // 🆕 当前工作目录
}

impl DeclarativeNormalizer {
    /// 从 TypeRegistry 提取别名映射和默认值
    pub fn from_registry(registry: &TypeRegistry) -> Self { /* ... */ }

    /// 便捷构造：使用完整 coding_agent + system_resource 本体
    pub fn builtin() -> Self { /* ... */ }
}

impl From<&TypeRegistry> for DeclarativeNormalizer { /* ... */ }
```

**核心逻辑**（新增路径字段自动解析，通过 `self.ontology` 查询 `ActionDef::path_fields`）：

```rust
fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError> {
    let mut args = action.arguments.clone();
    let mut log = Vec::new();

    // 1. 字段名规约：别名 → 规范名
    // 2. 默认值填充：缺失字段填充 default
    // 3. 路径字段自动解析（通过 ontology 查询 ActionDef.path_fields）
    //    对每个 path_field 调用 resolve_path() 做规范化
```

> **TODO**：`resolve_external()` 外部引用解析（如 `"module::Type"` → `TypeId("Type")`）未实现。

### 3.4 Firewall Validator 驱动

> **实现状态**：✅ 已实现。`uncode-agent/src/decision/firewall.rs:OntologyConstraintRule` 已接入 `build_default_firewall()`。

实现直接将 `ActionDef.preconditions` 中的 `Vec<Constraint>` 包装为 `ValidationRule`，通过 `evaluate_constraint()` 自由函数求值——不使用设计中的 `ConstraintEvaluator` struct wrapper：

```rust
pub struct OntologyConstraintRule {
    registry: uncode_ontology::TypeRegistry,  // 🆕 持有整个注册表用于实体 invariants 解析
}

impl ValidationRule for OntologyConstraintRule {
    fn validate(&self, parsed: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        // 1. 校验 ActionDef.preconditions（逐条 evaluate_constraint）
        // 2. 校验 Effect 引用实体的 invariants（含 extends 继承链解析）
        // 3. Hard 级别失败→approved:false 阻断；Soft 记录 violations 但放行
        // ...
    }
}

### 3.5 裁决器驱动

> **实现状态**：✅ 已实现。`uncode-agent/src/decision/adjudication.rs:EffectBasedPolicy`。

```rust
pub struct EffectBasedPolicy {
    auto_approve_readonly: bool,   // 🆕 可配置标记
}

impl DecisionPolicy for EffectBasedPolicy {
    fn evaluate(
        &self, _ctx: &DecisionContext, action: &NormalizedAction,
    ) -> Result<DecisionVerdict, AdjudicationError> {
        // 基于 Effect::is_read_only() 的自动放行
    }

    fn name(&self) -> &str { "effect_based_policy" }
}
```

> **注意**：该策略始终返回 `approved()`，不会拒绝——它只负责自动放行只读操作。破坏性操作仍走完整裁决链。

---

## 四、Ground Truth / Operational Truth 架构

> **决策**：⚠️ 不再实现。防火墙 P→V→N 管线已等效完成 GT→OT 转换。`OperationalTruth`/`GroundTruth`/`TruthFeedback` 作为中间抽象层会增加一层 indirection 而不带来额外价值——当前的 `ValidatedAction`/`NormalizedAction` 已从语义防火墙产出确定性结构。本章保留作为语义参考（概念注解），不再推进到实现阶段。

### 4.1 二元真相模型（概念注解）

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

### 4.2 Rust 类型表示（概念草案，不实现）

> **决策理由**：`ValidatedAction` + `NormalizedAction` 已在防火墙中承载 GT/OT 的角色。引入 `GroundTruth` wrapper 只是重新打包已有数据，不增加约束力。`OperationalTruth.confidence: f32` 在当前 LLM API 中无可靠数据源（大多数 API 不返回置信度）。

```rust
// 以下类型仅作概念注解，不会落地——防火墙的 ValidatedAction/NormalizedAction 已等效替代。

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

> **注**：本体仅对 9 个最核心工具建模（read/write/edit/grep/find/ls/bash/web_fetch/web_search）。后续新增的 `question`/`skill`/`task` 三个工具未注册到 TypeRegistry，仅通过 `ToolRegistry` 直接注册执行器。

```
TypeRegistry 中 actions 的编码 = 对 9 个核心 builtin tools 的本体描述

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

> **注意**：以下示例使用旧设计的 `input_schema: json_schema!(...)` 格式（为可读性保留）。实际代码中改为 `fields: Vec<FieldDef>` + `path_fields: Vec<PathField>`，见 §2.4 ADR-4。`output_type` 从 `TypeId::STRING` 常量改为 `TypeId::string()` 方法调用。

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

> **实现状态**：✅ 已实现。PathField/PathType 类型已添加到本体，7 个工具的 ActionDef 声明了 path_fields。DeclarativeNormalizer 在 normalize() 中自动解析路径。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathType {
    FilePath,         // 文件路径（read, write, edit）
    DirectoryPath,    // 目录路径（grep, find, ls）
    WorkDir,          // 工作目录（bash）
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathField {
    pub field_name: String,
    pub path_type: PathType,
    pub default: Option<String>,      // "." 表示默认当前目录
    pub must_exist: bool,              // 文件必须存在（read/edit=true, write=false）
}
```

**集成方式**：
- `ActionDef.path_fields` 字段声明哪些参数是路径
- `DeclarativeNormalizer` 持有 `ontology: Option<Arc<TypeRegistry>>` + `cwd: PathBuf`
- normalize() 中为每个 path_field 调用 `resolve_path()` 做规范化
- 工具 executor 中的 `resolve_path()` 调用保留为纵深防御

> **TODO**：`PathSafetyRule` 尚未从本体动态读取 path 字段名（仍硬编码 `"path"`/`"file"`）。

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

> **决策**：Phase 1 仅保留 `OntologyVersion` + `is_compatible_with()`（向前兼容检查）。`OntologySnapshot`、`EvolutionEngine`、`EvolutionReason`、`CompatibilityReport`、`EvolutionLog` 延后到 Phase 2——在当前 9 个固定工具的 Agent 场景中，本体演化尚非瓶颈。当系统支持用户自定义工具 / 扩展商店时，演化机制成为阻断项，届时从 Phase 2 激活。

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

> **整体进度**：Phase 1 全量完成。`to_tool_definition()`、`EntityDef.invariants`/`extends`、`PathField`、`RegexMatch`（regex crate）、`CustomRule`（回调机制）均已落地。
> 1764 workspace 测试（80 个 `uncode-ontology`：61 unit + 14 integration + 5 doctest），0 失败。
> GT/OT (§4) 已废弃，EvolutionEngine (§7.3) 延后到 Phase 2。

### Phase 1a：crate 基础（1 天）✅ 已完成

- ✅ 创建 `crates/uncode-ontology/Cargo.toml`
- ✅ 实现 `types.rs`（TypeId, EntityDef, ActionDef, LinkDef, FieldDef, Constraint, Effect, EntityCategory, ReasoningRule）
- ✅ 实现 `evaluate.rs`（ConstraintResult, FieldLookup trait, evaluate_constraint）
- ✅ 实现 `registry.rs`（TypeRegistry, merge, 分类查询, 字段别名/默认值汇总）
- ✅ 实现 `reasoner.rs`（traversal + derivation + evaluate_all_derivations）
- ✅ 单元测试：TypeRegistry 注册/查询、Constraint 求值、ReasoningRule 推理

### Phase 1b：工具本体编码（1 天）✅ 已完成 + 超出

- ✅ 实现 `builtin.rs`：3 个 `LazyLock` 缓存的构建函数
- ✅ `coding_agent_ontology()`：9 个工具 ActionDef + 3 个 Domain EntityDef
- ✅ `system_resource_ontology()`：LLM/Provider/Capability 3 实体 + 2 动作 + 5 推理规则
- ✅ `full_ontology()`：合并 domain + system 本体
- ✅ `ActionDef::to_json_schema()` 实现 — 从 FieldDef 动态构造 JSON Schema，闭环节点已打通

### Phase 1c：Normalizer 集成（1 天）✅ 已完成

- ✅ 实现 `DeclarativeNormalizer`（`firewall.rs`）
- ✅ 实现 `from_registry()` 和 `builtin()` 构造
- ✅ `From<&TypeRegistry>` impl
- 🔴 `resolve_external()` 外部引用解析未实现

### Phase 1d：Firewall 集成（1 天）✅ 已完成

- ✅ 实现 `OntologyConstraintRule`（包装 `ActionDef.preconditions` 为 `ValidationRule`）
- ✅ 集成到 `build_default_firewall()` 中
- ✅ `RegexMatch` 已接入 regex crate，`CustomRule` 支持回调映射表

### Phase 1e：裁决器集成（0.5 天）✅ 已完成

- ✅ 实现 `EffectBasedPolicy`（基于 Effect 的自动放行）
- ✅ 集成到 `Adjudicator` 策略链

### Phase 1f：#[tool] 宏扩展（1 天）❌ 未启动

> **决策**：低优先级。手写 `ActionDef` + `to_tool_definition()` 的组合比宏方案更透明、可调试。当工具数量 >20 时可重新评估宏方案的价值。

- 扩展 proc-macro 支持 `#[ontology(action = "...", effects = [...])]`
- 生成 `__ontology_action_*()` 函数返回 `ActionDef`

### Phase 1 收尾：P0 修补清单（均已闭环）

| 编号 | 任务 | 状态 | 实现位置 |
|---|---|---|---|
| P0-1 | `ActionDef::to_tool_definition()` / `to_json_schema()` | ✅ 完成 | `types.rs:ActionDef::to_json_schema()`，从 `fields: Vec<FieldDef>` 动态构造 JSON Schema |
| P0-2 | `EntityDef.invariants` 字段 | ✅ 完成 | `types.rs:EntityDef.invariants`；`firewall.rs:OntologyConstraintRule` 自动追加实体 invariants 到 Action 校验 |
| P0-3 | `EntityDef.extends` 字段 | ✅ 完成 | `registry.rs:TypeRegistry::resolve_entity()`，支持递归继承链合并 |
| P0-4 | TypeId 内置常量 | ✅ 完成 | `types.rs:TypeId::string()/integer()/number()/boolean()/unit()/any()` 六个方法 |
| P0-5 | `FieldDef.value_type` 升级为 `TypeId` | ✅ 完成 | 已全部替换，无遗留 String 类型 |

### Phase 2 预登记（延期项）

| 原设计 § | 延期内容 | 激活条件 | 决策理由 |
|---|---|---|---|
| §4 | GT/OT 二元真相模型 | 已废弃 | P→V→N 管线已有等效功能，引入新类型是过度抽象 |
| §7.2-§7.3 | EvolutionEngine 演化子系统 | 用户自定义工具 / 扩展商店上线 | 固定 9 工具场景下演化非瓶颈 |
| §5.3 | PathField / PathType | 路径沙箱逻辑重构 | executor 中隐式路径解析对 Phase 1 足够 |
| Phase 1f | #[tool] 宏扩展 | 工具数 >20 或频繁增加 | 手写 ActionDef 更透明可调试 |

### Phase 1 成果总结

| 能力 | 实现位置 | 生产状态 |
|---|---|---|
| ActionDef → ToolDefinition 桥接 | `ToolBridge` + `to_json_schema()` | ✅ 每轮运行 |
| EntityDef invariants + extends | `TypeRegistry::resolve_entity()` | ✅ 每次裁定 |
| Entity invariants → Action 校验 | `OntologyConstraintRule::validate()` | ✅ 每次裁定 |
| PathField 路径自动解析 | `DeclarativeNormalizer::normalize()` | ✅ 每次裁定 |
| RegexMatch 接入 regex crate | `evaluate_constraint_with_rules()` | ✅ 每次裁定 |
| CustomRule 回调机制 | `CustomRuleFn` + callback map | ✅ 可用 |
| ToolRegistry 本体优先 | `definitions()` ontology path | ✅ 每轮运行 |
| Provider 序列化验证 | 4 providers build 函数全测 | ✅ |
| EffectBasedPolicy 裁定 | `build_default_adjudicator()` | ✅ 每次裁定 |
| ModelCapabilityRule | firewall validators | ✅ 每次裁定 |
| GuardrailConfig 加载 | `harness.rs:114` | ✅ 启动时 |
| ReasoningRule 推理引擎 | `reasoner.rs` | ❌ 延后 — 5 规则已注册，无生产触发 |

**本体综合利用率：约 70%（6/7 路径在生产中运行）**

---

## 九、验证标准

| 测试场景 | 预期结果 | 状态 |
|:---|:---|:---|
| 本体注册 9 个工具 | `TypeRegistry.actions.len() == 11`（9 domain + 2 system） | ✅ 通过 |
| `coding_agent_ontology()` 包含所有 Domain Entity | File, Workspace, Module | ✅ 通过 |
| `system_resource_ontology()` 包含 System Entity | LLM, Provider, Capability | ✅ 通过 |
| `full_ontology()` 合并 domain + system | actions = 11, 所有 entity 可查 | ✅ 通过 |
| 5 条内置 ReasoningRule 求值 | traversal + derivation 正确输出 | ✅ 通过 |
| `ConstraintResult::BitOr` 合并逻辑 | Pass\|Warn=Warn, Warn\|Fail=Fail | ✅ 通过 |
| `FieldLookup` trait 对 HashMap + serde_json::Map | 两种 impl 均可求值 | ✅ 通过 |
| `DeclarativeNormalizer.normalize()` 遇 `filepath` → `path` | `normalized_fields` 包含 `"filepath → path"` | ✅ 通过 |
| `DeclarativeNormalizer.normalize()` 遇缺失可选字段 | 自动填充默认值 | ✅ 通过 |
| `EffectBasedPolicy` 遇 `read` 工具的 proposal | 自动放行（只读效应） | ✅ 通过 |
| `EffectBasedPolicy` 遇 `write` 工具的 proposal | 不放行（需走完整裁决链） | ✅ 通过 |
| `OntologyVersion::is_compatible_with()` | 同 semver 兼容判定 | ✅ 通过 |
| `ActionDef::to_tool_definition()` 生成 ToolDefinition | 与当前手写等价 | ✅ 通过 |
| `EntityDef.invariants` 自动追加到相关 ActionDef.preconditions | `read` 检查 File invariants | ✅ 通过 |
| `TypeId::STRING` 值为 `"string"` | `TypeId::STRING.0 == "string"` | ✅ 通过 |
| `Constraint::RegexMatch` 求值 | pattern 匹配失败→Fail | ✅ 通过 |

**已废弃项（不再验证）**

| 原验证标准 | 废弃原因 |
|:---|:---|
| `OntologyConstraintRule` 遇 `rm -rf /` → 拒绝 | 字段级 CustomRule 语义需重新设计 |
| 本体版本升级：新增必填字段 → ❌ 向前兼容 | EvolutionEngine 延后到 Phase 2 |
| 影响分析：修改 `read` ActionDef → 报告 `edit` 受影响 | EvolutionEngine 延后到 Phase 2 |
| GT/OT `operational_to_ground()` 转换 | GT/OT 模型已废弃 |

---

## 十、参考资料

| 资料 | 位置 | 用途 |
|:---|:---|:---|
| 重构回顾 | `docs/technologies/UNCODE_ONTOLOGY_REFACTORING_RETROSPECTIVE.md` | 设计→实现的差距分析、实现中的架构偏离及原因 |
| 旧版本体论基础 | `docs/references/ontology-foundations.md` | GT/OT 二元真相、Palantir 三层架构、本体四层模型（方案 §四 的理论来源） |
| 旧版本体-决策映射 | `docs/references/ontology-decision-mapping.md` | 本体在决策四阶段的角色分析、本体反馈演化机制（方案 §三 Validator/Normalizer/裁决器 的设计来源） |
| 旧版本体 vs DDD 对比 | `docs/references/ontology-vs-ddd.md` | 九维理论对比（战略/语义互补，方案中领域第一公民的理论正当性） |
| Microsoft Ontology Playground | `https://github.com/microsoft/Ontology-Playground` | 可视化本体编辑器 + RDF/XML 双向导出 + 自然语言→本体查询。**Entity/Property/Relationship 的 TypeScript 类型定义验证了我们方案中 EntityDef/FieldDef/LinkDef 的设计模式一致性**。NL2Ontology 查询引擎与我们的防火墙概念同构，可作为 Constraint 求值引擎 `evaluate.rs` 的概念验证。项目使用标准 OWL/RDF 格式——本方案未直接采用（对 Agent 运行时过重），但如需与外部本体互操作，其 RDF 序列化实现是参考模板。 |

---

## 十一、架构决策日志

以下记录设计→实现过程中的关键架构决策，包括原因、权衡、替代方案。

### ADR-1：GT/OT 二元真相模型的废弃

| 项 | 说明 |
|---|---|
| **日期** | 2026-05 |
| **决策** | 不实现 `OperationalTruth` / `GroundTruth` / `TruthFeedback` |
| **原因** | P→V→N 管线已等效完成 GT→OT 转换—— LLM 非结构化输出经 Parser→Validator→Normalizer 转化为 `ValidatedAction` / `NormalizedAction`，引入中间层不增加约束力 |
| **评估框架** | "模型外推理"参考文档——防火墙 Validator + Constraint 求值即是"符号系统做确定性推理"的体现 |
| **替代方案** | 保持 GT/OT 概念在文档中作为语义注解（§4），不在代码中创建新类型 |
| **后果** | 文档 §4 变为纯概念参考；无代码变更 |

### ADR-2：EvolutionEngine 延后到 Phase 2

| 项 | 说明 |
|---|---|
| **日期** | 2026-05 |
| **决策** | Phase 1 仅保留 `OntologyVersion` + `is_compatible_with()`；演化子系统（`OntologySnapshot` / `EvolutionEngine` / `EvolutionLog`）延后 |
| **原因** | 本体演化的工程复杂度远超其余待实现项之和。固定 9 工具场景下，`OntologyVersion` 已满足 semver 兼容检查需求 |
| **激活条件** | 用户自定义工具上线、扩展商店场景——届时需 `EvolutionEngine.migrate()` 处理旧 Session 的字段迁移 |
| **评估框架** | "本体桥梁七环节"之环节7（学习与适应）——当前 Agent 不需要从交互中更新本体 schema |
| **替代方案** | 先做最小闭环：`OntologyVersion` + `is_compatible_with()` |

### ADR-3：PathField 路径建模

| 项 | 说明 |
|---|---|
| **日期** | 2026-05 |
| **决策** | ✅ 已完成。PathField/PathType 已加入本体类型系统，7 个工具的 ActionDef 声明了 path_fields。DeclarativeNormalizer 在 normalize() 中自动调用 resolve_path()。工具 executor 中的路径解析保留为纵深防御 |
| **此前状态** | 2026-05 初次评估时延后到 Phase 2。路径字段知识保持在各工具 executor 中 |
| **重新激活原因** | 实现成本低于预期（~170 行），Normalizer 已有足够上下文（ontology + cwd）来驱动路径解析 |

### ADR-4：ActionDef.input_schema → fields 的设计变更

| 项 | 说明 |
|---|---|
| **日期** | 2025-12 |
| **决策** | `ActionDef` 用 `fields: Vec<FieldDef>` 替代原始设计的 `input_schema: JsonSchema` |
| **原因** | `FieldDef` 是结构化数据，比预构建 JSON Schema 更灵活——可从 fields 动态生成 Schema（`to_tool_definition()`），也可反向从 JSON Schema 解析为 fields |
| **替代方案** | 双持 `fields` 和 `input_schema`（冗余）；放弃 `to_tool_definition()` 动态生成能力 |
| **后果** | `to_tool_definition()` 需从 `FieldDef` 动态构造 JSON Schema，而非直接返回预构建对象 |

### ADR-5：ReasoningRule 单步推导的设计选择

| 项 | 说明 |
|---|---|
| **日期** | 2025-12 |
| **决策** | 推理引擎采用单步推导（single-step），不实现前向链接（forward chaining）或不动点（fixpoint） |
| **原因** | 当前 5 条内置规则均为独立求值——`FieldIsTrue` / `Arithmetic` / `Alias` 不需要链式推理。不动点计算在规则数 <10 时不会产生新结论 |
| **评估框架** | "模型外推理"参考文档——符号推理的前向链是"所有可推导结论的一次性计算"；uncode 的 5 条规则无需链式传递 |
| **激活条件** | 规则表增长到需要规则间依赖时（如规则 B 依赖规则 A 的输出）——此时需引入 Rete 模式或 fixpoint 迭代 |

### ADR-6：纯 Rust 类型替代 OWL/RDF 的工程决策

| 项 | 说明 |
|---|---|
| **日期** | 2025-12 |
| **决策** | 不使用 RDF/Turtle/OWL/SPARQL，全部采用 Rust struct + serde JSON |
| **原因** | RDF 序列化和 SPARQL 查询对 Agent 运行时过重——每次校验都解析 Turtle 不可接受。Rust struct 提供编译期类型安全 + 运行时零开销 |
| **评估框架** | Microsoft Ontology Playground 验证了 Entity/Property/Relationship 三元的普适性；去掉 RDF 层不影响建模能力 |
| **后果** | 无法与外部 OWL 本体互操作（可接受——当前无此需求） |

---

## 附录 A：最终状态快查表

| 设计项 | 设计 § | 实现状态 |
|:---|:---|:---|
| TypeId + Deref/From | §2.2 | ✅ 完成 |
| TypeId 内置常量 | §2.2 | ✅ 完成 (string/integer/number/boolean/unit/any) |
| EntityDef + invariants + extends | §2.3 | ✅ 完成 |
| EntityCategory 二元分类 | §2.9 | ✅ 完成 |
| ActionDef | §2.4 | ✅ 完成 |
| FieldDef (value_type: TypeId) | §2.5 | ✅ 完成 (P1 修补) |
| LinkDef + Cardinality + inverse | §2.6 | ✅ 完成 |
| Constraint + 7 变体 + 求值 | §2.7 | ✅ 完成 (Referential 已补) |
| ConstraintResult + BitOr | §2.7 | ✅ 完成 |
| RegexMatch (regex crate) | §2.7 | ✅ 完成 |
| CustomRule (回调机制) | §2.7 | ✅ 完成 |
| Effect (6 变体) | §2.8 | ✅ 完成 |
| ReasoningRule + reasoner.rs | §2.10 | ✅ 完成 |
| system_resource_ontology() | — | ✅ 完成 |
| ActionDef::to_tool_definition() | §3.2 | ✅ 完成 (ToolBridge) |
| DeclarativeNormalizer | §3.3 | ✅ 完成 |
| OntologyConstraintRule | §3.4 | ✅ 完成 (含 entity invariants 继承) |
| EffectBasedPolicy | §3.5 | ✅ 完成 |
| PathField / PathType | §5.3 | ✅ 完成 (Normalizer 自动解析) |
| 9 工具 ActionDef | §5.2 | ✅ 完成 |
| Provider 序列化验证 | — | ✅ 完成 (4 providers) |
| SurrealSessionStore 集成测试 | — | ✅ 完成 (+9 tests, fork bug fixed) |
| OperationalTruth / GroundTruth | §4.2 | 🗑 废弃 (ADR-1) |
| EvolutionEngine | §7.2-§7.3 | Phase 2 延期 (ADR-2) |
| #[tool] 宏扩展 | §8 Phase 1f | Phase 2 延期 |

**Phase 1 闭环路径**: ActionDef → to_json_schema() → ToolBridge → ToolRegistry.definitions() → Provider build_tools_json() → LLM API ✅
