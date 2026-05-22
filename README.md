# uncode

Rust 原生 AI Agent Coding 系统。**认知显化与决策驱动设计**范式的参考实现。

[![CI](https://github.com/FDE-GROUP/uncode/actions/workflows/ci.yml/badge.svg)](https://github.com/FDE-GROUP/uncode/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.91+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

---

## 范式

uncode 是**认知显化与决策驱动设计**（Cognitive Explicitation & Decision-Driven Design）的参考实现。

> **人机协同创作是一个愿景从模糊认知显化、再到 Agent 工程化实现的过程。决策的本质是模糊认知的显化。Agent Coding 不是 AI 替代，而是人与大模型的有机联动。**

```
模糊认知                  显化                      工程化实现
───────────    ───────────────────────    ──────────────────────
人的自然语言意图    →  SemanticFirewall      →  Adjudicator 裁决
LLM 的自由输出      →  Parsing→Validation   →  ExecutionOrchestrator
候选方案            →  →Normalization       →  Auditor 审计
                                           →  Evaluator 评估
                                           →  EvolutionEngine 进化
         ←────────── FeedbackBridge ────────── (事件流上行反馈)
```

整套架构范式定义见 [`认知显化与决策驱动设计`](docs/ai-agent-archi/cognition-decision-driven-design.md)，
开发心路历程见 [`开发回顾`](docs/ai-agent-archi/uncode-development-retrospective.md)。

---

## 快速开始

```bash
# 配置 API key
mkdir -p ~/.config/uncode
cat > ~/.config/uncode/config.json << 'EOF'
{ "model": "deepseek-v3", "providers": { "deepseek": { "api_key": "sk-xxx" } } }
EOF

# 构建并启动 TUI
cargo build --release
cargo run -p uncode-cli
```

---

## LLM 支持

| 协议 | 覆盖供应商 |
|:---|:---|
| OpenAI Completions | DeepSeek、GLM、OpenAI、Groq、Cerebras、xAI、Mistral、OpenRouter |
| Anthropic Messages | Claude、Fireworks、Kimi |
| Gemini Generative AI | Gemini |
| Ollama Native | 本地 Ollama |

13 个内置模型。**API-first**：按协议组织驱动，新增供应商通过 Model 声明接入。

---

## 内置工具

`read` / `write` / `edit` / `grep` / `find` / `ls` / `bash` / `web_fetch` / `web_search`

---

## 项目结构

```
crates/
├── uncode-shared/        # 错误 + 配置 + GuardrailConfig + EvolutionEngine
├── uncode-ai/            # LLM 驱动层（4 协议）
├── uncode-core/          # 共享类型：AgentEvent(32变体) + SessionEntry + AgentStep
├── uncode-agent/         # Agent 引擎
│   ├── cognition/        #   认知层：上下文·提示词·不确定性·分层记忆
│   ├── decision/         #   决策层：提案·防火墙·裁决·执行·审计·评估·反馈
│   └── harness.rs        #   编排器
├── uncode-extensions/    # WASM 扩展运行时
├── uncode-tui/           # 终端 UI
├── uncode-platform/      # Web 服务（规划中）
└── uncode-cli/           # 命令行入口
```

---

## 技术栈

Rust 2024 · tokio · ratatui + crossterm · clap · reqwest · SurrealDB · WASM

---

## 开发

```bash
cargo build --workspace
cargo test --workspace -- --test-threads=1
cargo fmt --check --all
cargo clippy --all-targets --no-deps
```

---

## 文档体系

### 范式与设计

| 文档 | 说明 |
|:---|:---|
| [认知显化与决策驱动设计](docs/ai-agent-archi/cognition-decision-driven-design.md) | 范式定义（核心文档） |
| [开发心路历程](docs/ai-agent-archi/uncode-development-retrospective.md) | 从 Pi 复刻到范式提出的五阶段回顾 |
| [DDD 在 AI Agent 中的适应](docs/ai-agent-archi/ddd-ai-agent.md) | DDD 四重冲突与适应性调整 |
| [7 种架构范式](docs/ai-agent-archi/DDD%20之外AI%20Agent%20系统治理复杂性的%207%20种架构范式.md) | 综合治理工具箱 |
| [Harness Engineering 五模块](docs/others/Harness_Engineering_Archi.md) | 编排·工具·记忆·观测·进化 |
| [代码组织与命名规范](docs/ai-agent-archi/paradigm-aligned-code-organization.md) | 范式对应的理想目录结构 |

### 外部验证

| 文献 | 路径 |
|:---|:---|
| DOP 论文 (arXiv 2604.05203) | [分析](docs/ai-agent-archi/dop-analysis.md) |
| Autodesk 认知架构 | [分析](docs/ai-agent-archi/cognitive-architecture-analysis.md) |
| Anthropic Harness Engineering | [分析](docs/others/HARNESS_ENGINEERING.md) |

### 实现层文档

| 文档 | 说明 |
|:---|:---|
| [决策层设计](docs/uncode-technologies/UNCODE_DECISION_LAYER.md) | 四阶段管线 + 架构图 |
| [认知层设计](docs/uncode-technologies/UNCODE_COGNITION_LAYER.md) | 不确定性三分类 + 分层记忆 |
| [语义防火墙设计](docs/uncode-technologies/UNCODE_SEMANTIC_FIREWALL.md) | 三层管线详解 |
| [治理层设计](docs/uncode-technologies/UNCODE_GOVERNANCE_LAYER.md) | 7 范式在 uncode 中的映射 |

---

## License

MIT
