# uncode

AI Agent Coding 系统 — Rust 原生、多供应商、流式优先。

[![CI](https://github.com/FDE-GROUP/uncode/actions/workflows/ci.yml/badge.svg)](https://github.com/FDE-GROUP/uncode/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.95+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## 概述

uncode 是一个使用 Rust 开发的终端 AI Agent Coding 系统。参照 [earendil-works/pi](https://github.com/earendil-works/pi) 的设计理念，纯 Rust 重写。

**术语与对标**：机制层（Turn、双层循环、会话树、Steering 等）与 **Pi** 对齐；实现 API 保持 Rust 自有命名。分层说明见 [术语对齐策略](docs/technologies/TERMINOLOGY_ALIGNMENT_STRATEGY.md) 与 [uncode 技术术语表](docs/uncode-technologies/UNCODE_TECHNOLOGIES_GLOSSARY.md)（含 Pi/OpenCode 映射列）。

**核心组件：**

| 组件 | 面向用户 | 形态 |
|------|---------|------|
| **TUI** | 前线部署工程师 | 终端四面板交互界面 |
| **Platform** | 软件工程师 | Web 分析监控平台 |

## 快速开始

```bash
# 1. 配置 LLM API key
mkdir -p ~/.config/uncode
cat > ~/.config/uncode/config.json << 'EOF'
{
  "model": "deepseek-v3",
  "providers": {
    "deepseek": { "api_key": "sk-xxx" }
  }
}
EOF

# 2. 构建
cargo build --release

# 3. 运行
cargo run -p uncode-cli -- --model deepseek-v3 "帮我分析这个项目"
cargo run -p uncode-cli -- -i          # 交互式 TUI

# 4. 从 GitHub Issue 开始工作
export GITHUB_TOKEN=ghp_xxx
cargo run -p uncode-cli -- --issue 42
```

## 功能

- **7 个 LLM 供应商**：GLM、DeepSeek、Ollama、OpenAI、Anthropic、Gemini、OpenRouter
- **8 个内置工具**：read、write、edit、grep、bash、find、ls + GitHub API
- **四面板 TUI**：任务清单 / 工具调用 / 思考过程 / 阶段总结
- **上下文压缩**：Token 估算 + 自动摘要
- **会话持久化**：SurrealDB 主存 + JSONL 导入/导出 + 分支支持
- **WASM 扩展**：8 个生命周期钩子
- **JSON-RPC**：stdio 外部接口
- **Platform**：会话分析 + Issues 面板

## 技术栈

| 层 | 技术 |
|----|------|
| 语言 | Rust (edition 2024) |
| 异步 | tokio |
| TUI | ratatui + crossterm |
| CLI | clap |
| 配置 | TOML |
| Platform 后端 | axum |
| Platform 前端 | React 19 + TanStack |
| 数据库 | SurrealDB (SurrealKV) |

## 文档

- [uncode 技术系列总览](docs/uncode-technologies/UNCODE_OVERVIEW.md)（含 L0–L3 术语分层）
- [Pi 机制对照](docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md)
- [项目愿景](docs/VISION.md)
- [架构设计](docs/ARCHITECTURE.md)
- [TUI 交互设计](docs/TUI_DESIGN.md)
- [Platform 设计](docs/PLATFORM_DESIGN.md)
- [会话数据格式](docs/SESSION_SCHEMA.md)
- [FDE 角色解读](docs/FDE_INSIGHT.md)

## 开发

```bash
cargo build --workspace     # 构建
cargo test --workspace      # 测试（42 tests）
cargo fmt --check           # 格式检查
cargo clippy                # lint

# Platform
cd apps/platform && bun install && bun dev
```

## License

MIT
