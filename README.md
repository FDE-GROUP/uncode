# uncode

Rust 原生 AI Agent Coding 系统 — 流式优先、多供应商、终端原生。

[![CI](https://github.com/FDE-GROUP/uncode/actions/workflows/ci.yml/badge.svg)](https://github.com/FDE-GROUP/uncode/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.91+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

---

## 概述

uncode 是一个使用 Rust 从零构建的终端 AI Agent Coding 系统，对标 [earendil-works/pi](https://github.com/earendil-works/pi) 的架构哲学。

**核心能力**：接收用户自然语言需求 → 构建完整上下文 → 提交 LLM → 流式处理响应 → 执行工具 → 循环迭代，直到完成目标。支持 13 个内置模型、4 种 LLM 协议。

**术语策略**（策略 C 分层混合）：L0 行业概念与 Pi/OpenCode 共用英文；L1 机制命名与 Pi 对齐；L2 Rust API 自有命名。详见 [术语对齐策略](docs/technologies/TERMINOLOGY_ALIGNMENT_STRATEGY.md)、[技术术语表](docs/uncode-technologies/UNCODE_TECHNOLOGIES_GLOSSARY.md)（含 Pi/OpenCode 列）、[Pi 机制对照](docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md)。

**核心组件：**

| 组件 | 面向用户 | 形态 |
|------|---------|------|
| **TUI** | 前线部署工程师 | 终端对话驱动界面 |
| **Platform** | 软件工程师 | Web 分析监控平台（规划中） |

---

## 快速开始

```bash
# 1. 配置 API key（CLI 默认路径）
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

# 3. TUI 交互模式（推荐）
cargo run -p uncode-cli

# 4. 单次问答
cargo run -p uncode-cli -- "帮我分析这个项目"
```

> 部分高级配置与技能路径使用 `~/.uncode/`（如 `config.toml`、skills）。详见 [模型配置指南](docs/guides/MODEL_CONFIG.md)。

---

## 功能

### LLM 支持

| 协议 | 覆盖供应商 |
|------|-----------|
| OpenAI Completions | DeepSeek、GLM、OpenAI、Groq、Cerebras、xAI、Mistral、OpenRouter |
| Anthropic Messages | Claude、Fireworks、Kimi |
| Gemini Generative AI | Gemini |
| Ollama Native | 本地 Ollama 实例 |

13 个内置模型，支持自定义模型配置。**API-first**：按协议组织驱动，不按供应商重复实现。

### 内置工具

| 工具 | 功能 |
|------|------|
| `read` / `write` / `edit` | 读写在沙箱内；`edit` 支持 hashline 与 legacy 模式 |
| `grep` / `find` / `ls` | 搜索与目录浏览 |
| `bash` | 命令执行（timeout、workdir、取消） |
| `web_fetch` / `web_search` | 网页抓取 / Tavily 搜索（需 API key） |

### TUI（终端界面）

- 流式显示 Thinking、Assistant 文本与工具调用卡片
- 工具自定义渲染器（对标 OpenCode scrollback 的信息密度）
- 虚拟滚动、聚焦卡片折叠/展开、权限确认与 ESC 多优先层中断
- 斜杠命令：`/model`、`/new`、`/compact`、`/thinking`、`/tree` 等

### Agent 引擎

- **双层循环**：`AgentHarness` 编排 + `AgentLoop` 执行（外层 Turn、内层多轮工具；**Pi:** `agentLoop`）
- **上下文构建**：`build_context` 从 **SurrealDB** 会话存储加载树状 `SessionEntry`（**Pi:** `convertToLlm` 前的上下文组装；物理存储与 Pi JSONL 不同）
- **上下文压缩**：Token 估算 + 自动摘要
- **消息排队**：Steering / Follow-up / Next-turn 三队列（**Pi:** `MessageQueue`）
- **会话持久化**：嵌入式 **SurrealDB 主存** + **JSONL 导入/导出** + 树状分支

其他：WASM 扩展（生命周期钩子）、JSON-RPC（stdio，规划中）、Platform 会话分析。

---

## 技术栈

| 层 | 选择 |
|----|------|
| 语言 | Rust 2024 edition，MSRV 1.91 |
| 异步 | tokio |
| TUI | ratatui + crossterm + syntect |
| CLI | clap |
| LLM | reqwest（流式）+ API-first 四协议 |
| 会话存储 | SurrealDB（嵌入式）+ JSONL 互操作 |
| 配置 | CLI：`~/.config/uncode/config.json`；扩展：`~/.uncode/config.toml` 等 |
| Platform | axum + React 19 + TanStack（`apps/platform`） |
| 跨层通信 | `AgentEvent` broadcast |

---

## 文档

| 文档 | 说明 |
|------|------|
| [架构总览](docs/uncode-technologies/UNCODE_OVERVIEW.md) | Crate 分层、L0–L3 术语分层 |
| [Pi 机制对照](docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md) | 事件 / 会话 / 循环对照表 |
| [循环引擎](docs/uncode-technologies/UNCODE_LOOP_ENGINE.md) | Turn / Steering、`AgentLoop` |
| [会话模型](docs/uncode-technologies/UNCODE_SESSION_MODEL.md) | `SessionEntry` 树、SurrealDB + JSONL |
| [事件系统](docs/uncode-technologies/UNCODE_EVENT_SYSTEM.md) | 18 种 `AgentEvent` |
| [术语分层重构方案](docs/technologies/TERMINOLOGY_LAYERED_REFACTOR_PLAN.md) | Phase 1–3 与 backlog |
| [项目愿景](docs/VISION.md) | 顶层设计 |
| [架构设计](docs/ARCHITECTURE.md) | 系统架构详案 |

---

## 开发

```bash
cargo build --workspace
cargo test --workspace -- --test-threads=1   # tools 测试需单线程
cargo fmt --check --all
RUSTFLAGS="-D warnings" cargo clippy --all-targets --no-deps

cd apps/platform && bun install && bun dev
```

---

## License

MIT
