# UNCODE_COGNITION_LAYER — 认知层设计文档

> **范式**：认知显化与决策驱动设计（`docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3）
> **实现层定位**：uncode 的认知层实现——源文件 `crates/uncode-agent/src/cognition/`

---

## 定位

认知层回答一个问题："**接下来可以做什么？**"

它负责理解任务、召回知识、推理方案、生成候选行动。
认知层的输出是 `ActionProposal`——候选方案，而非最终命令。
最终合法性判定由决策层（`decision/`）负责。

认知层可以换模型、换提示词、换推理策略——只要输出协议不变，决策层不感知。

---

## 模块结构

| 模块 | 文件 | 职责 |
|:---|:---|:---|
| 上下文构建 | `cognition/context_builder.rs` | re-export → `crate::context_builder::build_context()` |
| 提示词管理 | `cognition/prompt_manager.rs` | `PromptManager` 包装 `SystemPromptBuilder` |
| 不确定性管理 | `cognition/uncertainty.rs` | `UncertaintyClass` 三分类 + `from_error_category()` |
| 认知记忆 | `cognition/memory.rs` | `MemoryManager` + `CompactionDecision` |

---

## UncertaintyClass — 三分类模型

按不确定性**性质**分类，与按来源分类的 `ErrorCategory` 互补：

| 类型 | 来源 | 适配策略 |
|:---|:---|:---|
| `Generative` | LLM 采样 | `Rerank` / `MajorityVote` / `BestOfN` / `SingleSample` |
| `Cognitive` | 上下文不足 | `ContextRequirement` 枚举 + 补充检索 |
| `Executional` | 工具/外部系统 | `Retry` / `FallbackTool` / `Compensate` / `Escalate` |

`from_error_category()` 从现有 `ErrorCategory::Llm|Tool|Network|Config` 映射到对应类型。

---

## MemoryManager — 压缩决策

| 函数 | 输出 | 触发条件 |
|:---|:---|:---|
| `evaluate(current, max)` | `CompactionDecision` | 阈值百分比 + 溢出检测 |
| | `Noop` | < 阈值，无溢出 |
| | `ShouldCompact` | >= 阈值 |
| | `ForceCompact` | 当前 + reserve > max |

---

## PromptManager

包装 `SystemPromptBuilder`，提供 builder 模式：
```rust
PromptManager::new()
    .with_base("You are a coding assistant.")
    .with_tool_guide(&active_tools)
    .with_context(&workspace_info)
    .build()
```

---

## 与决策层的接口

```
认知层 (cognition/)          决策层 (decision/)
┌─────────────────┐          ┌─────────────────┐
│ ActionProposal  │ ──→     │ proposal.rs     │
│ (候选方案)       │          │ (提案接收)       │
│                 │ ←──     │                 │
│ UncertaintyClass│  事件    │ DecisionMade    │
│ (性质分类)       │  反馈    │ AgentStep       │
└─────────────────┘          └─────────────────┘
```

认知层永远不知道决策层的裁决逻辑。
决策层永远不接触认知层的自然语言。
**唯一的握手协议是结构化命令 + 结构化反馈。**
