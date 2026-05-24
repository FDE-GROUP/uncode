# Phase 3 工程设计：治理激活

> **对应重构方案**：`UNCODE_REFACTORING_PLAN.md` Phase 3
> **依赖**：Phase 0（GuardrailConfig 加载）、Phase 2（细粒度决策事件）
> **预计工期**：2-3 天

---

## 一、目标

将治理层从"类型已定义但未运行"推进到"运行时生效"：

1. EventRouter 接入 AgentLoop 主循环
2. 显式建模六状态 PhaseStateMachine
3. GuardrailConfig 约束在运行时实际生效（替换硬编码阈值）

---

## 二、现状分析

### 2.1 EventRouter（`core/event.rs:457-552`）

已实现完整的路由基础设施：

```rust
pub struct EventRouter {
    sync_handlers: HashMap<String, Vec<SyncEventHandler>>,
    hook_handlers: HashMap<String, Vec<AsyncHookHandler>>,
}
```

- `on()` / `on_hook()` 注册处理器
- `dispatch()` 同步分发给观察者
- `dispatch_hooks()` 异步分发给控制流钩子（可 Block/Patch）
- 单元测试覆盖了三种场景（`tests.rs:302-354`）

**当前状态**：定义在 `uncode-core`，测试通过，但 `uncode-agent` 和 `uncode-cli` 中零引用。

### 2.2 AgentHarnessPhase（`harness.rs:49-68`）

```rust
pub enum AgentHarnessPhase {
    Idle,           // 等待新 turn
    Turn,           // 正在处理
    Compaction,     // 上下文压缩
    BranchSummary,  // 摘要/分析阶段
    Retry,          // 自动恢复
}
```

映射到 `PhaseGuardPolicy`：仅 Idle/Turn 阶段允许工具执行。

**缺失**：范式定义的六状态（Init → Cognizing → Adjudicating → Executing → WaitingForUser → Terminated）未显式建模。当前五状态中，Turn 阶段同时涵盖认知、裁决、执行三个子阶段，无法区分。

### 2.3 GuardrailConfig（`shared/guardrails.rs`）

完整配置结构：

| 配置段 | 字段 | 当前状态 |
|:---|:---|:---|
| DecisionConfig | turn_limit, max_concurrent_tools, tool_timeout_seconds | 未加载，MAX_TURNS 硬编码为 50 |
| FirewallConfig | path_safety, tool_whitelist, resource_limits | 未加载 |
| AdjudicationConfig | policies[]（name, enabled, rules[]） | 未加载 |
| AuditConfig | event_levels, retention | 未加载 |

---

## 三、改动清单

### 3.1 EventRouter 接入 AgentLoop

**文件**：`crates/uncode-agent/src/loop_engine.rs`、`crates/uncode-core/src/event.rs`

#### 3.1.1 AgentLoop 新增 EventRouter 字段

```rust
// loop_engine.rs AgentLoop struct
pub struct AgentLoop {
    // ... 现有字段 ...
    event_router: std::sync::Mutex<EventRouter>,  // 新增
}
```

#### 3.1.2 构造时初始化 EventRouter

```rust
// AgentLoop::new() 中
event_router: std::sync::Mutex::new(EventRouter::new()),
```

#### 3.1.3 emit 方法扩展

当前 `emit()` 仅 broadcast 到 channel。扩展为同时路由到 EventRouter：

```rust
fn emit(&self, event: AgentEvent) {
    // 现有 broadcast（不变）
    let _ = self.event_sender.send(event.clone());

    // 新增：EventRouter 路由
    let router = self.event_router.lock().unwrap();
    router.dispatch(&event);
    // 注意：dispatch_hooks 是 async，不能在 sync 上下文中调用。
    // hook dispatch 放到专门的 tokio::spawn 中处理。
}
```

异步 hook 分发需要特殊处理——`emit()` 是同步方法，不能直接 await。两种方案：

**方案 A（推荐）**：hook 分发由 harness 层驱动，不在 AgentLoop 内部。

```rust
// harness.rs 的 run_turn 中，subscribe 事件流并异步分发 hooks
while let Ok(event) = rx.recv().await {
    let router = self.agent.event_router.lock().unwrap();
    let hook_results = router.dispatch_hooks(&event).await;
    // 处理 HookResult::Block / PatchMessages / PatchToolResult
}
```

**方案 B**：使用 tokio channel 缓冲事件，后台 task 消费并 dispatch_hooks。

选择方案 A，因为 harness 已经有事件订阅机制（`subscribe()`），无需引入新的并发原语。

#### 3.1.4 EventRouter 注册点

在 `AgentHarness::new()` 或 `build()` 中注册核心处理器：

```rust
// SurrealDB 持久化处理器
router.on("DecisionMade", Box::new(|event| {
    // 写入 SurrealDB decision_events 表
}));

router.on("DecisionAudited", Box::new(|event| {
    // 写入 SurrealDB audit_trail 表
}));

// TUI 渲染通过现有的 broadcast channel 已实现，无需额外注册

// Extension 钩子（Phase 3 暂不实现，预留接口）
// router.on_hook("ToolCallStart", Box::new(|event| { ... }));
```

### 3.2 PhaseStateMachine 显式建模

**新文件**：`crates/uncode-agent/src/governance/mod.rs`、`state_machine.rs`

#### 3.2.1 六状态定义

```rust
// governance/state_machine.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentPhase {
    Init,            // 初始化
    Cognizing,       // LLM 正在生成（接收流式输出）
    Adjudicating,    // 防火墙 + 裁决器运行
    Executing,       // 工具正在执行
    WaitingForUser,  // 等待用户输入
    Terminated,      // 会话结束
}

#[derive(Debug, Clone)]
pub struct PhaseTransition {
    pub from: AgentPhase,
    pub to: AgentPhase,
    pub timestamp: DateTime<Utc>,
    pub trigger: String,      // 触发转换的事件描述
}

pub struct PhaseStateMachine {
    current: AgentPhase,
    history: Vec<PhaseTransition>,
}

// 合法转换表
static ALLOWED_TRANSITIONS: &[(AgentPhase, &[AgentPhase])] = &[
    (Init,            &[Cognizing]),
    (Cognizing,       &[Adjudicating, WaitingForUser, Terminated]),
    (Adjudicating,    &[Executing, Cognizing, Terminated]),
    (Executing,       &[Cognizing, WaitingForUser, Terminated]),
    (WaitingForUser,  &[Cognizing, Terminated]),
    (Terminated,      &[]),
];
```

#### 3.2.2 转换方法

```rust
impl PhaseStateMachine {
    pub fn new() -> Self {
        Self {
            current: AgentPhase::Init,
            history: Vec::new(),
        }
    }

    pub fn current(&self) -> AgentPhase {
        self.current
    }

    pub fn transition(&mut self, to: AgentPhase, trigger: &str) -> Result<(), GovernanceError> {
        let allowed = ALLOWED_TRANSITIONS
            .iter()
            .find(|(from, _)| *from == self.current)
            .map(|(_, targets)| targets.contains(&to))
            .unwrap_or(false);

        if !allowed {
            return Err(GovernanceError::InvalidPhaseTransition {
                from: self.current,
                to,
            });
        }

        self.history.push(PhaseTransition {
            from: self.current,
            to,
            timestamp: Utc::now(),
            trigger: trigger.to_string(),
        });
        self.current = to;
        Ok(())
    }

    pub fn history(&self) -> &[PhaseTransition] {
        &self.history
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("invalid phase transition: {from:?} → {to:?}")]
    InvalidPhaseTransition {
        from: AgentPhase,
        to: AgentPhase,
    },
}
```

#### 3.2.3 与 AgentHarnessPhase 的关系

PhaseStateMachine **不替代** `AgentHarnessPhase`，而是作为它的子状态。映射关系：

| AgentHarnessPhase | 子阶段 AgentPhase |
|:---|:---|
| Idle | WaitingForUser |
| Turn | Cognizing → Adjudicating → Executing（循环） |
| Compaction | Executing（compaction 作为特殊工具执行） |
| BranchSummary | Executing |
| Retry | Cognizing |

`PhaseGuardPolicy` 仍然基于 `AgentHarnessPhase` 判断。`AgentPhase` 为内部可观测性服务——事件、日志、TUI 状态展示。

#### 3.2.4 集成到 harness

```rust
// harness.rs AgentHarness struct 新增字段
phase_machine: std::sync::Mutex<PhaseStateMachine>,
```

在关键点调用 `phase_machine.transition()`：

```rust
// prompt() 开始时
self.phase_machine.lock().unwrap().transition(AgentPhase::Cognizing, "user_prompt")?;

// loop_engine 开始处理 tool call 时
phase_machine.transition(AgentPhase::Adjudicating, "tool_call_received")?;

// 裁决通过、开始执行时
phase_machine.transition(AgentPhase::Executing, "adjudication_approved")?;

// 执行完成、回到流式接收时
phase_machine.transition(AgentPhase::Cognizing, "tool_execution_complete")?;

// turn 结束等待下一轮输入时
phase_machine.transition(AgentPhase::WaitingForUser, "turn_complete")?;
```

非法转换返回 `GovernanceError`，当前阶段仅 log warning，不阻塞执行。

### 3.3 GuardrailConfig 运行时生效

**文件**：`crates/uncode-agent/src/loop_engine.rs`、`crates/uncode-agent/src/decision/adjudication.rs`、`crates/uncode-agent/src/decision/firewall.rs`

#### 3.3.1 DecisionConfig.turn_limit 替换硬编码

**当前**：`loop_engine.rs` 中 `MAX_TURNS: u32 = 50` 硬编码。

**改为**：从 GuardrailConfig 读取。

```rust
// loop_engine.rs
// 保留 MAX_TURNS 作为默认值
const DEFAULT_MAX_TURNS: u32 = 50;

// AgentLoop 新增字段
guardrail_config: GuardrailConfig,

// 在 run_inner 中使用
let max_turns = if self.guardrail_config.decision.turn_limit > 0 {
    self.guardrail_config.decision.turn_limit
} else {
    DEFAULT_MAX_TURNS
};
```

`TurnLimitPolicy` 同步修改，从构造时注入的 `max_turns` 而非 `MAX_TURNS` 常量。

#### 3.3.2 FirewallConfig.path_safety 驱动 PathSafetyRule

**当前**：`PathSafetyRule`（`firewall.rs:221-297`）硬编码 CWD-only 策略。

**改为**：根据 `GuardrailConfig.firewall.path_safety.mode` 行为：

```rust
pub enum PathSafetyMode {
    CwdOnly,       // 现有行为：仅允许 CWD 内路径
    AllowList,     // CWD + allow_list 中的路径
    Unrestricted,  // 不限制（危险，仅用于测试）
}
```

在 `build_default_firewall()` 中根据 config 选择：

```rust
let path_rule = match &config.firewall.path_safety.mode {
    PathSafetyMode::CwdOnly => PathSafetyRule::cwd_only(cwd),
    PathSafetyMode::AllowList => PathSafetyRule::with_allow_list(cwd, &config.firewall.path_safety.allow_list),
    PathSafetyMode::Unrestricted => PathSafetyRule::unrestricted(),
};
```

#### 3.3.3 FirewallConfig.tool_whitelist 驱动工具过滤

**当前**：所有已注册工具均可被 LLM 调用。

**改为**：

```rust
pub enum ToolWhitelistMode {
    Builtin,  // 仅内置工具
    Custom,   // 仅扩展工具
    All,      // 所有工具（默认）
}
```

在 `ToolRegistry::active_tool_names()` 中排除 `blocked` 列表中的工具，并根据 `mode` 过滤。

#### 3.3.4 FirewallConfig.resource_limits 驱动执行限制

**当前**：`max_file_size_mb` 和 `max_bash_output_lines` 在工具执行器中各自硬编码默认值。

**改为**：从 `GuardrailConfig.firewall.resource_limits` 注入。

```rust
// 在 read 工具中
let max_size = resource_limits
    .map(|r| r.max_file_size_mb as u64 * 1024 * 1024)
    .unwrap_or(DEFAULT_MAX_FILE_SIZE);

// 在 bash 工具中
let max_lines = resource_limits
    .map(|r| r.max_bash_output_lines as usize)
    .unwrap_or(DEFAULT_MAX_BASH_LINES);
```

通过 `ToolHooks.after()` 统一执行资源限制检查，而非在每个工具内部单独处理。

#### 3.3.5 AdjudicationConfig 驱动裁决策略

**当前**：四个内置 Policy（PhaseGuard、TurnLimit、Cancellation、Concurrency）硬编码。

**改为**：

```rust
// 在 build_adjudicator() 中
fn build_adjudicator(config: &GuardrailConfig, phase_guard: Arc<PhaseGuardPolicy>) -> Adjudicator {
    let mut policies: Vec<Box<dyn DecisionPolicy>> = Vec::new();

    // PhaseGuard 始终存在
    policies.push(Box::new(PhaseGuardPolicy::clone_from(&phase_guard)));

    // 从配置中读取策略列表
    for policy_config in &config.adjudication.policies {
        if !policy_config.enabled {
            continue;
        }
        match policy_config.name.as_str() {
            "turn_limit" => policies.push(Box::new(TurnLimitPolicy::new(
                config.decision.turn_limit,
            ))),
            "cancellation" => policies.push(Box::new(CancellationPolicy::new(
                CancellationToken::new(),
            ))),
            "concurrency" => policies.push(Box::new(ConcurrencyPolicy::new())),
            // 自定义规则从 PolicyRule[] 构建
            _ => {
                let custom = CustomPolicy::from_config(policy_config);
                policies.push(Box::new(custom));
            }
        }
    }

    Adjudicator::new(policies)
}
```

**CustomPolicy**（新增）：

```rust
pub struct CustomPolicy {
    name: String,
    rules: Vec<PolicyRule>,
}

impl DecisionPolicy for CustomPolicy {
    fn adjudicate(&self, action: &NormalizedAction, ctx: &DecisionContext) -> Result<ApprovedAction, DecisionError> {
        for rule in &self.rules {
            if rule.pattern_matches(&action.tool_name) {
                match rule.action {
                    PolicyAction::Block => return Err(DecisionError::BlockedByPolicy {
                        policy: self.name.clone(),
                        rule: rule.pattern.clone(),
                    }),
                    PolicyAction::BlockAndWarn => {
                        // log warning + block
                        return Err(DecisionError::BlockedByPolicy { ... });
                    }
                    PolicyAction::AskUser => {
                        // 触发 PermissionGate（需与现有 permission_gate.rs 集成）
                        // 暂时按 Block 处理
                        return Err(DecisionError::RequiresUserApproval { ... });
                    }
                    PolicyAction::Allow => continue,
                }
            }
        }
        // 所有规则检查通过
        Ok(ApprovedAction { ... })
    }
}
```

---

## 四、测试计划

### 4.1 单元测试

| 测试 | 文件 | 验证点 |
|:---|:---|:---|
| `phase_machine_valid_transitions` | `governance/state_machine.rs` | Init→Cognizing→Adjudicating→Executing→WaitingForUser 全链路通过 |
| `phase_machine_invalid_transition` | `governance/state_machine.rs` | Init→Executing 返回 InvalidPhaseTransition 错误 |
| `phase_machine_terminated_no_exit` | `governance/state_machine.rs` | Terminated 状态不接受任何转换 |
| `path_safety_cwd_only` | `decision/firewall.rs` | 现有测试不回归 |
| `path_safety_allow_list` | `decision/firewall.rs` | allow_list 中的路径可访问 |
| `path_safety_unrestricted` | `decision/firewall.rs` | 不限制路径 |
| `tool_whitelist_blocked` | `loop_engine.rs` | blocked 工具不出现在 LLM 的 tool 列表中 |
| `custom_policy_block` | `decision/adjudication.rs` | 自定义规则阻止匹配工具 |
| `custom_policy_allow` | `decision/adjudication.rs` | 自定义规则放行匹配工具 |
| `guardrail_config_default_turn_limit` | `loop_engine.rs` | turn_limit=0 时使用 DEFAULT_MAX_TURNS |

### 4.2 集成测试

| 测试 | 验证点 |
|:---|:---|
| `event_router_dispatch_sync` | sync_handler 接收到所有决策事件 |
| `event_router_hook_block` | hook_handler 返回 Block 时工具不执行 |
| `guardrail_config_drives_firewall` | YAML 中的 path_safety.mode=CwdOnly 在运行时生效 |
| `guardrail_config_drives_turn_limit` | YAML 中 turn_limit=3 时第 4 turn 被拒绝 |
| `phase_machine_tracks_full_session` | 完整会话的 phase history 按预期记录所有转换 |

---

## 五、文件变更总览

| 文件 | 改动类型 | 说明 |
|:---|:---|:---|
| `governance/mod.rs` | 新建 | 模块入口 |
| `governance/state_machine.rs` | 新建 | PhaseStateMachine + AgentPhase + PhaseTransition |
| `harness.rs` | 修改 | 新增 phase_machine 字段，在关键点调用 transition() |
| `loop_engine.rs` | 修改 | 新增 event_router 字段；guardrail_config 字段；max_turns 从 config 读取；emit() 扩展路由 |
| `decision/firewall.rs` | 修改 | PathSafetyRule 支持三种模式；build_default_firewall() 接受 GuardrailConfig |
| `decision/adjudication.rs` | 修改 | 新增 CustomPolicy；build_adjudicator() 从 GuardrailConfig 构建 |
| `decision/mod.rs` | 修改 | 导出新增模块 |

---

## 六、风险与回滚

| 风险 | 缓解 |
|:---|:---|
| PhaseStateMachine 转换失败阻塞核心路径 | 转换失败仅 log warning，不返回错误到上层 |
| EventRouter sync handler 异常 | dispatch() 内部 catch_unwind，单个 handler 失败不影响其他 |
| GuardrailConfig 缺失字段 | 所有字段有 Default 实现，缺失时回退到当前硬编码行为 |
| PathSafetyRule Unrestricted 模式安全性 | 仅在 `#[cfg(test)]` 或显式配置时启用，CLI 默认 CwdOnly |
| CustomPolicy AskUser 与 PermissionGate 集成 | 第一版按 Block 处理，AskUser 完整集成作为后续迭代 |

---

## 七、与 Phase 2 的依赖关系

Phase 3 依赖 Phase 2 产生的细粒度事件（ProposalReceived、FirewallCheck 等）。如果 Phase 2 未完成：

- EventRouter 仍可接入，但只能路由已有事件（DecisionMade、ToolCallStart/End 等）
- PhaseStateMachine 不依赖 Phase 2，可独立开发
- GuardrailConfig 运行时生效不依赖 Phase 2

三个子任务（EventRouter / PhaseStateMachine / GuardrailConfig）相互独立，可并行推进。
