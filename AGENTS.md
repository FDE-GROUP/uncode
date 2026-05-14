# uncode — Rust-native Agent Coding System

**重要约定**
文档及Issues优先原则：没有文档和Issues不能开发，设计决策先写入 @docs/ 目录下的对应文档，确认后需要检查github issues 是否有对应的issues，如果没有应当及时创建，然后再开始编码

当前阶段：进入 Phase 1 核心骨架开发。

## 项目概述

uncode 是一个使用 Rust 开发的终端 AI Agent Coding 系统。参考 [earendil-works/pi](https://github.com/earendil-works/pi) 的设计理念和框架，使用 Rust 重构，不涉及 Pi 的 web-ui 部分。

详细愿景和设计决策见 @docs/VISION.md，后续设计文档见 @docs/ 目录。

## 项目结构

```
uncode/
├── Cargo.toml              # Rust workspace 根
├── AGENTS.md               # 本文档（opencode 协作指引）
├── docs/                   # 设计文档
│   ├── VISION.md           #   项目愿景与设计蓝图
│   ├── TUI_DESIGN.md       #   TUI 交互设计详案
│   ├── PLATFORM_DESIGN.md  #   Platform 设计详案
│   ├── ARCHITECTURE.md     #   架构详细设计
│   ├── SESSION_SCHEMA.md   #   会话数据 JSONL Schema
│   └── FDE_INSIGHT.md      #   FDE 角色深度解读
├── crates/                 # Rust workspace 成员
│   ├── uncode-core/        #   共享类型、trait、错误、事件
│   ├── uncode-macros/      #   过程宏（#[tool] 等）
│   ├── uncode-llm/         #   LLM 驱动抽象 + 供应商实现
│   ├── uncode-session/     #   会话持久化（JSONL）
│   ├── uncode-tools/       #   内置工具集
│   ├── uncode-extensions/  #   扩展系统（规划中）
│   ├── uncode-agent/       #   代理循环引擎
│   ├── uncode-tui/         #   终端 UI（4 大展示模块）
│   ├── uncode-rpc/         #   JSON-RPC 外部接口（规划中）
│   ├── uncode-platform/    #   Platform 服务端（规划中）
│   └── uncode-cli/         #   命令行入口
├── platform/               # Platform 前端（TypeScript，规划中）
├── tests/                  # 集成测试
└── .github/workflows/      # CI/CD
```

## 技术栈

- **语言**：Rust（edition 2024，MSRV 1.85）
- **框架/库**：tokio（异步）、ratatui + crossterm（TUI）、clap（CLI）、reqwest（HTTP）、serde（序列化）
- **LLM 供应商**：GLM、DeepSeek、OpenRouter、Ollama、OpenAI、Anthropic、Gemini
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

## 外部规则引用

当需要了解项目定位、目标用户、TUI 设计理念、Platform 功能、Issues 同步策略等详细设计时，请读取 @docs/VISION.md。
后续设计文档编写时，请读取 @docs/VISION.md 确保一致性，参考 opencode 的 AGENTS.md 规范格式。
