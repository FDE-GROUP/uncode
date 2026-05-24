# Phase 5a 工程设计：GuardrailConfig 运行时生效

> **对应重构方案**：`UNCODE_PHASE3_GOVERNANCE_ACTIVATION.md` §3.3（部分已完成）
> **依赖**：Phase 3（GuardrailConfig 结构 + 加载机制）
> **预计工期**：0.5 天

---

## 一、目标

将 `GuardrailConfig` 中已定义但未接入的字段在运行时生效：

1. CustomPolicy 从配置文件加载到 Adjudicator
2. 删除 `MAX_TURNS` 硬编码常量，统一使用 config fallback

---

## 二、现状分析

### 已接入

| 配置段 | 字段 | 状态 |
|:---|:---|:---|
| DecisionConfig | turn_limit | ✅ harness.rs 读取，传入 build_default_adjudicator |
| FirewallConfig | path_safety | ✅ build_firewall_from_config() |
| FirewallConfig | tool_whitelist | ✅ 自动放行逻辑 |

### 未接入

| 配置段 | 字段 | 当前行为 |
|:---|:---|:---|
| AdjudicationConfig | policies[] | CustomPolicy 代码已实现，但未在 harness 中加载 |
| DecisionConfig | MAX_TURNS 常量 | loop_engine.rs 硬编码 `50` |
| DecisionConfig | tool_timeout_seconds | 未使用（需 tokio::timeout，本次不做） |
| FirewallConfig | resource_limits | 工具内部硬编码（本次不做） |
| AuditConfig | 全部 | 无消费者（本次不做） |

---

## 三、改动清单

### 3.1 Adjudicator.add_policy()

**文件**：`crates/uncode-agent/src/decision/adjudication.rs`

```rust
impl Adjudicator {
    pub fn add_policy(&mut self, policy: Box<dyn DecisionPolicy>) {
        self.policies.push(policy);
    }
}
```

### 3.2 Harness 加载 CustomPolicy

**文件**：`crates/uncode-agent/src/harness.rs`

在 `AgentHarness::new()` 中，`agent.set_adjudicator(adjudicator)` 之后：

```rust
// 从 GuardrailConfig 加载 CustomPolicy
let gc = agent.guardrail_config();
if gc.adjudication.enabled {
    let mut adj = agent.adjudicator.lock().unwrap();
    if let Some(ref mut adjudicator) = *adj {
        for policy_config in &gc.adjudication.policies {
            if policy_config.enabled {
                adjudicator.add_policy(Box::new(
                    crate::decision::adjudication::CustomPolicy::from_config(policy_config),
                ));
            }
        }
    }
}
```

需确认 `agent.adjudicator` 字段可访问。当前 `adjudicator` 是 `std::sync::Mutex<Option<Adjudicator>>`，已在 harness.rs 中通过 `agent.set_adjudicator()` 设置。需新增 getter 或将字段改为 pub(crate)。

### 3.3 删除 MAX_TURNS 常量

**文件**：`crates/uncode-agent/src/loop_engine.rs`

删除 `pub const MAX_TURNS: u64 = 50;`。

已有代码在 adjudication 段读取 guardrail_config：
```rust
let max_turns = if gc.decision.turn_limit > 0 {
    gc.decision.turn_limit
} else {
    crate::loop_engine::MAX_TURNS as u32
};
```

改为：
```rust
let max_turns = if gc.decision.turn_limit > 0 {
    gc.decision.turn_limit as u64
} else {
    50
};
```

同时检查 `loop_engine.rs` 中其他引用 `MAX_TURNS` 的位置，全部替换为 config 读取或 fallback 50。

---

## 四、文件变更总览

| 文件 | 改动类型 | 说明 |
|:---|:---|:---|
| `decision/adjudication.rs` | 修改 | 新增 `add_policy()` |
| `harness.rs` | 修改 | new() 中加载 CustomPolicy |
| `loop_engine.rs` | 修改 | 删除 MAX_TURNS 常量，所有引用改为 fallback 50 |

---

## 五、不做的事

| 项目 | 原因 |
|:---|:---|
| 资源限制注入工具执行器 | 涉及工具实例重构，风险较高 |
| tool_timeout | 需 tokio::time::timeout 包装 execute_single_tool |
| audit config | 无明确消费者 |
