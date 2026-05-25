# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

uncode is a Rust-native AI Agent Coding system with two user-facing components: a TUI for front-line deployment engineers and a web Platform for software engineers. It supports 11 LLM providers (GLM, DeepSeek, Ollama, OpenAI, Anthropic, Gemini, OpenRouter, Groq, Cerebras, Mistral, xAI) across 4 API protocols with streaming-first architecture.

## Build & Development Commands

```bash
cargo build --workspace          # Build all crates
cargo build -p uncode-cli        # Build CLI only
cargo test --workspace           # Run all tests
cargo test -p uncode-agent        # Run single crate tests
cargo test -p uncode-agent test_name  # Run single test
cargo test --workspace -- --test-threads=1  # Run tests single-threaded (required for tools tests)
cargo fmt --check --all          # Format check
cargo clippy --all-targets --no-deps  # Lint
cargo api-doc                       # API docs (workspace, --no-deps; aliases in .cargo/config.toml)
cargo api-doc-open                  # Open uncode-core / uncode-agent / uncode-ai docs in browser
cargo run -p uncode-cli -- --model deepseek-v3 "prompt"  # Run CLI
cd apps/platform && bun install && bun dev   # Platform frontend dev server
cd apps/platform && bun run build           # Platform frontend build
cd apps/platform && bun run lint            # Platform frontend lint (Biome)
```

CI runs: `cargo fmt --check`, `cargo clippy --all-targets --no-deps`, `cargo build --workspace`, `cargo doc --workspace --no-deps`, `cargo test --workspace -- --test-threads=1` with `RUSTFLAGS="-D warnings"` (rustdoc uses `RUSTDOCFLAGS="-D warnings"` via `.cargo/config.toml`). CI also uses `--test-threads=1` because tools tests require it.

## Architecture

Three-layer dependency graph aligned with Pi architecture:

```
uncode-cli (entry point, clap arg parsing)
├── uncode-tui (ratatui + crossterm, conversation-driven terminal UI)
├── uncode-platform (axum REST backend)
└── uncode-rpc (JSON-RPC, planned)
        │
    uncode-agent (full-stack engine: loop + harness + session + tools + compaction + skills)
        ├── uncode-ai (LLM abstraction: Api trait + 4 providers + models + messages + streaming)
        ├── uncode-core (shared agent types: events, tool traits, session types, skills, templates)
        ├── uncode-extensions (WASM extension runtime)
        └── uncode-ontology (type registry, constraint axioms, action metadata, EntityCategory)
                │
            uncode-shared (error types + config — leaf crate)
            uncode-macros (proc macros: #[tool], #[derive(Event)] — compile-time only)
```

Cross-layer communication uses event streams. Upper layers subscribe to events, not direct calls.

### LLM Provider Architecture

All providers implement `Api` trait (`uncode-ai/src/api.rs`). 4 API protocol implementations in `uncode-ai/src/providers/`:
- `openai-completions` — covers DeepSeek, GLM, OpenAI, OpenRouter, Groq, Cerebras, Mistral, xAI
- `anthropic-messages` — Anthropic
- `google-generative-ai` — Gemini
- `ollama-native` — Ollama

New vendors are added via `ProviderPreset` declarations in `uncode-ai/src/provider_preset.rs` — no new protocol driver needed unless the API protocol is genuinely different. Tool calls follow a three-stage protocol: `ToolCallStart` → `ToolCallDelta` → `ToolCallEnd`. Every stream must end with `StreamEvent::Done`.

### Tool System

9 tools in `uncode-agent/src/tools/`: read, write, edit, grep, find, ls, bash, web_fetch, web_search. Each implements `ToolExecutor` trait. File tools use `normalize_path()` + `resolve_path()` for sandbox enforcement — paths must resolve within CWD. Tools are registered via `ToolRegistry` with `ToolHooks` (before/after) for permission gating and result patching.

### Agent Loop

`AgentHarness` (`uncode-agent/src/harness.rs`) orchestrates phases, session persistence, and compaction triggers. `AgentLoop` (`uncode-agent/src/loop_engine.rs`) is the core ReAct execution engine with a double-layer loop: outer loop handles follow-up injection; inner loop processes LLM streaming responses and tool calls. Three message queues: Steering (interrupt), Follow-up (within-turn), Next-turn (user input). `MAX_TURNS=50` hard limit prevents runaway loops.

### Paradigm: Cognition-Explicitization & Decision-Driven Design

Implements the paradigm defined in [`docs/agent-archi/`](docs/agent-archi/README.md). Four layers:

| Layer | Module | Status |
|:---|:---|:---:|
| **认知层** | `uncode-agent/src/cognition/` (WM → EM → memory manager) | ✅ 实现 |
| **语义防火墙** | `uncode-agent/src/decision/firewall.rs` (P→V→N) | ✅ 已实现：DeclarativeNormalizer 对接本体，OntologyConstraintRule 校验 preconditions |
| **决策层** | `uncode-agent/src/decision/` (proposal → adjudication → execution → audit) | ⚠️ 管线已是前门控模式，但 ActionProposal 缺少上下文字段，需补全并发射细粒度事件 |
| **治理层** | `uncode-shared/src/guardrails.rs` + `uncode-core/src/event.rs` + `AgentHarness` | ⚠️ GuardrailConfig 已定义但未运行时加载；EventRouter 未接入主循环 |

**核心缺口**：`uncode-ontology` 已实现领域语义本体（9 工具 + 3 实体）+ 系统资源语义本体（LLM/Provider/Capability + 2 Action）+ 关系类型（LinkDef，5 条内置）。工具权限已通过 `ExecutionCategory` 和 `OntologyConstraintRule` 对接本体，但 `GuardrailConfig` 尚未在运行时加载。剩余缺口：ReasoningRule（约束链 + 关系遍历）、本体版本管理。详见重构计划和技术方案。

### Compaction

`uncode-agent/src/compaction.rs` handles automatic context compression when token usage approaches context window limits (default 80% threshold). Triggered by the harness at turn boundaries. Preserves decision-relevant content while condensing verbose tool output.

### Streaming Protocol

LLM responses stream as `StreamEvent` variants: `TextDelta` → `ToolCallStart` → `ToolCallDelta` → `ToolCallEnd` → `Done`. The loop processes each event to update UI in real-time via `AgentEvent` broadcast.

### Permission Gate

`PermissionGate` (`uncode-agent/src/permission_gate.rs`) blocks tool execution pending TUI user confirmation. Policy in `uncode-agent/src/tool_permission.rs`: read-only tools auto-allowed when `auto_allow_readonly=true`; write/edit/bash require approval. `SAFE_COMMANDS` array whitelists low-risk bash commands.

### Event System

`AgentEvent` (`uncode-core/src/event.rs`) has 36 variants for session/turn/message/tool/compaction/decision/evaluation lifecycle. `EventRouter` dispatches via dual channels: sync_handlers (observation) and hook_handlers (control flow — can block or redirect execution). Upper layers (TUI, Platform) subscribe to events, never call agent directly.

## 文档结构

重要文档位置：

| 文档 | 位置 | 用途 |
|:---|:---|:---|
| 架构范式系列 | `docs/agent-archi/README.md` | AI Agent 架构治理范式（8 篇核心 + 1 篇回顾） |
| 重构技术方案 | `docs/technologies/UNCODE_REFACTORING_PLAN.md` | 范式→代码的差距分析 + 四阶段重构路线图 |
| 本体设计方案 | `docs/technologies/UNCODE_ONTOLOGY_DESIGN.md` | `uncode-ontology` crate 的完整设计：TypeRegistry / Constraint / Effect / LinkDef / GT-OT 二元模型 |
| 决策管线设计 | `docs/technologies/UNCODE_PHASE2_DECISION_PIPELINE.md` | Phase 2：ActionProposal 扩展、细粒度事件、审计持久化 |
| 治理激活设计 | `docs/technologies/UNCODE_PHASE3_GOVERNANCE_ACTIVATION.md` | Phase 3：EventRouter 接入、PhaseStateMachine、GuardrailConfig 运行时生效 |
| 旧版系列（归档） | `docs/references/` | 旧 `docs/ai-agent-archi/` 和 `docs/others/` 的历史文档，保留作为参考 |
| 技术方案文档 | `docs/technologies/` | LLM 驱动层、Pi 对照、Harness Engineering 等 |
| 实现层文档 | `docs/uncode-technologies/` | 与源码同步：会话模型、术语表、Pi 机制对照 |

## Key Design Decisions

- **Language**: Rust edition 2024, MSRV 1.91
- **Unsafe code**: denied (`unsafe_code = "deny"` in workspace lints)
- **Error handling**: anyhow for application code, thiserror for library crate error types
- **Async runtime**: tokio (full features)
- **Config**: Two paths — `~/.config/uncode/config.json` (CLI: model + provider API keys) and `~/.uncode/` (extensions, skills, `config.toml`)
- **Session format**: tree-shaped `SessionEntry` in **SurrealDB** (embedded); JSONL for import/export and migration
- **Platform frontend**: React 19 + TanStack Router/Query + Vite, TypeScript strict mode
- **Cargo profiles**: dev uses `opt-level = 1` + `line-tables-only` for fast incremental builds; release uses LTO + strip

## 重要约定（文档及 Issues 优先原则）

**新功能开发前，必须先有对应文档和 Issue。** 设计决策先写入 `docs/` 目录下的对应文档，确认后检查 GitHub Issues 是否有对应 Issue，如果没有应当及时创建，然后再开始编码。

**此原则不适用于：测试、错误修复

## Development Workflow

- **GitHub Flow**: main ← PR ← feature-branch, PR body references issues with `closes #N`
- **Branch naming**: `feat/N-desc`, `fix/N-desc`, `refactor/N-desc`, `docs/N-desc`, `test/N-desc`, `perf/N-desc`
- **Documentation language**: Chinese (中文)
- **Keep main green**: main branch must always build and pass all tests
- **Commit format**: `type: description (refs #N)` — types: feat, fix, docs, refactor, test, perf, chore
- **PR format**: title uses `(refs #N)`，body contains `closes #N` for auto-close on merge

### 推送前必须本地执行 CI 预检测

推送代码或创建 PR 前，**必须**在本地运行以下五项检查，全部通过后才能推送：

```bash
RUSTFLAGS="-D warnings" cargo fmt --check --all
RUSTFLAGS="-D warnings" cargo clippy --all-targets --no-deps
RUSTFLAGS="-D warnings" cargo build --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
RUSTFLAGS="-D warnings" cargo test --workspace -- --test-threads=1
```

## Test Caveats

`uncode-agent/src/tools/` tests use `set_current_dir()` for sandbox isolation, which is process-global. These tests **must** run single-threaded: `cargo test --workspace -- --test-threads=1`. Running `cargo test --workspace` with default parallelism may cause intermittent failures in tools tests. CI also uses `--test-threads=1`.

## Terminology Strategy (Policy C)

Four layers: L0 (industry terms), L1 (mechanism names aligned with Pi), L2 (Rust API own naming), L3 (UI strings). Do NOT rename Rust public symbols to match Pi's TypeScript names. Glossary reference: `docs/references/HARNESS_ENGINEERING_GLOSSARY.md`.
