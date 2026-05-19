# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

uncode is a Rust-native AI Agent Coding system with two user-facing components: a TUI for front-line deployment engineers and a web Platform for software engineers. It supports 7 LLM providers (GLM, DeepSeek, Ollama, OpenAI, Anthropic, Gemini, OpenRouter) with streaming-first architecture.

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
cargo run -p uncode-cli -- --model deepseek-v3 "prompt"  # Run CLI
cd apps/platform && bun install && bun dev   # Platform frontend dev server
cd apps/platform && bun run build           # Platform frontend build
cd apps/platform && bun run lint            # Platform frontend lint
```

CI runs: `cargo fmt --check`, `cargo clippy --all-targets --no-deps`, `cargo build --workspace`, `cargo test --workspace` with `RUSTFLAGS="-D warnings"`.

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
        └── uncode-extensions (WASM extension runtime)
                │
            uncode-shared (error types + config — leaf crate)
            uncode-macros (proc macros: #[tool], #[derive(Event)] — compile-time only)
```

Cross-layer communication uses event streams. Upper layers subscribe to events, not direct calls.

### LLM Provider Architecture

All providers implement `Api` trait (`uncode-ai/src/api.rs`). 4 API protocol implementations in `uncode-ai/src/providers/`: openai_completions (covers OpenAI/DeepSeek/GLM/OpenRouter/Ollama-compatible), anthropic_messages, gemini_generative, ollama_native. Tool calls follow a three-stage protocol: `ToolCallStart` → `ToolCallDelta` → `ToolCallEnd`. Every stream must end with `StreamEvent::Done`.

### Tool System

7 tools in `uncode-agent/src/tools/` (read, write, edit, grep, bash, find, ls). Each implements `ToolExecutor` trait. All tools use `normalize_path()` + `resolve_path()` for sandbox enforcement — files must stay within CWD.

## Key Design Decisions

- **Language**: Rust edition 2024, MSRV 1.85
- **Unsafe code**: denied (`unsafe_code = "deny"` in workspace lints)
- **Error handling**: anyhow for application code, thiserror for library crate error types
- **Async runtime**: tokio (full features)
- **Config**: TOML at `~/.uncode/config.toml`
- **Session format**: tree-shaped `SessionEntry` in **SurrealDB** (embedded); JSONL for import/export and migration (logical model aligned with Pi — see `docs/technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`)
- **Platform frontend**: React 19 + TanStack Router/Query + Vite, TypeScript strict mode
- **Cargo profiles**: dev uses `opt-level = 1` + `line-tables-only` for fast incremental builds; release uses LTO + strip

## 重要约定（文档及 Issues 优先原则）

**没有文档和 Issues 不能开发。** 设计决策先写入 `docs/` 目录下的对应文档，确认后检查 GitHub Issues 是否有对应 Issue，如果没有应当及时创建，然后再开始编码。

**此原则不适用于测试、错误修复。**

## Development Workflow

- **GitHub Flow**: main ← PR ← feature-branch, PRs reference issues with `closes #N`
- **Branch naming**: `feat/N-desc`, `fix/N-desc`, `refactor/N-desc`, `docs/N-desc`, `test/N-desc`, `perf/N-desc`
- **Documentation language**: Chinese (中文)
- **Keep main green**: main branch must always build and pass all tests

### 提交前必须本地执行 CI 预检测

推送代码或提交审核前，**必须**在本地运行以下四项检查，全部通过后才能 push：

```bash
RUSTFLAGS="-D warnings" cargo fmt --check --all
RUSTFLAGS="-D warnings" cargo clippy --all-targets --no-deps
RUSTFLAGS="-D warnings" cargo build --workspace
RUSTFLAGS="-D warnings" cargo test --workspace
```

## Test Caveats

`uncode-agent/src/tools/` tests use `set_current_dir()` for sandbox isolation, which is process-global. These tests **must** run single-threaded: `cargo test --workspace -- --test-threads=1`. Running `cargo test --workspace` with default parallelism may cause intermittent failures in tools tests.
