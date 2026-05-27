# 双推理激活分析

> 背景：`uncode-ontology` 中实现了约束求值（`evaluate_constraint`）、推导（`evaluate_derivation`）、遍历（`evaluate_traversal`）三个符号推理引擎，但当前仅约束求值接入了生产链路。本文讨论剩余两个引擎的激活路径、收益和技术方案。

---

## 一、当前状态

### 1.1 三引擎全景

| 引擎 | 函数 | 实现 | 测试 | 生产调用 | 角色 |
|:---|:---|:---|:---|:---|:---|
| 约束求值 | `evaluate_constraint` | ✅ | 11 tests | ✅ `firewall.rs:620` | 守门人：校验 LLM 工具调用参数是否合法 |
| 推导 | `evaluate_derivation` | ✅ | 11 tests | ❌ | 计算者：从已知字段派生新字段 |
| 遍历 | `evaluate_traversal` | ✅ | 11 tests | ❌ | 图查询：沿 LinkDef 发现关联实体 |

### 1.2 5 条已注册推理规则

```
coding_agent_ontology:
  rule_workspace_files_traversal  Traversal   Workspace → File

system_resource_ontology:
  rule_provider_models_traversal  Traversal   Provider  → LLM
  rule_llm_capabilities_traversal  Traversal   LLM       → Capability
  rule_vision_implies_image_input  Derivation  supports_vision → has_image_input_modality
  rule_total_cost_derivation       Derivation  pricing_input + pricing_output → total_cost_per_million
```

### 1.3 约束求值——唯一活着的分支

```
LLM 输出 ToolCall { name: "read", args: { path: "src/main.rs" } }
  → ProposalAccumulator → ActionProposal
  → SemanticFirewall.process()
      → OntologyConstraintRule.validate()
          → registry.get_action("read").preconditions  // [RequiredField("path"), CustomRule("file exists")]
          → registry.resolve_entity("File").invariants  // [RequiredField("path")]
          → for each constraint: evaluate_constraint(c, args_map)
              → Pass / Warn / Fail(Hard→拒绝)
```

---

## 二、推导引擎激活分析

### 2.1 接入点

`ModelBridge::model_to_fields()`（`bridge.rs:38`）是唯一的天然接入点：

```rust
pub fn model_to_fields(model: &Model) -> HashMap<String, serde_json::Value> {
    // 当前产出 10 个字段：
    // model_id, provider, context_window, max_output_tokens,
    // supports_vision, supports_reasoning, supports_tools,
    // api_protocol, pricing_input_per_million, pricing_output_per_million

    // ← 在此追加一行即可激活：
    // let derivations = evaluate_all_derivations(&reasoning_rules, &fields);
    // for d in derivations { fields.insert(d.derived_field, d.value); }
}
```

改动量：~5 行 Rust。下游 `check_capability()`、`CostBudgetPolicy` 等所有 `model_to_fields()` 消费者自动获得派生字段。

### 2.2 收益评估——当前规则集几乎为零

**`rule_vision_implies_image_input`**：
- 前提：`supports_vision = true`
- 结论：`has_image_input_modality = true`
- 问题：`supports_vision` 本身已是"模型能否处理图像"的终局判定，衍生的 `has_image_input_modality` **未新增任何决策信息**。这是重言式推导。

**`rule_total_cost_derivation`**：
- 前提：`pricing_input_per_million + pricing_output_per_million`
- 结论：`total_cost_per_million = 3.0`
- 问题：该字段是新的，但**当前无消费者**。`CostBudgetPolicy` 直接用两个独立定价值做计算，不查询合成字段。

**真实价值不在现有规则，在机制到位后的扩展性**：

| 可扩展规则示例 | 类型 | 价值 |
|:---|:---|:---|
| `context_usage > 80% && cheapest_alt_exists → suggest_switch` | Derivation | 成本治理自动化 |
| `file_was_read && sibling_in_same_module → suggest_related` | Derivation | 上下文注入 |
| `tool_sequence = [grep, read, edit] → refactoring_session` | Derivation | 会话意图识别 |

### 2.3 结论

推导激活**不构成独立里程碑**。可在遍历激活或后续功能开发时顺手完成。它的价值在机制而非内容。

---

## 三、遍历引擎激活分析

### 3.1 核心差距——类型级 vs 实例级

```
当前引擎输出                    生产需要
─────────────────────       ─────────────────────────
TraversalResult {            Vec<EntityInstance> {
  target_ids: [                 { type: "File", id: "src/main.rs", ... },
    TypeId("File")              { type: "File", id: "Cargo.toml",  ... },
  ]                           ]
}
```

ontology 只有 `TypeRegistry`（记录哪些类型存在），没有**实例注册表**（记录运行时有哪些具体实体）。遍历引擎的注释也确认了这一点：

> "This returns entity types, not instances. Instance-level traversal requires runtime data (e.g., Model instances), which is handled by the bridge."

### 3.2 路线对比

| | 路线 A：实例注册表 | 路线 B：手写桥接 |
|:---|:---|:---|
| **做法** | 新建 `InstanceRegistry`，注入运行时实体实例，遍历引擎直接查询实例 | 在 `bridge.rs` 中绕过 ontology，手写遍历逻辑 |
| **例子** | `registry.traverse("Workspace_contains_File") → [Instance("src/main.rs"), Instance("Cargo.toml")]` | `fn files_in_workspace(cwd) → Vec<PathBuf>` 直接调 `ls` |
| **优点** | 与本体统一建模，后续遍历规则自动生效；`reasoner.rs` 获得生产价值 | 快，不引入新模块 |
| **缺点** | 新模块 + 实例注入 + 生命周期管理 | 绕开本体，"双推理"叙事破产；`reasoner.rs` 成永久死代码 |
| **推荐** | ✅ | ❌ |

路线 B 等于承认 ontology 的遍历引擎只是个装饰——不值得做。

### 3.3 实例注册表——详细设计

#### 3.3.1 位置：`crates/uncode-ontology/src/instance.rs`

EntityInstance 是 EntityDef 的运行时投影，与类型定义同属 ontology crate。

#### 3.3.2 核心类型

```rust
/// 实体运行时实例
#[derive(Debug, Clone)]
pub struct EntityInstance {
    pub type_id: TypeId,                           // e.g. TypeId("File")
    pub id: String,                                 // e.g. "src/main.rs"
    pub fields: HashMap<String, serde_json::Value>, // 映射 FieldDef.name → 值
}

/// 实例注册表
#[derive(Debug, Clone, Default)]
pub struct InstanceRegistry {
    // 主索引：(type_id, id) → instance
    instances: HashMap<(TypeId, String), EntityInstance>,
    // 类型索引：type_id → id 列表（快速 list_by_type）
    by_type: HashMap<TypeId, Vec<String>>,
}
```

**API**：

```rust
impl InstanceRegistry {
    // ── 增删查 ──
    pub fn insert(&mut self, instance: EntityInstance);
    pub fn remove(&mut self, type_id: &TypeId, id: &str);
    pub fn get(&self, type_id: &TypeId, id: &str) -> Option<&EntityInstance>;
    pub fn list_by_type(&self, type_id: &TypeId) -> Vec<&EntityInstance>;
    pub fn filter(
        &self,
        type_id: &TypeId,
        predicate: impl FnMut(&&EntityInstance) -> bool,
    ) -> Vec<&EntityInstance>;

    // ── 遍历（默认策略：field-match）──
    // 假设：target 类型实例的 field[source_type名小写] == source_id
    // 对 Provider→LLM 有效（LLM.provider == "deepseek"）
    // 对 Workspace→File 无效（需消费方自定义过滤）
    pub fn traverse_typed(
        &self,
        type_registry: &TypeRegistry,
        link_id: &TypeId,
        source_id: &str,
    ) -> Vec<&EntityInstance>;
}
```

**3 条遍历规则的实现方式**：

| 规则 | 方式 | 说明 |
|:---|:---|:---|
| `Provider_provides_LLM` | `traverse_typed()` | 通用 field-match 直接命中：`LLM.provider == provider_id` |
| `LLM_has_Capability` | 推导式 | 不存 Capability 实例——从 LLM 字段派生。用 `DerivationExpr` 计算后合并到 LLM 实例的 fields 中 |
| `Workspace_contains_File` | `filter()` + 路径前缀 | `File.path` 在 `Workspace.root` 下即属于该 Workspace |

#### 3.3.3 实例注入策略——按实体类型

**Workspace**（Agent 初始化时，1 个实例）：

```rust
EntityInstance {
    type_id: TypeId::from("Workspace"),
    id: cwd.to_string_lossy().into(),
    fields: { "root": cwd },
}
```

**LLM**（Agent 初始化时，从 `ModelRegistry.all_models()` 导入）：

```rust
for model in model_registry.all_models() {
    let fields = ModelBridge::model_to_fields(&model);  // 复用已有映射
    registry.insert(EntityInstance {
        type_id: TypeId::from("LLM"),
        id: model.id.clone(),
        fields,
    });
}
```

**File**（混合策略）：

| 阶段 | 来源 | 覆盖范围 |
|:---|:---|:---|
| 初始化 | WorkspaceGraph 构建结果（`file_hashes: HashMap<String, u64>`） | 所有 `.rs` 文件（已有扫描，零 I/O 增量） |
| 工具执行后 | 每次 read/write/edit/find/ls/grep 的结果，提取参数或输出中的文件路径 | 补充非 Rust 文件 + 新增文件 |
| compaction 时 | `CompactionEntry.files_read/files_modified` | 作为惰性注入的兜底 |

File 实例字段（精简——不存 content）：

```rust
EntityInstance {
    type_id: TypeId::from("File"),
    id: "src/main.rs".into(),
    fields: {
        "path": "src/main.rs",
        "exists": true,
    },
}
```

**Module**（可选，延后）：当前 `entity_module()` 仅定义了 `name` 字段，无实例来源。可从 Cargo.toml workspace members 解析，优先级低于 File/LLM/Workspace。

#### 3.3.4 生命周期

```
Agent 启动
  │
  ├─ Harness::new()
  │     └─ InstanceRegistry::new()
  │         ├─ insert Workspace 实例
  │         └─ insert LLM 实例 × N  (ModelRegistry.all_models())
  │
  ├─ run_inner() 首次 rebuild_context
  │     └─ WorkspaceGraph.build(cwd)
  │         └─ graph.file_hashes → insert File 实例 × M  (.rs 文件)
  │
  └─ 每个 turn 后
        ├─ 工具执行成功 → 从参数/输出提取文件路径 → insert/update File 实例
        ├─ Compaction → compaction_entry.files_read/modified → merge 进 InstanceRegistry
        └─ 所有 insert 使用幂等覆盖（同名文件以最新路径为准）
```

### 3.4 接入 Agent 循环——上下文注入

消费点在 `rebuild_context_with_injections()`。在已有的 Workspace Graph 注入之后追加：

```rust
// loop_engine.rs:1011 (workspace graph 注入之后)

// 实例级文件清单：轻量于 graph signatues，互补于 graph 结构
let file_instances = instance_registry.list_by_type(&TypeId::from("File"));
if !file_instances.is_empty() {
    let listing = file_instances.iter()
        .map(|inst| format!("  {}", inst.id))
        .take(30)     // 上限：避免长列表撑爆 context
        .collect::<Vec<_>>()
        .join("\n");
    built.messages.insert(
        after_graph_position,
        Message::system(format!("## Workspace Files\n\n当前工作区包含以下文件：\n{listing}")),
    );
}
```

**注入规则**：

- 仅当文件数 > 0 时注入
- 上限 30 个文件路径（保护 context window）
- 注入位置在 Workspace Graph 之后、用户消息之前
- 如果遍历结果与上个 turn 无变化，可缓存跳过（可选优化）

### 3.5 激活路径

| 阶段 | 内容 | 文件 | 估计改动 |
|:---|:---|:---|:---|
| Step 1 | `EntityInstance` + `InstanceRegistry` + 基础 API | `uncode-ontology/src/instance.rs` | ~150 行 |
| Step 2 | Harness 初始化时注入 Workspace + LLM 实例；WorkspaceGraph 构建后注入 File 实例 | `harness.rs` + `loop_engine.rs` | ~30 行 |
| Step 3 | `rebuild_context_with_injections()` 中注入文件清单 | `loop_engine.rs` | ~15 行 |
| Step 4 | 工具执行后增量更新 File 实例 | `loop_engine.rs`（工具结果处理附近） | ~20 行 |
| Step 5 | 推导顺手激活：`model_to_fields()` 追加 `evaluate_all_derivations()` | `bridge.rs` | ~5 行 |

总计：~220 行，其中核心新模块 ~150 行集中在 `instance.rs`。

---

## 四、优先级建议

```
约束求值   ████████████████  ✅ 已完成
推导激活   ████████████████  ✅ 已完成
遍历激活   ████████████████  ✅ 已完成 (InstanceRegistry + 上下文注入)
          │
          真正的能力提升在这里

优先级：遍历实例层 > 推导激活 > 扩展规则集
```

推导激活是机制的胜利，遍历实例层是能力的胜利。前者让"双推理"从 1/3 变成 2/3，后者让它从装饰变成武器。

---

## 五、相关文件

| 文件 | 角色 |
|:---|:---|
| `crates/uncode-ontology/src/reasoner.rs` | 遍历/推导引擎实现 |
| `crates/uncode-ontology/src/registry.rs` | TypeRegistry — 当前仅类型级 |
| `crates/uncode-ontology/src/instance.rs` | **（新增）** InstanceRegistry + EntityInstance — 实例注册表 |
| `crates/uncode-ontology/src/evaluate.rs` | 约束求值 — 已接入 |
| `crates/uncode-ontology/src/builtin.rs` | 5 条推理规则注册 |
| `crates/uncode-agent/src/decision/bridge.rs` | ModelBridge::model_to_fields() — 推导接入点 |
| `crates/uncode-agent/src/decision/firewall.rs` | OntologyConstraintRule — 约束求值接入点 |
| `crates/uncode-agent/src/loop_engine.rs` | AgentLoop — 遍历结果消费点 |
| `crates/uncode-agent/src/harness.rs` | AgentHarness — 实例注入点 |
| `docs/technologies/UNCODE_ONTOLOGY_DESIGN.md` | 本体详细技术方案 |
