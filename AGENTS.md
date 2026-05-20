# uncode — Rust-native Agent Coding System

## 项目概述

uncode 是一个使用 Rust 开发的终端 AI Agent Coding 系统。参考 [earendil-works/pi](https://github.com/earendil-works/pi) 的设计理念和框架，使用 Rust 重构，不涉及 Pi 的 web-ui 部分。

**架构对齐策略**：核心模块的设计对齐 Pi 的架构哲学；与 Pi 的分层映射、哲学条款（如不做 MCP 主路径等）及工程取舍的**权威对照**见 [`docs/technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。LLM 驱动层采用 **API-first 架构**（参考 `@docs/technologies/LLM_DRIVER_UPGRADE_FEASIBILITY.md`），以 API 协议为核心组织供应商，而非为每个供应商编写独立驱动实现。

## 项目结构

```
uncode/
├── Cargo.toml              # Rust workspace 根
├── AGENTS.md               # 本文档（opencode 协作指引）
├── docs/                   # 设计文档
│   ├── VISION.md           #   项目愿景与设计蓝图（顶层设计指引）
│   ├── ARCHITECTURE.md     #   架构详细设计
│   ├── TUI_DESIGN.md       #   TUI 交互设计详案
│   ├── PLATFORM_DESIGN.md  #   Platform 设计详案
│   ├── SESSION_SCHEMA.md   #   会话条目逻辑模型（导出 JSONL 见 docs/uncode-technologies）
│   ├── FDE_INSIGHT.md      #   FDE 角色深度解读
│   ├── technologies/       #   技术分析与方案文档
│   │   ├── UNCODE_PI_ALIGNMENT_AND_EVALUATION.md  # uncode 对 Pi 的复刻：深度对比与评价
│   │   ├── EXTENSION_COMPOSABLE_HARNESS_DESIGN.md  # 可组合扩展与 Plan 模式：设计理念与技术方案
│   │   ├── AGENT_CODING_FUNDAMENTALS.md
│   │   ├── LLM_DRIVER_DESIGN.md
│   │   ├── LLM_DRIVER_COMPARISON_PI.md
│   │   └── LLM_DRIVER_UPGRADE_FEASIBILITY.md
│   ├── opencode-technologies/  #   OpenCode 上游实现层文档（基于 ~/EA/opencode 源码）
│   │   ├── OPENCODE_OVERVIEW.md
│   │   └── …
│   └── uncode-technologies/  #   实现层技术文档（与源码同步）
│       ├── UNCODE_OVERVIEW.md
│       ├── UNCODE_SESSION_MODEL.md
│       └── …
├── crates/                 # Rust workspace 成员
│   ├── uncode-shared/      #   错误类型 + 配置（叶子 crate）
│   ├── uncode-macros/      #   过程宏（#[tool] 等）
│   ├── uncode-ai/          #   LLM 驱动层（Api trait + 4 个协议实现）
│   ├── uncode-core/        #   共享类型、ToolExecutor、AgentEvent、SessionEntry 等
│   ├── uncode-extensions/  #   扩展系统（WASM 运行时 + 生命周期钩子）
│   ├── uncode-agent/       #   循环 + SurrealDB 会话 + 工具 + 压缩 + skills
│   ├── uncode-tui/         #   终端 UI
│   ├── uncode-rpc/         #   JSON-RPC 外部接口（规划中）
│   ├── uncode-platform/    #   Platform 服务端
│   └── uncode-cli/         #   命令行入口
├── apps/                   # 前端应用
│   └── platform/           #   Platform 前端（TypeScript + React 19 + TanStack）
├── tests/                  # 集成测试
└── .github/workflows/      # CI/CD
```

## 技术栈

- **语言**：Rust（edition 2024，MSRV 1.85）
- **框架/库**：tokio（异步）、ratatui + crossterm（TUI）、clap（CLI）、reqwest（HTTP）、serde（序列化）
- **LLM 协议**（API-first）：
  - `openai-completions` — OpenAI、DeepSeek、GLM、Groq、Cerebras、xAI、Mistral 等所有 OpenAI Chat Completions 兼容供应商
  - `anthropic-messages` — Anthropic、Fireworks、Kimi 等
  - `google-generative-ai` — Gemini
  - `ollama-native` — Ollama 原生 API
- **Platform 前端**：TypeScript（React 19 + TanStack 全家桶）

## 构建与验证

```bash
cargo build              # 构建所有 crate
cargo build -p uncode-cli # 仅构建 CLI 入口
cargo test               # 运行所有测试
cargo test -p uncode-core # 运行单个 crate 测试
cargo fmt --check        # 格式检查
cargo clippy             # lint 检查
```

## 开发约定

- 设计决策先写入 @docs/ 目录下的对应文档，确认后需要检查github issues 是否有对应的issues，如果没有应当及时创建，然后再开始编码
- 文档使用中文书写
- 架构分层严格遵守：core → llm/session/tools/extensions → agent → tui/platform → cli
- 跨层通信通过事件流，上层不直接依赖下层实现
- LLM 驱动层以 API 协议为组织单位，新增供应商通过 Model 声明接入，不新增驱动实现
- 技术对标 Pi 时，架构哲学优先对齐（API-first），工程细节次之（具体字段/选项）
- **术语（策略 C）**：L0 用 Harness 综述表；L1 机制与 Pi 对齐（见 `UNCODE_PI_MECHANISM_MAP.md`）；L2 Rust API 自有命名；文档写「同 Pi 的 X」而非批量改 API 名

## 外部规则引用

当需要了解项目定位、目标用户、TUI 设计理念、Platform 功能、Issues 同步策略等详细设计时，请读取 @docs/VISION.md。

技术方案文档位于 @docs/technologies/；uncode **实现层**细节见 @docs/uncode-technologies/（与源码同步，含会话 SurrealDB 与 Pi 对齐说明）。

- 基本功能模块分析 → `AGENT_CODING_FUNDAMENTALS.md`
- LLM 驱动层技术方案 → `LLM_DRIVER_DESIGN.md`
- 与 Pi 的技术比对 → `LLM_DRIVER_COMPARISON_PI.md`
- 对齐 Pi 的升级方案 → `LLM_DRIVER_UPGRADE_FEASIBILITY.md`
- Harness Engineering 行业综述 → `HARNESS_ENGINEERING.md`
- Harness Engineering 术语索引（中英） → `HARNESS_ENGINEERING_GLOSSARY.md`
- Coding Agent 工具开发指南 → `CODING_AGENT_TOOL_DEVELOPMENT.md`
- OpenCode 与 Pi 架构/功能/哲学对比（独立技术分析） → `OPENCODE_VS_PI.md`
- OpenCode 上游实现层文档（`~/EA/opencode` 源码） → `docs/opencode-technologies/OPENCODE_OVERVIEW.md` 系列
- 术语是否与 Pi/OpenCode 趋同或引用 → `TERMINOLOGY_ALIGNMENT_STRATEGY.md`
- 术语分层重构（策略 C 落地）→ `TERMINOLOGY_LAYERED_REFACTOR_PLAN.md`
- uncode 实现层术语表（含 Pi/OpenCode 列）→ `docs/uncode-technologies/UNCODE_TECHNOLOGIES_GLOSSARY.md`
- uncode ↔ Pi 机制对照（L1）→ `docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md`
- uncode 对 Pi 的 Rust 复刻：哲学/机制/存储/扩展深度评价 → `UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`
- 可组合扩展宿主与 Plan 模式（Pi plan-mode 对照、uncode 演进方案；**Turn ≠ Plan 模式**，见 §2.3）→ `EXTENSION_COMPOSABLE_HARNESS_DESIGN.md`
- 微观规划（micro-planning）能力说明：ReAct Turn 内决策 vs Plan 模式 → `docs/uncode-technologies/UNCODE_MICRO_PLANNING.md`
- TUI 微观规划 UX 评价（Turn 边界、`agent_busy`、steering）→ `docs/uncode-technologies/UNCODE_TUI_MICRO_PLANNING_UX.md`

后续设计文档编写时，请读取 @docs/VISION.md 确保一致性，参考 opencode 的 AGENTS.md 规范格式。