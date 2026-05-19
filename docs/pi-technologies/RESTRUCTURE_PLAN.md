# Crate 重组计划：对齐 Pi 架构分层

> **存档说明**：本文为 **重组过程的历史方案**（早期 11 crate → 目标 7 crate 的推演）。**当前仓库**以根目录 `Cargo.toml` 为准：存在 **`uncode-ai`**，**不存在** 独立成员 `uncode-llm`、`uncode-session`、`uncode-tools`。与 Pi 的对齐叙事与「逻辑会话 / SurrealDB」以 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) 与 [`../uncode-technologies/`](../uncode-technologies/) 为准。

## 背景

Pi 项目（github.com/earendil-works/pi）的 5 包结构精确反映了其三层架构：

```
pi-ai (LLM 抽象)           ← 无内部依赖
  ↓
pi-agent-core (Agent 引擎)  ← 依赖 pi-ai
  ↓
pi-coding-agent (应用层)    ← 依赖 pi-ai + pi-agent-core + pi-tui
```

当前 uncode 的 11 crate 结构与系统架构存在三个核心偏差：

1. **`uncode-core` 是万能垃圾桶** — 11 个不相关模块（LLM 类型、Agent 类型、Config、Skill、Template）混在一个 crate 中
2. **过度拆分弱化了 Agent 整体性** — session、tools、extensions 作为独立 crate，但只被 agent 消费，拆分无复用收益
3. **依赖方向不反映架构层次** — uncode-core 既包含 LLM 类型也包含 Agent 类型，但 LLM 层和 Agent 层应在不同抽象层级

## 目标

从 11 crate 重组为 7 crate，使 crate 边界精确映射架构层次：

```
重组前 (11 crates)                          重组后 (7 crates)
──────────────────                          ──────────────────
uncode-core (11模块混合)                    uncode-ai (LLM + 消息 + 模型)
uncode-macros                    ──→        uncode-macros (不变)
uncode-llm                       ──↗        uncode-agent (引擎全栈：loop + harness +
uncode-session                   ──→              session + compaction + tools + skills +
uncode-tools                     ──→              templates + events + error)
uncode-extensions                            uncode-extensions (不变)
uncode-agent                     ──↗
uncode-tui                       ──→        uncode-tui (不变)
uncode-platform                  ──→        uncode-platform (不变)
uncode-rpc                                   uncode-rpc (不变)
uncode-cli                       ──→        uncode-cli (不变)
```

## 依赖分析基础

### uncode-core 内部模块依赖图

```
config ─────────────────────────────────┐
context ────────────────────────────────┤ 无依赖（基础层）
error ──────────────────────────────────┤
skill ──────────────────────────────────┤
template ───────────────────────────────┘
    ↓
message ────────────────────────────────┐ 无内部依赖，被多方引用
    ↓                                   │
tool ← error                            │ 单向依赖
    ↓                                   │
api_types ← message, tool               │
    ↓                                   │
model ← api_types, config               ┘
    ↓
session ← api_types, message
event ← message, tool
```

### 关键发现：可三组干净拆分

| 组别 | 模块 | 内部依赖 |
|------|------|---------|
| **AI 类型** | message, api_types, model | api_types → message, tool；model → api_types, config |
| **Agent 类型** | tool, session, event, skill, template | tool → error；session → message, api_types；event → message, tool |
| **共享基础** | error, config, context | 无内部依赖 |

跨组依赖仅有：
- `model`（AI 类型）→ `config::UserModelConfig`（共享基础）
- `session`（Agent 类型）→ `api_types::ThinkingLevel`（AI 类型）
- `event`（Agent 类型）→ `message`（AI 类型）, `tool`（Agent 类型）

### 外部 crate 对 uncode-core 类型的消费

| 消费者 | 消费的类型 | 归属组 |
|--------|-----------|--------|
| **uncode-llm** | Message, Context, StreamOptions, ThinkingLevel, Model, UsageInfo, UncodeError | AI + 共享 |
| **uncode-session** | SessionEntry, SessionHeader, Message, ThinkingLevel, UncodeError | Agent + AI + 共享 |
| **uncode-tools** | ToolExecutor, ToolDefinition, ExecutionMode, ToolContext, UncodeError | Agent + 共享 |
| **uncode-agent** | 以上全部 | 全部 |
| **uncode-tui** | AgentEvent, UsageInfo, TemplateStore, SkillRegistry | Agent + AI |

## 重组方案

### Phase 1: 创建 `uncode-ai` crate

**目标**: LLM 抽象层，对应 `pi-ai`

**内容**（从 uncode-core 迁移 + 合并 uncode-llm）：

```
uncode-ai/
├── Cargo.toml
└── src/
    ├── lib.rs              # 统一导出
    ├── types.rs            # 从 core/message.rs 迁移
    ├── api_types.rs        # 从 core/api_types.rs 迁移
    ├── model.rs            # 从 core/model.rs 迁移
    ├── stream.rs           # 从 llm/stream.rs 迁移
    ├── api.rs              # 从 llm/api.rs 迁移（Api trait, StreamEvent）
    ├── api_registry.rs     # 从 llm/api_registry.rs 迁移
    ├── model_registry.rs   # 从 llm/model_registry.rs 迁移
    └── providers/
        ├── mod.rs
        ├── anthropic_messages.rs
        ├── gemini_generative.rs
        ├── ollama_native.rs
        └── openai_completions.rs
```

**依赖**：仅 `uncode-shared`（Phase 2 创建）+ 外部依赖

**需要从 api_types.rs 剥离的 Agent 类型**：
- `ToolDefinition` 引用 → 保留在 api_types 中的最小引用，或改用 `String` 类型标注
- `Context` 中的 `ToolDefinition` 字段 → 改为泛型或延迟绑定

### Phase 2: 创建 `uncode-shared` crate（替代原 uncode-core 的基础层）

**目标**: 零业务语义的共享基础设施

```
uncode-shared/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── error.rs            # 从 core/error.rs 迁移
    └── config.rs           # 从 core/config.rs 迁移
```

**依赖**：无 uncode 内部依赖（叶 crate）

**导出**：UncodeError 全家族、AppConfig、ModelConfig 等

### Phase 3: 重组 `uncode-agent`（合并 core Agent 类型 + session + tools + 原 agent）

**目标**: Agent 引擎全栈，对应 `pi-agent-core`

```
uncode-agent/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── types.rs            # 从 core 汇聚：tool + session + event + message Agent 变体
    ├── loop_engine.rs      # 不变
    ├── harness.rs          # 不变
    ├── system_prompt.rs    # 不变
    ├── compaction.rs       # 不变
    ├── branch_summarization.rs  # 不变
    ├── context.rs          # 从 core/context.rs 迁移
    ├── context_builder.rs  # 不变
    ├── steering.rs         # 不变
    ├── stop.rs             # 不变
    ├── model_switch.rs     # 不变
    ├── token.rs            # 不变
    ├── github.rs           # 不变
    ├── skill.rs            # 从 core/skill.rs 迁移
    ├── template.rs         # 从 core/template.rs 迁移
    ├── session/
    │   ├── mod.rs          # 从 session crate 合并
    │   ├── store.rs
    │   ├── manager.rs
    │   ├── export.rs
    │   └── migration.rs
    ├── tools/
    │   ├── mod.rs          # 从 tools crate 合并
    │   ├── registry.rs
    │   ├── read.rs
    │   ├── write.rs
    │   ├── edit.rs
    │   ├── bash.rs
    │   ├── find.rs
    │   ├── grep.rs
    │   ├── ls.rs
    │   └── local_env.rs
    └── tests.rs
```

**依赖**：`uncode-ai` + `uncode-shared`

### Phase 4: 删除旧 crate，更新下游依赖

**操作**：
1. 删除 `uncode-core`（功能已全部迁出）
2. 更新 `uncode-extensions` 依赖：`uncode-core` → `uncode-shared` + `uncode-agent`
3. 更新 `uncode-tui` 依赖：`uncode-core` + `uncode-agent` → `uncode-ai` + `uncode-agent`
4. 更新 `uncode-cli` 依赖：全部 → `uncode-ai` + `uncode-agent` + `uncode-tui`
5. 更新 `uncode-platform` 依赖：`uncode-agent`
6. 删除 `uncode-llm`（已合并入 uncode-ai）
7. 删除 `uncode-session`（已合并入 uncode-agent）
8. 将原 `uncode-tools` 中的内容已合入 uncode-agent/src/tools/

### Phase 5: 验证与清理

**验证清单**：
- [ ] `cargo build --workspace` 通过
- [ ] `cargo test --workspace -- --test-threads=1` 通过
- [ ] `cargo clippy --all-targets --no-deps` 无警告
- [ ] `cargo fmt --check --all` 通过
- [ ] 所有现有测试不丢失（329 个）
- [ ] 无 `pub use` 泄漏（检查新旧 crate 的公开 API）
- [ ] 更新 CLAUDE.md 中的架构描述

**清理**：
- 移除 workspace Cargo.toml 中已删除 crate 的 members
- 更新 CI 配置
- 更新依赖图文档

## 重组后架构

```
uncode-shared (error + config)         ← 叶 crate，无内部依赖
  ↓
uncode-ai (LLM + 消息 + 模型 + 7 providers)  ← 依赖 shared
  ↓
uncode-agent (引擎全栈)                ← 依赖 ai + shared
  ├── loop_engine, harness, compaction
  ├── session (JSONL 持久化)
  ├── tools (7 内置工具)
  ├── skill, template
  └── event, steering, stop
  ↓
uncode-extensions (WASM)              ← 依赖 agent (获取 tool trait)
uncode-tui (终端 UI)                   ← 依赖 ai + agent
uncode-platform (Web 后端)             ← 依赖 agent
uncode-rpc (JSON-RPC, planned)         ← 依赖 agent
uncode-macros (proc macros)            ← 无依赖
  ↓
uncode-cli (入口)                      ← 依赖 ai + agent + tui + platform + rpc
```

### 与 Pi 的映射

| Pi 包 | Uncode crate | 内容对齐度 |
|-------|-------------|-----------|
| `pi-ai` | `uncode-ai` | ~90% — LLM 抽象 + 消息类型 + 模型定义 |
| `pi-agent-core` | `uncode-agent`（核心部分） | ~85% — loop + harness + compaction + session |
| `pi-coding-agent` | `uncode-agent`（tools/skills/templates）+ `uncode-cli` | ~70% — tools + skills + CLI |
| `pi-tui` | `uncode-tui` | ~80% — 终端 UI 框架 |
| `pi-web-ui` | `uncode-platform` | ~60% — Web 端（仅后端，前端在 apps/platform） |

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| uncode-agent crate 体积过大 | 内部用 module 组织，对外只导出必要类型；Rust crate 内 module 边界已足够隔离 |
| 合并后编译时间增加 | 增量编译只重编译变更的 module；uncode-ai 仍独立，LLM provider 变更不影响 agent |
| api_types 中 ToolDefinition 循环依赖 | Context 中 tool_definitions 改为 Vec<serde_json::Value>，具体化在 agent 层 |
| 大量文件移动导致 git 历史断裂 | 使用 `git mv` 保留历史；每 Phase 独立提交 |
| uncode-extensions 依赖 agent 可能过重 | 通过 feature flag 控制依赖粒度，仅引入 tool trait 部分 |

## 实施顺序

```
Phase 1 (创建 uncode-shared) ──┐
                               ├──→ Phase 3 (创建 uncode-ai) ──→ Phase 5 (合并入 uncode-agent)
Phase 2 (uncode-core 拆分)  ──┘                                 │
                                                                ├──→ Phase 6 (更新下游 + 删除旧 crate)
                               Phase 4 (uncode-agent 扩展)  ──┘
                                                                        │
                                                                        ├──→ Phase 7 (验证 + 清理)
```

每个 Phase 完成后执行 CI 预检：
```bash
RUSTFLAGS="-D warnings" cargo fmt --check --all
RUSTFLAGS="-D warnings" cargo clippy --all-targets --no-deps
RUSTFLAGS="-D warnings" cargo build --workspace
RUSTFLAGS="-D warnings" cargo test --workspace -- --test-threads=1
```
