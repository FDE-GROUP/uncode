# uncode 架构总览

> 系列文档索引 | 基于 uncode 源码分析

uncode 是一个 Rust 原生的 AI Agent Coding 系统，面向两类用户：前线部署工程师（TUI）和软件工程师（Web Platform）。核心引擎约 15,000 行 Rust 代码，支持 7 个 LLM Provider、流式优先架构、JSONL 会话持久化和树状分支。

---

## 三层架构

```
┌─────────────────────────────────────────────────────────┐
│                    Entry Points                         │
│  uncode-cli ──┬── TUI (ratatui)                        │
│               ├── Platform (axum REST + WebSocket)      │
│               ├── RPC (JSON-RPC over stdio)             │
│               └── CLI (one-shot / REPL / --issue)       │
├─────────────────────────────────────────────────────────┤
│                  Agent Engine                           │
│  uncode-agent ── LoopEngine + Harness + Session         │
│       ├── Steering (steer / followUp / nextTurn)        │
│       ├── Compaction (token-aware context compression)  │
│       ├── Context Builder (session → LLM messages)      │
│       ├── System Prompt Builder                         │
│       ├── 7 Tools (read/write/edit/grep/bash/find/ls)   │
│       └── Branch Summarization                          │
├─────────────────────────────────────────────────────────┤
│                 Foundation                              │
│  uncode-ai ─────── Api trait + 4 provider impls         │
│  uncode-core ───── shared types (event/tool/session)    │
│  uncode-shared ─── error + config (leaf crate)          │
│  uncode-macros ─── #[tool] proc macro (compile-time)    │
│  uncode-extensions─ WASM extension runtime (scaffold)   │
└─────────────────────────────────────────────────────────┘
```

---

## 系列文档索引

| 文档 | 内容 |
|------|------|
| [架构总览](UNCODE_OVERVIEW.md) | 三层架构、依赖图、设计决策（本文档） |
| [循环引擎](UNCODE_LOOP_ENGINE.md) | 双层循环、Turn 生命周期、Steering、压缩 |
| [LLM 抽象层](UNCODE_LLM_LAYER.md) | Api trait、4 种协议、StreamEvent、模型注册 |
| [工具系统](UNCODE_TOOL_SYSTEM.md) | ToolExecutor、7 工具、沙箱、Hooks、执行模式 |
| [会话模型](UNCODE_SESSION_MODEL.md) | SessionEntry、JSONL 持久化、树状分支、压缩摘要 |
| [事件系统](UNCODE_EVENT_SYSTEM.md) | AgentEvent、EventRouter、HookResult、事件序列 |
| [TUI 枲构](UNCODE_TUI_ARCHITECTURE.md) | 虚拟滚动、增量渲染、syntect 高亮、Markdown 渲染 |

---

## 核心设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 语言 | Rust 2024 edition，MSRV 1.85 | 性能 + 安全，`unsafe_code = "deny"` |
| 异步运行时 | tokio（full features） | 流式 LLM 调用需要全功能异步 |
| 错误处理 | thiserror（库）+ anyhow（应用） | 分层：库 crate 用结构化错误，应用层用 anyhow |
| 会话持久化 | JSONL append-only | 崩溃安全、天然回放、支持分支 |
| LLM 通信 | 流式优先，所有 Provider 返回 BoxStream | 用户体验：实时显示思考和文本 |
| 工具注册 | 编译时 `#[tool]` 宏 + 运行时 `ToolRegistry` | 类型安全的 Schema 生成 |
| 跨层通信 | `broadcast::Sender<AgentEvent>` | 发布-订阅，TUI/Platform/RPC 统一订阅 |
| 配置 | TOML `~/.uncode/config.toml` | 人工可编辑，类型安全解析 |
| 前端 | React 19 + TanStack + Vite (TypeScript strict) | Platform 独立前端，内嵌于 Platform 二进制 |

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
uncode-extensions (WASM runtime — scaffold)
    ↑
uncode-agent (AgentLoop + Harness + Session + Tools + Compaction)
    ↑
┌───┴───┬────────┬──────────┐
uncode-tui  uncode-platform  uncode-rpc
    ↑
uncode-cli (唯一入口点)
```

**关键约束**：依赖方向严格从上到下。上层 crate 通过 `broadcast` channel 或 trait object 与下层交互，不直接调用具体实现。

---

## Crate 一览

| Crate | 行数 | 职责 |
|-------|------|------|
| `uncode-shared` | ~300 | `UncodeError` 错误体系（5 个子类型，数字编码）+ `AppConfig` 配置 |
| `uncode-macros` | ~210 | `#[tool]` 属性宏：从函数签名生成 `ToolDefinition` |
| `uncode-ai` | ~2500 | `Api` trait、4 种 API 协议实现、`StreamEvent` 流式协议、`ModelRegistry` |
| `uncode-core` | ~1800 | `ToolExecutor` trait、`AgentEvent` 枚举、`SessionEntry` 树状模型、`SkillRegistry` |
| `uncode-extensions` | ~200 | 生命周期 Hook 系统（8 个钩子）、WASM 加载器（scaffold） |
| `uncode-agent` | ~4500 | `AgentLoop` 双层循环、`AgentHarness` 编排器、压缩、7 工具实现 |
| `uncode-tui` | ~3500 | ratatui 渲染引擎、虚拟滚动、syntect 高亮、Markdown 渲染 |
| `uncode-platform` | ~850 | axum REST + WebSocket 服务、session metrics、GitHub proxy |
| `uncode-rpc` | ~500 | JSON-RPC 2.0 over stdio、8 个核心命令 |
| `uncode-cli` | ~730 | clap 参数解析、模式路由（TUI/CLI/RPC/Platform） |

---

## 运行模式

CLI 入口点（`uncode-cli/src/main.rs`）根据参数选择运行模式：

| 模式 | 触发条件 | 行为 |
|------|----------|------|
| **TUI** | 无参数启动 | 启动 ratatui 全屏 UI，agent 在后台 tokio task 运行 |
| **CLI one-shot** | `uncode "prompt"` | 单次 `agent.run()` → 打印结果 |
| **CLI streaming** | `uncode "prompt" --mode json` | 流式输出 `AgentEvent` JSONL 到 stdout |
| **REPL** | `uncode --repl` | stdin 循环读取输入 |
| **RPC** | `uncode --mode rpc` | JSON-RPC 2.0 over stdio |
| **Platform** | `uncode platform` | 启动 axum HTTP 服务器 |
| **Issue** | `uncode --issue 42` | 抓取 GitHub Issue → 一次性执行 |

---

*本文档基于 uncode 源码编写。*
