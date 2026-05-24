# Phase 5c 工程设计：认知层→决策层反馈增强

> **依赖**：Phase 2（TurnFeedback + record_feedback）
> **预计工期**：0.5 天

---

## 一、目标

在 TurnFeedback 中追加 decision summary，通过 record_feedback() 注入 WorkingMemory，使认知层感知到"哪些工具被拒绝、为什么"。

---

## 二、改动清单

### 2.1 新增 DecisionSummary 结构体

**文件**：`crates/uncode-agent/src/decision/feedback.rs`

```rust
#[derive(Debug, Clone, Default)]
pub struct DecisionSummary {
    pub tools_approved: Vec<String>,
    pub tools_denied: Vec<String>,
    pub denial_reasons: Vec<String>,
    pub firewall_violations: Vec<String>,
}
```

### 2.2 TurnFeedback 新增 decision_summary 字段

```rust
pub struct TurnFeedback {
    pub turn_number: u32,
    pub observations: Vec<String>,
    pub agent_steps: Vec<AgentStep>,
    pub evaluation: Option<TurnEvaluation>,
    pub decision_summary: Option<DecisionSummary>,  // 新增
}
```

### 2.3 loop_engine.rs — turn 结束时填充 DecisionSummary

在 record_feedback 调用之前，从本 turn 的 denied_results 提取信息。

### 2.4 feedback.rs — record_feedback 增强

当 decision_summary 存在且有 denied/violations 时，写入 WorkingMemory 的 observe_important()。

---

## 三、文件变更

| 文件 | 改动 |
|------|------|
| `decision/feedback.rs` | DecisionSummary 结构体 + TurnFeedback 扩展 + record_feedback 增强 |
| `loop_engine.rs` | turn 结束时填充 decision_summary |

---

## 四、不做的事

- 不改 UncertaintyClassifier
- 不改 PromptManager（WorkingMemory 自动进入 context）
- 不改 EpisodeMemory 模式识别
