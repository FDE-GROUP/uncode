# UNCODE_SEMANTIC_FIREWALL — 语义防火墙设计文档

> **范式**：认知与决策驱动设计（`docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3）
> **实现层定位**：`crates/uncode-agent/src/decision/firewall.rs`

---

## 定位

语义防火墙是认知层与决策层之间的**唯一通道**。

认知层永远不知道决策层的裁决逻辑。
决策层永远不接触认知层的自然语言。
唯一的握手协议是结构化命令 + 结构化反馈。

---

## 三层管线

```
ActionProposal (LLM 原始输出)
  │
  ▼
┌─────────────────────────────────────────────┐
│ Parser (ParseStrategy trait)                │
│ DefaultParser: 原样通过 raw_arguments        │
│ → ParsedAction                              │
└──────────────┬──────────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────────┐
│ Validator (ValidationRule trait chain)      │
│ ┌───────────────────────────────────────┐   │
│ │ SchemaCoercionRule                    │   │
│ │ 包装 ToolRegistry::prepare_and_validate()│  │
│ │ ← JSON Schema 校验 + 类型自动转换       │   │
│ ├───────────────────────────────────────┤   │
│ │ PathSafetyRule                        │   │
│ │ 复现 tools/mod.rs resolve_path()       │   │
│ │ ← CWD sandbox + 路径规范化             │   │
│ ├───────────────────────────────────────┤   │
│ │ PermissionPolicyRule                  │   │
│ │ 包装 tool_permission::PermissionPolicy │   │
│ │ ← 危险命令检测 + 受保护路径检查         │   │
│ └───────────────────────────────────────┘   │
│ → ValidatedAction + ValidationVerdict       │
└──────────────┬──────────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────────┐
│ Normalizer (NormalizeStrategy trait)        │
│ DefaultNormalizer: 原样通过                  │
│ → NormalizedAction（消歧义后最终形式）        │
└─────────────────────────────────────────────┘
               │
               ▼
         进入决策层 adjudication
```

---

## 核心 Trait

| Trait | 方法 | 输入 → 输出 |
|:---|:---|:---|
| `ParseStrategy` | `parse(&self, raw) -> ParsedAction` | `ActionProposal` → `ParsedAction` |
| `ValidationRule` | `validate(&self, action) -> ValidationVerdict` | `ParsedAction` → `ValidationVerdict` |
| `NormalizeStrategy` | `normalize(&self, action) -> NormalizedAction` | `ValidatedAction` → `NormalizedAction` |

---

## 包装策略

`ValidationRule` 实现不重写逻辑——它们**包装**现有安全基础设施：

| ValidationRule | 包装的现有组件 | 文件位置 |
|:---|:---|:---|
| `SchemaCoercionRule` | `ToolRegistry::prepare_and_validate()` | `tools/registry.rs` |
| `PathSafetyRule` | `resolve_path()` 逻辑复现 | `tools/mod.rs` |
| `PermissionPolicyRule` | `PermissionPolicy::needs_confirmation()` | `tool_permission.rs` |

---

## 快速构建

```rust
let firewall = build_default_firewall(
    Arc::new(PermissionPolicy::default_policy()),
    Arc::new(ToolRegistry::new()),
    std::env::current_dir().unwrap(),
);
// 验证顺序：Schema → Path → Permission
```

---

## 可测试性

每个 `ValidationRule` 可独立单元测试。防火墙管线可用 mock Parser/Normalizer 做集成测试。

当前测试覆盖：
- DefaultParser 透传
- PermissionPolicyRule 阻断 rm -rf / 允许 ls
- PathSafetyRule 阻断 traversal / 允许非文件工具
- 完整管线：危险 bash 被拒绝
