# uncode 架构总览

> 系列文档索引 | 基于 uncode 源码分析，2026-05 修订；2026-05 起与 Pi 对齐叙事见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)

uncode 是一个 **以 Pi（earendil-works/pi）为架构与哲学参照** 的 Rust 原生 AI Agent Coding 系统：分层（`uncode-ai` / `uncode-core`+`uncode-agent` / CLI+TUI）与事件驱动 Harness 与 Pi 同构；在存储与交付面上有工程化取舍（见下文「会话」）。核心引擎约 28,000 行 Rust 代码，支持十余个内置模型、4 种 LLM 协议、流式优先架构、**树状会话模型**（逻辑上与 Pi 的 JSONL 会话树同构）及分支/压缩。

---

## 与 Pi 的关系（一句话）

- **逻辑**：`SessionEntry` 树、分支、压缩、分支摘要、`AgentEvent` 流式 UI 解耦 —— 对齐 Pi 的 harness 心智。  
- **物理**：会话默认落 **嵌入式 SurrealDB**（`kv-rocksdb`），非「每会话一个 `.jsonl` 文件」；仍支持 **JSONL 导入旧数据** 与 **导出**（审计/迁移）。详见 [会话模型](UNCODE_SESSION_MODEL.md)。

---

## 三层架构

```
┌─────────────────────────────────────────────────────────┐
│                    Entry Points                         │
│  uncode-cli ──┬── TUI (ratatui + crossterm)            │
│               └── CLI (one-shot / REPL / --issue)       │
├─────────────────────────────────────────────────────────┤
│                  Agent Engine                           │
│  uncode-agent ── LoopEngine + Harness + Session         │
│       ├── Steering (steer / followUp / nextTurn)        │
│       ├── Compaction (token-aware context compression)  │
│       ├── Context Builder (session → LLM messages)      │
│       ├── System Prompt Builder                         │
│       ├── 工具集（9 个实现；CLI 默认注册 7，find/ls 等可按需注册）         │
│       └── Branch Summarization                          │
├─────────────────────────────────────────────────────────┤
│                 Foundation                              │
│  uncode-ai ─────── Api trait + 4 provider impls         │
│  uncode-core ───── shared types (event/tool/session)    │
│  uncode-shared ─── error + config (leaf crate)          │
│  uncode-macros ─── #[tool] proc macro (compile-time)    │
│  uncode-extensions─ WASM extension runtime              │
└─────────────────────────────────────────────────────────┘
```

---

## 系列文档索引

| 文档 | 内容 |
|------|------|
| [术语索引](UNCODE_TECHNOLOGIES_GLOSSARY.md) | **中英对照术语表**（读本系列前的速查） |
| [术语对齐策略](../technologies/TERMINOLOGY_ALIGNMENT_STRATEGY.md) | 与 Pi/OpenCode 趋同还是引用的策略论述 |
| [架构总览](UNCODE_OVERVIEW.md) | 三层架构、依赖图、设计决策（本文档） |
| [循环引擎](UNCODE_LOOP_ENGINE.md) | 双层循环、Turn 生命周期、Steering、压缩 |
| [LLM 抽象层](UNCODE_LLM_LAYER.md) | Api trait、4 种协议、StreamEvent、模型注册 |
| [工具系统](UNCODE_TOOL_SYSTEM.md) | ToolExecutor、工具实现、沙箱、Hooks、执行模式 |
| [会话模型](UNCODE_SESSION_MODEL.md) | SessionEntry、SurrealDB 持久化、JSONL 互操作、树状分支、压缩摘要 |
| [事件系统](UNCODE_EVENT_SYSTEM.md) | AgentEvent 18 variants、EventRouter、HookResult、事件序列 |
| [TUI 架构](UNCODE_TUI_ARCHITECTURE.md) | 虚拟滚动、增量渲染、syntect 高亮、Markdown 渲染、工具渲染器 |
| [TUI 事件流](TUI_EVENT_FLOW.md) | 三股事件流汇聚、快捷键系统、渲染管线、组件协作 |
| [请求生命周期](UNCODE_REQUEST_LIFECYCLE.md) | 用户输入 → Context 构建 → LLM 调用 → 流式响应 → 工具执行 |

---

## 核心设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 语言 | Rust 2024 edition，MSRV 1.85 | 性能 + 安全 |
| 异步运行时 | tokio（full features） | 流式 LLM 调用需要全功能异步 |
| 错误处理 | `uncode_shared::error::UncodeError`（14 variants） | 分层：库 crate 用结构化错误，应用层用 anyhow |
| 会话持久化 | SurrealDB（嵌入式）+ 异步 `SessionStore`；JSONL 仅导入/导出 | 与 Pi 逻辑模型同构；多面（TUI/Platform）与索引需求；导出保留可审计文本流 |
| LLM 通信 | 流式优先，所有 Provider 返回 BoxStream | 用户体验：实时显示思考和文本 |
| 工具注册 | 编译时 `#[tool]` 宏 + 运行时 `ToolRegistry` | 类型安全的 Schema 生成 |
| 跨层通信 | `broadcast::Sender<AgentEvent>`（18 variants） | 发布-订阅，TUI 统一订阅 |
| 配置 | TOML `~/.uncode/config.toml` | 人工可编辑，类型安全解析 |
| 工具渲染 | 9 个 per-tool Renderer（零分配静态分发） | 对标 opencode 的 scrollback 格式 |

---

## 模块依赖关系

```
uncode-shared (error + config — 叶子 crate)
    ↑
uncode-macros (proc-macro，编译时无依赖)
    ↑
uncode-ai (Api trait + providers + models + messages + streaming)
    ↑                ↑
uncode-core ←────────┘ (tool/event/session/skill/template — 重新导出 ai + shared 类型)
    ↑
uncode-extensions (WASM runtime)
    ↑
uncode-agent (AgentLoop + Harness + Session + Tools + Compaction)
    ↑
uncode-tui (TUI engine + chat state + tool renderers + input editor + markdown)
    ↑
uncode-cli (唯一入口点)
```

**关键约束**：依赖方向严格从上到下。上层 crate 通过 `broadcast` channel 或 trait object 与下层交互，不直接调用具体实现。

---

## Crate 一览

| Crate | 代码量 | 职责 |
|-------|--------|------|
| `uncode-shared` | ~650 | `UncodeError` 错误体系（14 variants，5 个子类型 + 数字编码）+ `AppConfig` 配置 |
| `uncode-macros` | ~390 | `#[tool]` 属性宏：从函数签名生成 `ToolDefinition` |
| `uncode-ai` | ~3000 | `Api` trait、4 种 API 协议实现、`StreamEvent` 流式协议、13 个内置模型、`CompatConfig`（16 fields） |
| `uncode-core` | ~3200 | `ToolExecutor` trait、`AgentEvent` 枚举（18 variants）、`SessionEntry` 树状模型、`SkillRegistry` |
| `uncode-extensions` | ~430 | 生命周期 Hook 系统（8 个钩子）、WASM 加载器 |
| `uncode-agent` | ~10200 | `LoopEngine` 双层循环、`AgentHarness` 编排器、压缩、工具实现 |
| `uncode-tui` | ~9400 | ratatui 渲染引擎、虚拟滚动、syntect 语法高亮、Markdown 渲染、9 个工具渲染器 |
| `uncode-cli` | ~1900 | clap 参数解析、模式路由（TUI/CLI）、工具注册 |

---

## 运行模式

CLI 入口点（`uncode-cli/src/main.rs`）根据参数选择运行模式：

| 模式 | 触发条件 | 行为 |
|------|----------|------|
| **TUI** | 无参数启动 | 启动 ratatui 全屏 UI，agent 在后台 tokio task 运行 |
| **CLI one-shot** | `uncode "prompt"` | 单次 `agent.run()` → 打印结果 |
| **Issue** | `uncode --issue 42` | 抓取 GitHub Issue → 一次性执行 |

---

*本文档基于 uncode 源码编写；2026-05 修订会话存储叙事（SurrealDB + Pi 对齐说明）。*
