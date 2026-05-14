# Changelog

## [0.1.0] — 2026-05-14

### Added
- 13 Rust crates: core, macros, llm, session, tools, extensions, agent, tui, rpc, platform, cli
- 7 LLM providers: GLM, DeepSeek, Ollama, OpenAI, Anthropic, Gemini, OpenRouter
- 8 built-in tools: read, write, edit, grep, bash, find, ls + GitHub API
- 4-panel TUI (tasks, tools, thinking, summary) with /simple mode
- Markdown rendering (pulldown-cmark) + syntax highlighting
- Tab completion + command history + Emacs editing
- Diff viewer with multi-file n/p navigation
- Overlay selector for models/sessions
- Agent loop engine with Steering/FollowUp message queues
- Context compaction (token estimation + LLM summarization)
- SystemPromptBuilder with tool guide + context + skills injection
- ContextLoader (AGENTS.md/CLAUDE.md traversal)
- SKILL.md skills system
- Token estimation + cost tracking (7 model pricing)
- Session persistence (JSONL) + branching (SessionManager::branch_session)
- StopCondition trait (step_count_is, text_contains)
- CompletionRequestBuilder (builder pattern)
- `#[tool]` macro with auto JSON Schema generation
- WASM extension framework (8 lifecycle hooks)
- JSON-RPC 2.0 over stdio (uncode-rpc)
- Platform backend (axum + REST API)
- Platform frontend (React 19 + TanStack)
- GitHub API integration (Issue fetch + PR creation)
- GitHub Actions CI (build/test/fmt/clippy)
- Docker distribution (Dockerfile + docker-compose.yml)
- Shell completions (--completions flag)
- Session analysis dashboard (timeline view)
- Multimodal support (ContentBlock::Image)
- Model hot-switching with JSONL logging
- TUI error states (user-friendly per-category messages)
- 42 unit tests + 4 golden integration tests
- 7 design documents (VISION, TUI, PLATFORM, ARCHITECTURE, SESSION_SCHEMA, FDE_INSIGHT)

### Infrastructure
- Workspace-based monorepo (Cargo workspace)
- Rust edition 2024, MSRV 1.85
- Zero clippy warnings, zero build warnings
- MIT License
