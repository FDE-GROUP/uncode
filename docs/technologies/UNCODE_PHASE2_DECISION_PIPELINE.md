# Phase 2 工程设计：决策管线完善

> **对应重构方案**：`UNCODE_REFACTORING_PLAN.md` Phase 2
> **依赖**：Phase 0 完成（GuardrailConfig 加载、DeclarativeNormalizer）
> **预计工期**：3-4 天

---

## 一、目标

当前决策管线已是前门控模式（`loop_engine.rs:1592-1636`），firewall.process → adjudicator.adjudicate → allowed=true 才执行。本阶段不改变执行顺序，而是：

1. 补全 `ActionProposal` 的上下文字段（G-7）
2. 发射细粒度决策生命周期事件
3. 完善被拒绝提案的反馈路径（LLM 可感知拒绝原因并修正）
4. 审计记录持久化到 SurrealDB

---

## 二、现状分析

### 2.1 当前类型流

```
ActionProposal (types.rs:20-25)
  → ParsedAction (types.rs:28-31)
  → ValidatedAction (types.rs:34-39)
  → NormalizedAction (types.rs:41-46)
  → ApprovedAction (types.rs:48-52)
  → DecisionVerdict (types.rs:54-58)
```

### 2.2 当前 ActionProposal

```rust
// types.rs:20-25
pub struct ActionProposal {
    pub tool_name: String,
    pub raw_arguments: serde_json::Value,
    pub rationale: Option<String>,
    pub confidence: Option<f32>,
}
```

缺失：`proposal_id`、`intent`、`alternatives`、`trace`。

### 2.3 当前事件发射

仅 `AgentEvent::DecisionMade` 在两个位置发射：
- `loop_engine.rs:1620-1626`（裁决通过）
- `loop_engine.rs:1645-1651`（防火墙拒绝）

缺失：`ProposalReceived`、`FirewallCheck`、`ActionExecuted`、`DecisionAudited`。

### 2.4 当前审计

`PendingAudit`（`loop_engine.rs:1555`）在 turn 结束后通过 `persist_decision_audit()` 处理，但最终在内存中丢弃，未持久化到数据库。

---

## 三、改动清单

### 3.1 扩展 ActionProposal

**文件**：`crates/uncode-agent/src/decision/types.rs`

```rust
use uuid::Uuid;

/// 意图类型：LLM 调用工具的目的分类
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentType {
    FileRead,      // 读取文件/目录
    FileWrite,     // 创建/修改文件
    FileEdit,      // 精确编辑文件片段
    Search,        // 搜索代码/文本
    Execution,     // 执行命令
    WebAccess,     // 网络请求
    Unknown,       // 无法分类
}

/// 候选动作（多路裁决时的备选方案）
#[derive(Debug, Clone)]
pub struct Alternative {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub description: String,
}

/// 认知路径溯源
#[derive(Debug, Clone)]
pub struct CognitiveTrace {
    pub turn: u32,
    pub source: String,          // "streaming" | "retry" | "follow_up"
    pub llm_model: String,
}

pub struct ActionProposal {
    pub proposal_id: Uuid,
    pub tool_name: String,
    pub raw_arguments: serde_json::Value,
    pub intent: IntentType,
    pub rationale: Option<String>,
    pub confidence: Option<f32>,
    pub alternatives: Vec<Alternative>,
    pub trace: Vec<CognitiveTrace>,
}
```

**迁移策略**：所有现有构造点补全默认值：

```rust
ActionProposal {
    proposal_id: Uuid::new_v4(),
    tool_name: /* existing */,
    raw_arguments: /* existing */,
    intent: IntentType::from_tool_name(tool_name),
    rationale: /* existing */,
    confidence: /* existing */,
    alternatives: vec![],
    trace: vec![],
}
```

**IntentType 推断**：提供 `from_tool_name()` 方法，按工具名映射：

| tool_name | IntentType |
|:---|:---|
| read, find, ls, grep | FileRead |
| write | FileWrite |
| edit | FileEdit |
| bash | Execution |
| web_fetch, web_search | WebAccess |
| 其他 | Unknown |

**受影响的构造点**：
- `proposal.rs:63-68`：ProposalAccumulator 创建 ActionProposal 的唯一位置

### 3.2 新增决策生命周期事件

**文件**：`crates/uncode-core/src/event.rs`

在 `AgentEvent` enum 中新增四个变体：

```rust
/// 提案被接收进入决策管线
ProposalReceived {
    turn_id: String,
    proposal_id: String,
    tool_name: String,
    intent: String,
}

/// 防火墙检查完成（通过或拒绝）
FirewallCheck {
    turn_id: String,
    proposal_id: String,
    tool_name: String,
    passed: bool,
    stage: String,           // "parse" | "validate" | "normalize"
    violations: Vec<String>,
    duration_ms: u64,
}

/// 工具执行完成
ActionExecuted {
    turn_id: String,
    proposal_id: String,
    tool_name: String,
    success: bool,
    duration_ms: u64,
}

/// 审计记录已持久化
DecisionAudited {
    turn_id: String,
    proposal_id: String,
    tool_name: String,
    verdict_allowed: bool,
    persisted: bool,
}
```

**detail_level 分类**：
- `ProposalReceived` → Standard
- `FirewallCheck` → Standard
- `ActionExecuted` → Standard
- `DecisionAudited` → Critical

### 3.3 改造 loop_engine.rs 决策管线

**文件**：`crates/uncode-agent/src/loop_engine.rs`

#### 3.3.1 ProposalAccumulator 传递上下文

在 `ProposalAccumulator::feed()` 返回 `ActionProposal` 时注入 trace 信息：

```rust
// proposal.rs:63 处构造点
ActionProposal {
    proposal_id: Uuid::new_v4(),
    tool_name: name.clone(),
    raw_arguments: serde_json::from_str(&args).unwrap_or(serde_json::Value::Null),
    intent: IntentType::from_tool_name(&name),
    rationale: None,
    confidence: None,
    alternatives: vec![],
    trace: vec![CognitiveTrace {
        turn,  // 需要将 turn 信息传入 ProposalAccumulator
        source: "streaming".to_string(),
        llm_model: model_id.clone(),
    }],
}
```

ProposalAccumulator 需要新增 `turn` 和 `model_id` 字段来支持 trace 构建。

#### 3.3.2 发射细粒度事件

在 `run_inner()` 的决策管线中（`loop_engine.rs:1589-1650`），插入事件发射：

```rust
// 当前代码结构（简化）：
for proposal in proposals {
    // --- 新增：ProposalReceived ---
    self.emit(AgentEvent::ProposalReceived {
        turn_id: format!("turn-{turn}"),
        proposal_id: proposal.proposal_id.to_string(),
        tool_name: proposal.tool_name.clone(),
        intent: format!("{:?}", proposal.intent),
    });

    match firewall.process(&proposal) {
        Ok(normalized) => {
            // --- 新增：FirewallCheck passed ---
            self.emit(AgentEvent::FirewallCheck { ... passed: true ... });

            match adjudicator.adjudicate(&normalized, &decision_ctx) {
                Ok(_approved) => {
                    // 已有的 DecisionMade allowed=true
                    // ... 执行工具 ...
                    // --- 新增：ActionExecuted ---
                    self.emit(AgentEvent::ActionExecuted { ... success: ... });
                }
                Err(e) => {
                    // 已有的 DecisionMade allowed=false
                }
            }
        }
        Err(e) => {
            // --- 新增：FirewallCheck failed ---
            self.emit(AgentEvent::FirewallCheck { ... passed: false ... });
            // 已有的 DecisionMade allowed=false
        }
    }
}
```

#### 3.3.3 增强被拒绝提案的反馈

当前被拒绝的工具仅记录事件，不向 LLM 反馈拒绝原因。需要将拒绝信息作为 `tool_result` 回注对话流：

```rust
// denied 工具的处理（当前 loop_engine.rs:1681-1700 附近）
for audit in &pending_audits {
    if !audit.allowed {
        // 构造错误 tool_result 并注入 executions
        let error_result = ToolContent::text(format!(
            "Action denied by decision pipeline: {}",
            audit.reason.as_deref().unwrap_or("unknown reason")
        ));
        executions.push(Execution {
            tool_id: format!("denied-{}", audit.tool_name),
            tool_name: audit.tool_name.clone(),
            result: vec![error_result],
            terminate: false,
        });
    }
}
```

这样 LLM 在下一轮能看到工具被拒绝的原因，并据此调整行为。

### 3.4 审计记录持久化

**文件**：`crates/uncode-agent/src/decision/types.rs` + 新增持久化逻辑

#### 3.4.1 DecisionRecord 完善字段

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub id: String,                           // proposal_id
    pub turn_id: String,
    pub session_id: String,
    pub proposal: ActionProposal,
    pub firewall_result: Option<FirewallAudit>,
    pub verdict: DecisionVerdict,
    pub approved_action: Option<ApprovedAction>,
    pub execution_result: Option<ExecutionResult>,
    pub timestamp: DateTime<Utc>,
    pub adjudication_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallAudit {
    pub passed: bool,
    pub stage_failed: Option<String>,   // "parse" | "validate" | "normalize"
    pub violations: Vec<String>,
    pub normalized_fields: Vec<String>,
}
```

#### 3.4.2 SurrealDB 持久化

在 `persist_decision_audit()` 中将 `PendingAudit` 转为 `DecisionRecord` 并写入 SurrealDB：

```rust
async fn persist_decision_audit(
    db: &SurrealDb,
    session_id: &str,
    audits: &[PendingAudit],
    proposals: &[ActionProposal],
) -> Result<(), UncodeError> {
    for audit in audits {
        let record = DecisionRecord {
            id: audit.proposal_id.to_string(),  // 需要在 PendingAudit 中保留 proposal_id
            turn_id: audit.turn_id.clone(),
            session_id: session_id.to_string(),
            // ... 从 proposals 中查找匹配的 proposal
            timestamp: Utc::now(),
            adjudication_duration_ms: audit.duration_ms,
            // ...
        };
        db.create("decision_records")
            .content(record)
            .await?;
    }
    Ok(())
}
```

**PendingAudit 扩展**（新增 `proposal_id` 和 `firewall_result` 字段）：

```rust
struct PendingAudit {
    turn_id: String,
    proposal_id: Uuid,           // 新增
    tool_name: String,
    allowed: bool,
    reason: Option<String>,
    firewall_result: Option<FirewallAudit>,  // 新增
    duration_ms: u64,
}
```

---

## 四、测试计划

### 4.1 单元测试

| 测试 | 文件 | 验证点 |
|:---|:---|:---|
| `intent_type_from_tool_name` | `decision/types.rs` | 9 种工具名正确映射到 IntentType |
| `action_proposal_default_fields` | `decision/types.rs` | 新字段默认值正确（proposal_id 非空、alternatives 为空） |
| `decision_record_serialization` | `decision/types.rs` | DecisionRecord 可序列化/反序列化（SurrealDB 要求） |
| `firewall_audit_roundtrip` | `decision/types.rs` | FirewallAudit 序列化往返无损 |

### 4.2 集成测试

| 测试 | 验证点 |
|:---|:---|
| `events_emitted_in_order` | 一次工具调用按序产生 ProposalReceived → FirewallCheck → DecisionMade → ActionExecuted → DecisionAudited |
| `denied_proposal_feeds_back` | 被拒绝的工具产生包含拒绝原因的 tool_result，LLM 下一轮可见 |
| `audit_record_persisted` | DecisionRecord 写入 SurrealDB 后可按 session_id + turn_id 查询 |
| `proposal_has_trace` | ActionProposal 的 trace 字段包含正确的 turn、source、model 信息 |

---

## 五、文件变更总览

| 文件 | 改动类型 | 说明 |
|:---|:---|:---|
| `decision/types.rs` | 修改 | 新增 IntentType、Alternative、CognitiveTrace、FirewallAudit；扩展 ActionProposal、DecisionRecord、PendingAudit |
| `decision/proposal.rs` | 修改 | ProposalAccumulator 注入 proposal_id、intent、trace |
| `core/event.rs` | 修改 | 新增 4 个 AgentEvent 变体 |
| `loop_engine.rs` | 修改 | 插入细粒度事件发射；增强被拒绝反馈；审计持久化 |
| `decision/mod.rs` | 可能修改 | 如需导出新类型 |

**不需要新建文件**。所有改动在现有文件中完成。

---

## 六、风险与回滚

| 风险 | 缓解 |
|:---|:---|
| ActionProposal 新字段破坏现有构造点 | 新字段均有默认值，`IntentType::Unknown` 兜底 |
| 新事件变体影响下游消费者（TUI/Platform） | 新事件为附加信息，不影响已有事件处理逻辑 |
| 审计持久化失败影响核心路径 | 持久化失败仅 log warning，不阻塞工具执行 |
| SurrealDB schema 迁移 | DecisionRecord 使用 `db.create()` 动态写入，无需预建表 |
