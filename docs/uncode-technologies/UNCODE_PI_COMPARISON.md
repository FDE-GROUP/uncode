# uncode ↔ Pi 架构与功能对比分析

> 基于对 `~/EA/pi` (TypeScript monorepo, v0.0.3) 与 `~/EA/uncodenow` (Rust workspace) 源码的逐模块深度扫描。
> 日期：2026-05-21
> 修订：2026-05-22 — 对齐 Compaction 系统（工具截断、Turn 分割摘要、可配置参数、摘要注入）

---

## 1. 架构层对比

| 维度 | Pi (TypeScript) | uncode (Rust) | 对齐状态 |
|------|-----------------|---------------|----------|
| 包结构 | 5 packages (ai, agent, coding-agent, tui, web-ui) | 10 crates (shared, macros, ai, core, extensions, agent, tui, rpc, platform, cli) | ✅ 对齐，uncode 拆分更细 |
| 分层方向 | ai → agent → coding-agent → tui | shared/macros → ai/core/extensions → agent → tui/platform → cli | ✅ 对齐 |
| 跨层通信 | EventEmitter (pub/sub) | AgentEvent broadcast + EventRouter (双通道) | ✅ 对齐，uncode 多了 hook 控制通道 |
| 入口 | coding-agent CLI | uncode-cli (clap) | ✅ 对齐 |

---

## 2. Agent Loop 对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| 双层循环 | `agentLoop()` 外层 follow-up + 内层 tool call | `AgentLoop::run_inner()` 外层 follow-up + 内层 tool/stream | ✅ 对齐 |
| 编排器 | `Agent` class (stateful, owns transcript + queues) | `AgentHarness` (phase guard + session persistence + compaction trigger) | ✅ 对齐 |
| Steering | `steer()` → one-at-a-time drain | `steer()` → drained by inner loop | ✅ 对齐 |
| Follow-up | `followUp()` → outer loop restart | `follow_up()` → outer loop injection | ✅ 对齐 |
| Next-turn | 无独立队列，通过 `prompt()` | `next_turn()` 独立队列 | ⚠️ uncode 多了此队列 |
| Turn 上限 | 未硬编码 | `MAX_TURNS = 50` | ⚠️ uncode 更保守 |
| Phase 概念 | 无显式 phase | Idle/Turn/Compaction/BranchSummary/Retry | ⚠️ uncode 多了 phase 机制 |
| PhaseSummary | 无 | 启发式或 LLM 生成的 Turn 总结 | ❌ uncode 独有 |

**结论**：核心双层循环机制完全对齐，uncode 额外引入了 Phase 状态机和 PhaseSummary。

---

## 3. Tool 系统对比

### 3.1 工具清单

| 工具 | Pi | uncode | 备注 |
|------|-----|--------|------|
| `read` | ✅ | ✅ | 均有 offset/limit |
| `write` | ✅ | ✅ | 均原子写入 |
| `edit` | ✅ | ✅ | uncode 支持 hashline + legacy |
| `bash` | ✅ | ✅ | 均有 timeout、取消 |
| `grep` | ✅ | ✅ | |
| `find` | ✅ | ✅ | |
| `ls` | ✅ | ✅ | |
| `web_fetch` | ❌ | ✅ | uncode 独有 |
| `web_search` | ❌ | ✅ | uncode 独有 (Tavily) |

### 3.2 机制对比

| 机制 | Pi | uncode | 对齐 |
|------|-----|--------|------|
| 注册 | `createCodingTools()` factory | `ToolRegistry` + `register_coding_tools()` | ✅ |
| 并行/顺序执行 | `executionMode` per tool | `execution_mode()` per tool | ✅ |
| before/after hooks | `beforeToolCall`/`afterToolCall` config | `ToolHooks` trait (before/after) | ✅ |
| 沙箱路径 | CWD 限制 | `normalize_path` + `resolve_path` | ✅ |
| 自定义渲染 | `renderCall`/`renderResult` | `ToolRendererRegistry` | ✅ |
| 动态 Tool 注册 | `pi.registerTool()` 含自定义渲染 | 无 | ❌ |

**结论**：核心 7 工具完全对齐。uncode 多了 web_fetch/web_search。Pi 多了运行时动态 Tool 注册。

---

## 4. LLM Provider 对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| API 协议数 | 9 种 | 4 种 | ⚠️ Pi 多 5 种 |
| 覆盖供应商 | 30+ (含 Azure, Bedrock, Vertex, Copilot, xAI 等) | 10+ (DeepSeek, GLM, OpenAI, Anthropic, Gemini, Ollama, OpenRouter, Groq, Cerebras, Mistral, xAI) | ⚠️ uncode 缺少 Azure/Bedrock/Vertex/Copilot |
| 扩展供应商 | `pi.registerProvider()` + 自定义 stream | `UserModelConfig` + compat flags | ⚠️ Pi 更灵活 |
| 内置模型 | 动态加载，模型极多 | 14 个内置模型 | ⚠️ Pi 模型覆盖更广 |
| OAuth | Anthropic/Copilot/OpenAI OAuth 流程 | 无 | ❌ uncode 缺失 |
| API-first 架构 | 按协议组织驱动 | 按协议组织驱动 | ✅ 对齐 |
| Thinking/Reasoning | `ThinkingLevel` 6 级 | `ThinkingLevel` 5 级 + Vision | ✅ 基本对齐 |

### uncode 缺失的协议

| 协议 | 用途 |
|------|------|
| OpenAI Responses API | OpenAI 新一代 API |
| Azure OpenAI Responses | Azure 云部署 |
| Amazon Bedrock Converse Stream | AWS 云部署 |
| Google Vertex AI | Google 云部署 |
| Mistral Conversations (独立协议) | Mistral 专用接口 |

**结论**：API-first 架构理念对齐，但 uncode 协议覆盖和供应商支持明显少于 Pi。OAuth 完全缺失。

---

## 5. Event 系统对比

### 5.1 核心 Agent 事件

| Pi 事件 | uncode 事件 | 对齐 |
|---------|------------|------|
| `agent_start/end` | `SessionStart/End` | ✅ |
| `turn_start/end` | `TurnStart/End` | ✅ |
| `message_start/update/end` | `MessageStart/End` + `ContentDelta` | ✅ |
| `tool_execution_start/update/end` | `ToolCallStart/Progress/End` | ✅ |

### 5.2 差异事件

| 事件 | 归属 | 说明 |
|------|------|------|
| `ToolCallAwaitingApproval` | uncode | 权限确认事件 |
| `CompactionComplete` | uncode | 压缩完成通知 |
| `PhaseSummary`/`TaskUpdate` | uncode | Phase 状态机相关 |
| `MessageQueued/Delivered` | uncode | 队列状态追踪 |
| `AgentSettled`/`AgentInterrupted` | uncode | 生命周期精细控制 |
| `model_select`/`thinking_level_select` | Pi | 模型/思维级别切换事件 |

### 5.3 Extension 事件

| 维度 | Pi | uncode |
|------|-----|--------|
| 扩展事件数 | 25+ typed events | 8 lifecycle hooks |
| Event Bus | Node.js EventEmitter | DashMap + hook dispatch |

**结论**：核心 agent 生命周期事件对齐。uncode 多了权限和队列相关事件；Pi 的扩展事件体系远比 uncode 丰富。

---

## 6. Session 模型对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| 存储格式 | JSONL 文件树 | SurrealDB (RocksDB) + JSONL 导出 | ⚠️ 物理存储不同 |
| 逻辑模型 | 树状 `id/parentId` | 树状 `id/parent_id` (UUIDv7) | ✅ 对齐 |
| Entry 类型数 | 9 种 | 12 种 | ✅ 对齐 (uncode 多 Branch/Leaf/System) |
| 分支 | `branch(entryId)` + leafId | `fork_session()` + leaf_id | ✅ 对齐 |
| 导入/导出 | JSONL, HTML, GitHub Gist | JSONL, HTML | ⚠️ Pi 多 Gist 分享 |
| Session 恢复 | `continueRecent()`, `inMemory()` | `find_most_recent()` | ✅ 基本对齐 |

**结论**：逻辑模型完全对齐。物理存储不同（SurrealDB vs JSONL 文件）是刻意的设计选择——SurrealDB 提供更强的查询能力。

---

## 7. Context/Compaction 对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| Token 估算 | `chars / 4` | `chars / 4` | ✅ 完全对齐 |
| 触发阈值 | `contextWindow - reserveTokens` | `(contextWindow - reserveTokens) × threshold%` | ✅ 已对齐 |
| 保留最近 | `keepRecentTokens = 20000` (固定值) | `keep_recent_tokens = 20000` (可配置固定值) | ✅ 已对齐 |
| 预留 token | `reserveTokens = 16384` | `reserve_tokens = 16384` (可配置) | ✅ 已对齐 |
| 裁剪点选择 | 仅在 user/assistant/custom/bashExecution | 寻找 User 消息做 turn 边界 | ✅ 对齐 |
| Turn 分割检测 | `isSplitTurn()` | `is_split_turn()` | ✅ 对齐 |
| Turn 分割摘要 | 生成 turn prefix summary + `---` 合并 | 生成 turn prefix summary + `---` 合并 | ✅ 已对齐 |
| 工具结果截断 | 2000 字符 + 截断标记 | 2000 字符 + `[... N more characters truncated]` | ✅ 已对齐 |
| 摘要生成 | LLM 生成结构化摘要 + 增量摘要 | LLM 生成 8 段结构化摘要 + 增量 | ✅ 对齐 |
| 摘要注入 | `<summary>` XML + user role | `<summary>` XML + user role | ✅ 已对齐 |
| 文件追踪 | 追踪 read/modified 文件跨压缩 | 追踪 read/modified 文件跨压缩 + XML 标签注入摘要 | ✅ 已对齐 |
| 可配置参数 | `CompactionSettings` (enabled/reserveTokens/keepRecentTokens) | `CompactionConfig` (enabled/threshold_percent/keep_recent_tokens/reserve_tokens) | ✅ 已对齐 |
| 扩展钩子 | `session_before_compact` (可取消/自定义摘要) | 无（依赖 WASM 扩展运行时） | ❌ uncode 缺失 |

**结论**：Compaction 系统已全面对齐 Pi 的核心能力：工具结果截断（2000 字符）、Turn 分割摘要生成、`<summary>` XML 注入 + user role、可配置压缩参数。uncode 额外保留了 Phase 互斥（防并发压缩）、8 段结构化摘要格式、PRESERVE/ADD/MOVE/UPDATE 增量合并规则等独有优势。唯一差距为 Pi 的 `session_before_compact` 扩展钩子（依赖 Extension 运行时实现）。

---

## 8. TUI 对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| 框架 | 自建 TUI framework (differential rendering) | ratatui + crossterm | ⚠️ 实现不同 |
| 流式显示 | ✅ | ✅ | ✅ |
| Thinking 可见性 | ✅ 折叠/展开 | ✅ Ctrl+T 切换 | ✅ |
| Tool 卡片 | ✅ 自定义渲染器 | ✅ ToolRendererRegistry | ✅ |
| 虚拟滚动 | ✅ | ✅ | ✅ |
| 主题 | ✅ 热重载 | ✅ 多主题 | ✅ |
| 斜杠命令 | 18 个 | 21 个 | ✅ uncode 更多 |
| 快捷键 | 15+ | 15+ | ✅ 基本对齐 |
| 图片支持 | ✅ 粘贴、内联渲染、自动缩放 | ❌ | ❌ uncode 缺失 |
| 外部编辑器 | Ctrl+G | Ctrl+G ($EDITOR) | ✅ |
| Follow-up 排队 | Alt+Enter | `/later <msg>` | ✅ 功能等价 |
| Model 选择器 | Ctrl+L overlay | Ctrl+L overlay | ✅ |
| Session 树导航 | `/tree` | `/tree` | ✅ |
| undo turn | Ctrl+/ | Ctrl+/ | ✅ |

**结论**：核心 TUI 交互对齐。主要缺失：图片支持（粘贴、内联渲染、Vision）。

---

## 9. Extension 系统对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| 运行时 | TypeScript (jiti 运行时加载) | WASM (规划中) | ❌ uncode 未实现 |
| 发现路径 | 3 路径 (cwd/global/configured) | `~/.uncode/extensions/` | ⚠️ uncode 简化 |
| 加载状态 | 完整加载 + 50+ 示例扩展 | `loader.rs` 返回 `Ok(0)` — **未实现** | ❌ 完全未对齐 |
| Event 订阅 | 25+ typed events | 8 lifecycle hooks | ⚠️ 机制有，粒度差 |
| Tool 注册 | `pi.registerTool()` 含自定义渲染 | 无动态注册 | ❌ |
| Command 注册 | `pi.registerCommand()` | 无 | ❌ |
| Shortcut 注册 | `pi.registerShortcut()` | 无 | ❌ |
| Provider 注册 | `pi.registerProvider()` + OAuth | 无 | ❌ |
| 自定义消息渲染 | `pi.registerMessageRenderer()` | 无 | ❌ |
| Flag 注册 | `pi.registerFlag()` | 无 | ❌ |
| SDK/编程接口 | 完整 SDK + 13 示例 | 无 | ❌ |
| 示例扩展数 | 50+ (auto-commit, permission-gate, snake game, SSH, plan-mode, subagent 等) | 0 | ❌ |

**结论**：这是最大的差距。Pi 的扩展系统极其成熟（50+ 示例扩展、完整 API、动态注册），uncode 的 WASM 扩展运行时完全未实现。

---

## 10. Permission/Safety 对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| beforeToolCall hook | ✅ 可 block | ✅ 可 block | ✅ |
| afterToolCall hook | ✅ 可修改结果 | ✅ 可修改结果 | ✅ |
| Permission gate | 通过扩展实现 (confirm-destructive, protected-paths) | 内置 `PermissionGate` + TUI 确认 + `PermissionPolicy` | ✅ 已对齐 |
| Protected paths | `protected-paths` 扩展 | 内置 `PermissionConfig.protected_paths` (glob 匹配) | ✅ 已对齐 |
| Dangerous bash detection | `permission-gate` 扩展 | 内置 `PermissionConfig.dangerous_bash_patterns` (regex 匹配) | ✅ 已对齐 |
| Configurable safe commands | 扩展自定义 | `PermissionConfig.extra_safe_commands` | ✅ 已对齐 |
| Auto-allow readonly | 扩展自行决定 | 内置 `auto_allow_readonly` | ✅ |
| Output guard | ✅ `output-guard.ts` (stdout 劫持) | `output_guard` 模块 (tracing 写 stderr + `write_raw_stdout`) | ✅ 已对齐 |

**结论**：Permission/Safety 已完全对齐。策略不同（Pi 用扩展、uncode 用内置 `PermissionPolicy`），但功能等价：均覆盖 protected paths、dangerous bash 检测、可配置安全命令、output guard。uncode 额外支持用户通过 `config.json` 自定义策略。

---

## 11. Skills 对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| 格式 | Markdown + YAML frontmatter | Markdown + YAML frontmatter | ✅ |
| 发现路径 | cwd + global + configured | `~/.uncode/skills/**/*.md` | ⚠️ uncode 少 cwd 级 |
| 内置 skills | 无明确内置 | 5 个 (code-review, explain, test-gen, refactor, security-audit) | ⚠️ 策略不同 |
| 注册为命令 | `/skill:name` | `/<skill_name>` | ✅ |

---

## 12. Platform/Web UI 对比

| 维度 | Pi | uncode | 对齐状态 |
|------|-----|--------|----------|
| Web UI | `web-ui` package — 框架无关组件库 (ChatPanel, AgentInterface, Artifact 渲染器等) | `apps/platform` — React 19 + TanStack 前端 | ⚠️ 方向不同 |
| 后端 | RPC mode (stdin/stdout JSON-lines) | Axum REST + WebSocket | ⚠️ 策略不同 |
| GitHub Issues | 无 | ✅ Issues proxy | ⚠️ uncode 独有 |
| Metrics/分析 | 无内置 | ✅ Session metrics + optimization suggestions | ⚠️ uncode 独有 |
| Artifact 渲染 | HTML/PDF/SVG/Image/Markdown/Excel/DOCX | 无 | ❌ uncode 缺失 |
| Session 存储 | IndexedDB (客户端) | SurrealDB (服务端) | ⚠️ 架构不同 |

---

## 13. 总体对齐度评估

### 已对齐模块

| 模块 | 对齐度 | 说明 |
|------|--------|------|
| Agent 双层循环 | **95%** | 核心机制完全对齐，uncode 多 Phase 状态机 |
| Tool 系统 (7 核心工具) | **95%** | 功能和行为对齐 |
| Session 逻辑模型 | **95%** | 树状 entry 完全对齐 |
| Context/Compaction + 文件追踪 | **98%** | 算法 + 文件追踪 + Turn 分割摘要 + 可配置参数 + `<summary>` 注入全部对齐 |
| Permission/Safety | **95%** | 内置 `PermissionPolicy` 覆盖 Pi 扩展式权限的全部核心能力 |
| Event 系统 | **85%** | 核心事件对齐，扩展事件不足 |
| LLM API-first 架构 | **80%** | 理念对齐，协议覆盖不足 |
| TUI 交互 | **85%** | 核心操作对齐，缺图片 |
| Skills 系统 | **80%** | 格式对齐，发现路径简化 |

### 未对齐的关键差距

| 模块 | 差距 | 优先级 | 影响 |
|------|------|--------|------|
| **Extension 运行时** | 完全未实现 (WASM loader 返回空) | **P0** | Pi 最核心的差异化能力，50+ 示例扩展生态 |
| **LLM 协议覆盖** | 缺 5 种协议 (Responses/Azure/Bedrock/Vertex/Mistral) | **P1** | 企业用户无法接入云服务 |
| **OAuth 认证** | 完全缺失 | **P1** | 无法使用 Copilot/Codex 等需 OAuth 的服务 |
| **动态注册能力** | Tool/Command/Shortcut/Provider/Flag 注册全部缺失 | **P1** | 依赖 Extension 运行时实现 |
| **SDK/编程接口** | 无 | **P2** | 无法嵌入其他应用 |
| **图片支持** | 粘贴、内联渲染、Vision 均缺失 | **P2** | 多模态能力缺口 |
| **Artifact 渲染** | HTML/PDF/SVG/Excel/DOCX 渲染缺失 | **P3** | Web UI 富内容展示 |

### 已对齐的差距

| 原差距 | 对齐方式 | 实现位置 | 对齐日期 |
|--------|----------|----------|----------|
| ~~文件追踪~~ | 跨压缩周期合并 + XML 标签注入摘要 | `compaction.rs` | 2026-05-21 |
| ~~Protected paths~~ | 内置 `PermissionConfig.protected_paths` (glob) | `config.rs` + `tool_permission.rs` | 2026-05-21 |
| ~~Dangerous bash detection~~ | 内置 `PermissionConfig.dangerous_bash_patterns` (regex) | `config.rs` + `tool_permission.rs` | 2026-05-21 |
| ~~Configurable safe commands~~ | 内置 `PermissionConfig.extra_safe_commands` | `config.rs` + `tool_permission.rs` | 2026-05-21 |
| ~~Output guard~~ | `output_guard` 模块 + tracing 写 stderr | `main.rs` | 2026-05-21 |
| ~~工具结果截断过短~~ | `TOOL_RESULT_MAX_CHARS` 200 → 2000 + 截断标记 | `compaction.rs` | 2026-05-22 |
| ~~压缩参数硬编码~~ | 新增 `CompactionConfig`（enabled/threshold_percent/keep_recent_tokens/reserve_tokens） | `config.rs` + `compaction.rs` + `loop_engine.rs` | 2026-05-22 |
| ~~Turn 分割只退回边界~~ | 生成 turn prefix summary + `---` 合并主摘要 | `compaction.rs` | 2026-05-22 |
| ~~摘要注入 system role~~ | `<summary>` XML 标签 + user role 注入 | `compaction.rs` + `context_builder.rs` | 2026-05-22 |

### uncode 独有的超出部分

| 模块 | 说明 |
|------|------|
| web_fetch / web_search 工具 | Pi 核心工具集不含 |
| Platform 后端 (Axum) | Session 分析、GitHub Issues proxy、Metrics、优化建议 — Pi 无内置 |
| SurrealDB 存储 | 嵌入式数据库，查询能力比 JSONL 文件更强 |
| Phase 状态机 | 更精细的执行阶段管理 (Idle/Turn/Compaction/BranchSummary/Retry)，防并发压缩 |
| 8 段结构化摘要 | Goal/Constraints/Progress/Decisions/Next Steps/Critical Context 固定格式 |
| PRESERVE/ADD/MOVE/UPDATE 增量规则 | 显式 LLM 指令保证增量摘要质量 |
| Template 系统 | 6 内置 + 用户 TOML 模板 |
| Session HTML 导出 | 自包含 HTML 带样式 |
| PermissionPolicy 可配置 | 用户通过 `config.json` 自定义保护路径、危险命令 pattern、扩展安全命令 — Pi 需写扩展实现同等能力 |
| CompactionConfig 可配置 | 用户通过 `config.json` 自定义压缩阈值、保留 token 数、预留 token 数 — Pi 需修改 settings 文件 |
