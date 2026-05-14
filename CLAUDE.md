# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

uncode is a Rust-native AI Agent Coding system with two user-facing components: a TUI for front-line deployment engineers and a web Platform for software engineers. It supports 7 LLM providers (GLM, DeepSeek, Ollama, OpenAI, Anthropic, Gemini, OpenRouter) with streaming-first architecture.

## Build & Development Commands

```bash
cargo build --workspace          # Build all crates
cargo build -p uncode-cli        # Build CLI only
cargo test --workspace           # Run all tests
cargo test -p uncode-core        # Run single crate tests
cargo test -p uncode-core test_name  # Run single test
cargo fmt --check                # Format check
cargo clippy --all-targets       # Lint
cargo run -p uncode-cli -- --model deepseek-v3 "prompt"  # Run CLI
cd apps/platform && bun install && bun dev   # Platform frontend dev server
cd apps/platform && bun run build           # Platform frontend build
cd apps/platform && bun run lint            # Platform frontend lint
```

CI runs: `cargo fmt --check`, `cargo clippy --all-targets --no-deps`, `cargo build --workspace`, `cargo test --workspace` with `RUSTFLAGS="-D warnings"`.

## Architecture

Strict layered dependency graph — upper layers depend on lower, never the reverse:

```
uncode-cli (entry point, clap arg parsing)
├── uncode-tui (ratatui + crossterm, conversation-driven terminal UI)
├── uncode-platform (axum REST backend)
└── uncode-rpc (JSON-RPC, planned)
        │
    uncode-agent (agent loop engine, system prompts, token estimation, context compression)
        ├── uncode-llm (LLM driver trait + 7 provider implementations + registry)
        ├── uncode-session (JSONL session persistence)
        ├── uncode-tools (8 built-in tools: read/write/edit/grep/bash/find/ls + GitHub API)
        └── uncode-extensions (WASM extension runtime)
                │
            uncode-core (shared types, traits, errors, events — leaf crate, no internal deps)
            uncode-macros (proc macros: #[tool], #[derive(Event)] — compile-time only)
```

Cross-layer communication uses event streams. Upper layers subscribe to events, not direct calls.

## Key Design Decisions

- **Language**: Rust edition 2024, MSRV 1.85
- **Unsafe code**: denied (`unsafe_code = "deny"` in workspace lints)
- **Error handling**: anyhow for application code, thiserror for library crate error types
- **Async runtime**: tokio (full features)
- **Config**: TOML at `~/.uncode/config.toml`
- **Session format**: JSONL with branch support
- **Code parsing**: tree-sitter (10 languages)
- **Platform frontend**: React 19 + TanStack Router/Query + Vite, TypeScript strict mode

## 重要约定（文档及 Issues 优先原则）

**没有文档和 Issues 不能开发。** 设计决策先写入 `docs/` 目录下的对应文档，确认后检查 GitHub Issues 是否有对应 Issue，如果没有应当及时创建，然后再开始编码。

**此原则不适用于测试、错误修复。**

## Development Workflow

- **GitHub Flow**: main ← PR ← feature-branch, PRs reference issues with `closes #N`
- **Branch naming**: `feat/N-desc`, `fix/N-desc`, `refactor/N-desc`, `docs/N-desc`, `test/N-desc`, `perf/N-desc`
- **Documentation language**: Chinese (中文)
- **Keep main green**: main branch must always build and pass all tests

## LLM Provider Implementations

All providers implement a common trait in `uncode-llm`. Provider implementations live in `uncode-llm/src/providers/`. The registry pattern allows runtime model switching.
