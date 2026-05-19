# Pi Agent Rust vs Uncode 对比分析报告

> 分析对象：[pi_agent_rust](https://github.com/Dicklesworthstone/pi_agent_rust) (commit main) vs 本项目 uncode  
> 日期：2026-05-18（第二次修订，反映 SurrealDB 迁移后状态）

> **说明**：本文对比的是第三方仓库 **pi_agent_rust**，与 [earendil-works/pi](https://github.com/earendil-works/pi)（TypeScript 上游）及 [`UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) 中的「与 Pi 对齐」**不是同一分析对象**；需要 Pi 本体与 uncode 的对照时请读该对齐文档与 `docs/pi-technologies/`。

## 1. 项目概览

| 维度 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| GitHub Stars | 972 | 内部项目 |
| Commits | 3,336+ | — |
| 语言 | Rust (edition 2024, nightly) | Rust (edition 2024, MSRV 1.85) |
| Unsafe | `#![forbid(unsafe_code)]` | `unsafe_code = "deny"` |
| Crate 结构 | 单 crate (~80+ 源文件, ~30K+ LOC) | 8 crate workspace (~15K LOC) |
| TUI 框架 | charmed_rust (Bubble Tea / Elm Architecture) | ratatui + crossterm |
| 异步运行时 | asupersync (自研结构化并发运行时) | tokio (full features) |
| LLM 调用 | reqwest + 自定义 SSE 状态机 | reqwest + 自定义流式解析 |
| 扩展系统 | QuickJS 嵌入式 JS 引擎 + 1001 扩展 | WASM 运行时 (规划中) |
| 会话存储 | JSONL v3 + SQLite 索引 + V2 Sidecar | SurrealDB v3 (kv-rocksdb) |
| 序列化 | serde + postcard (二进制) | serde + JSON |
| 配置格式 | TOML | TOML |
| 贡献模式 | 不接受外部贡献，仅 Issues/PR 参考 | GitHub Flow，团队协作 |

## 2. 架构对比

### 2.1 Pi Agent Rust：单体深度架构

```
src/
├── main.rs              # 入口 (clap)
├── agent/               # Agent 核心循环
├── llm/                 # LLM 抽象层 (10 原生 provider + 兼容预设)
├── tools/               # 7 内置工具 + hashline_edit
├── tui/                 # Elm Architecture TUI (charmed_rust)
├── extensions/          # QuickJS 扩展运行时 + capability 安全
├── session/             # JSONL v3 + SQLite 索引 + V2 Sidecar
├── diff/                # 独立 diff 引擎
├── process/             # 进程树管理 (sysinfo)
├── knowledge/           # 知识库
├── capability/          # Capability-based 安全模型
├── vendor/              # 内嵌依赖 (asupersync, charmed_rust, rich_rust)
├── oauth/               # OAuth 支持
├── math/                # 数学驱动决策系统 (CUSUM, BOCPD, PAC-Bayes)
└── ...
```

特点：

- 所有代码在单一 crate 内，通过 mod 组织
- 内嵌 3 个 vendor crate（asupersync, charmed_rust, rich_rust），零外部运行时依赖
- 文件数量多但模块边界清晰
- 自研异步运行时 asupersync，专为结构化并发设计

### 2.2 Uncode：Workspace 分层架构

```
uncode-cli          # 入口，clap 参数解析
├── uncode-tui      # ratatui TUI (组件化: ChatPanel, InputBar, StatusBar)
├── uncode-platform # axum Web 平台 (React 19 前端)
└── uncode-rpc      # JSON-RPC 2.0 over stdio
    └── uncode-agent    # Agent 引擎 (loop + harness + tools + compaction + skills)
        ├── uncode-ai       # LLM 抽象 (Api trait + 4 协议实现)
        ├── uncode-core     # 共享类型 (events, tool traits, session types)
        └── uncode-extensions  # WASM 扩展 (框架就绪)
            └── uncode-shared  # 配置 + 错误类型 (leaf crate)
            └── uncode-macros  # 过程宏 (#[tool], #[derive(Event)])
```

特点：

- 严格的三层依赖图，禁止循环依赖
- 上层通过事件流（`broadcast::Receiver<AgentEvent>`）订阅下层
- 每个 crate 有独立的 Cargo.toml 和测试
- SurrealDB 全异步存储，所有 SessionStore 方法为 `async fn`

### 2.3 架构对比总结

| 方面 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| 编译粒度 | 整体编译，改一行全量重编 | crate 级增量编译 |
| 依赖管理 | vendor 内嵌，零外部运行时 | Cargo workspace 统一管理 |
| 复用性 | 低（单体） | 高（core/ai 可独立发布） |
| 构建速度 | 慢（30K+ LOC 单 crate） | 快（增量编译 + opt-level=1） |
| 新人上手 | 简单（一个 crate） | 需理解 crate 间关系 |
| 运行时依赖 | asupersync（自研） | tokio（标准生态） |

## 3. LLM Provider 对比

### 3.1 Pi Agent Rust

10 个原生 provider + 大量 OpenAI 兼容预设：

| 原生 Provider | 特点 |
| --- | --- |
| Anthropic | Messages API |
| OpenAI Chat | Chat Completions |
| OpenAI Responses | Responses API |
| Google Gemini | Generative API |
| Cohere | Chat API |
| Azure OpenAI | Azure 部署 |
| AWS Bedrock | 托管模型 |
| Google Vertex | 托管模型 |
| GitHub Copilot | Copilot Chat |
| GitLab | GitLab AI |

兼容预设（通过 OpenAI 协议）：Groq, OpenRouter, Mistral, Together, DeepSeek, Cerebras, 等等。

每个 provider 有独立的 `compat` 配置，粒度极细：

- `thinking_format`: 各家的思维链格式不同
- `tool_call_style`: tool_choice / parallel_tool_calls 等
- `streaming_format`: SSE / NDJSON / 自定义
- `system_prompt_style`: system 角色位置
- OAuth 支持：Anthropic, OpenAI Codex, Google, GitHub Copilot, GitLab, Kimi

### 3.2 Uncode

4 种 API 协议实现，覆盖 7 个 provider：

| 协议实现 | 覆盖 Provider |
| --- | --- |
| openai_completions | OpenAI, DeepSeek, GLM, OpenRouter, Ollama |
| anthropic_messages | Anthropic |
| gemini_generative | Gemini |
| ollama_native | Ollama (原生工具调用) |

通过 `CompatConfig` 统一处理差异：

```rust
pub struct CompatConfig {
    pub thinking_format: Option<ThinkingFormat>,
    pub thinking_level: Option<ThinkingLevel>,
    pub tool_call_style: Option<ToolCallStyle>,
    // ...
}
```

### 3.3 Provider 对比总结

| 方面 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| 原生 Provider 数量 | 10 | 4（协议实现） |
| 覆盖 Provider | 10+ 大量兼容预设 | 7 |
| OAuth 认证 | ✅（6 家 OAuth） | ❌ |
| 代码复用 | 低（每家独立实现） | 高（协议共享） |
| 新增 Provider | 新建模块 | 添加 CompatConfig |
| 配置粒度 | 极细（每家独立 compat） | 中等（协议级 compat） |
| 测试覆盖 | 224 扩展 E2E + 25 场景 | 28 model/compat 测试 |

## 4. 工具系统对比

### 4.1 工具列表

| 工具 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| Read | ✅ | ✅ |
| Write | ✅ | ✅ (返回 unified diff) |
| Edit | ✅ hashline_edit (精确行级编辑) | ✅ hashline + 字符串替换双模式 |
| Bash | ✅ (进程树管理) | ✅ (进程组管理 + 信号传播) |
| Find/Glob | ✅ | ✅ |
| Ls | ✅ | ✅ |
| Grep | ✅ | ✅ |
| Web Fetch | ✅ | ✅ |
| Web Search | ✅ | ✅ (Tavily API, 可选) |
| Memory | ✅ | ✅ (Semantic Workspace Graph + Context Bundle) |
| Notebook Edit | ✅ | ❌ |
| Diff | ✅ (独立 diff 引擎) | ✅ (独立 diff 引擎, 结构化类型) |

### 4.2 工具执行模型

| 方面 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| Trait 定义 | 自定义 | `ToolExecutor` |
| 参数格式 | JSON | JSON |
| 沙箱 | normalize_path + CWD 限制 | normalize_path + resolve_path + CWD 边界检查 |
| 进度反馈 | 有 | 有 (ToolProgress) |
| Diff 展示 | 独立 diff 引擎，丰富渲染 | unified diff，TUI +/- 着色 |
| 进程管理 | 进程树追踪、超时、信号传播、孤儿回收 | 进程组管理 (process_group(0) + SIGKILL) |
| 编辑精度 | hashline_edit (行级定位) | hashline 行级定位 + 字符串替换双模式 |

## 5. 会话系统对比

### 5.1 Pi Agent Rust

- **三层存储**：
  - JSONL v3：主存储，每行一条消息，支持树状分支
  - SQLite 索引 sidecar：WAL 模式，消息元数据索引，支持快速查询
  - Session Store V2 sidecar：分段日志 + 偏移索引，大 session 快速恢复
- **恢复性能**：1M 消息 ~250ms，5M 消息 ~1.4s
- **Compaction**：阈值驱动，turn 边界切割点，文件操作追踪保留上下文
- **Truncation**：HEAD/tail 策略，2000 行/50KB 限制

### 5.2 Uncode

- **SurrealDB v3**：嵌入式文档-图数据库，kv-rocksdb 后端
- **Schema**：session / entry / leaf 三表，SCHEMAFULL + 索引
- **全异步 API**：所有 SessionStore 方法为 `async fn`
- **分支**：parent_id 链式结构，LeafEntry 指针，BranchEntry 分叉记录
- **Branch Summarization**：移动 leaf 时自动生成被弃分支的结构化摘要
- **JSONL 导入**：`import_jsonl_dir()` 支持从旧 JSONL 格式迁移
- **Compaction**：阈值驱动 + turn 边界切割 + split-turn 检测 + 文件操作追踪 + 迭代摘要

### 5.3 会话对比总结

| 方面 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| 存储引擎 | JSONL + SQLite + V2 Sidecar | SurrealDB v3 (kv-rocksdb) |
| 查询性能 | 快（SQLite 索引 + 分段日志偏移） | 快（SurrealDB 索引查询 O(1)） |
| 大 session 恢复 | V2 Sidecar ~250ms/1M 条 | 依赖 SurrealDB 查询性能 |
| 分支支持 | ✅ 树状分支 | ✅ parent_id 链 + LeafEntry |
| 分支摘要 | ❌ | ✅ 自动生成 |
| Compaction 智能度 | 高（文件操作追踪 + turn 边界） | 高（文件操作追踪 + turn 边界 + split-turn 检测 + 迭代摘要） |
| 可移植性 | 需 SQLite | 需 SurrealDB（嵌入式，无需外部服务） |
| 迁移支持 | `pi migrate` CLI | JSONL 自动导入 |

## 6. TUI 对比

### 6.1 Pi Agent Rust

- **框架**: charmed_rust (Rust 版 Bubble Tea)
- **架构**: Elm Architecture (Model-Update-View)
- **渲染**: rich_rust (Python Rich 库的 Rust 移植)
- **模式**: Interactive (全 TUI) / Print (脚本化) / RPC (无头 JSON)
- **特点**:
  - 丰富的 Markdown 渲染
  - 语法高亮
  - 进度条和 Spinner
  - 工具调用内联折叠展示
  - async/sync 桥接 (mpsc channel)

### 6.2 Uncode

- **框架**: ratatui + crossterm
- **架构**: 组件化 (ChatPanel, InputBar, StatusBar 等)
- **渲染**: 自定义 ToolRenderer 注册表
- **特点**:
  - 组件化布局
  - 工具调用卡片（折叠/展开）
  - Diff 着色（+/−/@@ 行）
  - 主题系统
  - 事件驱动（订阅 AgentEvent 广播）

### 6.3 TUI 对比总结

| 方面 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| 框架 | charmed_rust (Elm) | ratatui (immediate mode) |
| Markdown 渲染 | 丰富 (rich_rust：进度条/Spinner/面板布局) | 完整 GFM（标题/列表/代码高亮/表格/引用/任务列表） |
| 代码高亮 | ✅ | ✅ (syntect) |
| Diff 可视化 | 丰富 (独立引擎) | 结构化 Patch 类型 + +/-/@@ 着色 |
| 布局模式 | Elm 模式 | ratatui Layout |
| 跨平台 | 需端口适配 | crossterm 原生跨平台 |
| 额外模式 | Print / RPC | Web Platform / JSON-RPC |

## 7. 扩展系统对比

### 7.1 Pi Agent Rust — QuickJS 扩展生态

- **运行时**: QuickJS 嵌入式 JS 引擎（无 Node/Bun 依赖）
- **安全模型**: Capability-based 权限控制
  - 6 类 capability: tool / exec / http / session / ui / events
  - 两阶段执行强制：capability 门控 + 命令中介（阻止危险 shell 模式）
  - 信任生命周期：pending → acknowledged → trusted → killed
  - kill-switch 审计日志
  - 环境变量过滤（阻止凭证泄露）
  - 运行时风险账本（防篡改）
- **扩展规模**: 224 vendored + 777 unvendored = 1001 扩展
- **验证管线**: 三轨验证（vendored 合规 + unvendored + 发布二进制 E2E）
- **Hostcall 协议**: Fast lane vs 兼容 lane，shadow 双重执行，SHA-256 去重
- **包管理**: `pi install` / `pi remove` / `pi update`
- **技能系统**: SKILL.md drop-in 技能文件

### 7.2 Uncode — WASM 扩展（规划中）

- **运行时**: WASM 运行时 (规划)
- **Crate**: `uncode-extensions` 已创建，框架就绪
- **安全模型**: 待定

### 7.3 扩展对比总结

| 方面 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| 运行时 | QuickJS (成熟) | WASM (规划中) |
| 语言支持 | JavaScript | WASM (多语言) |
| 安全模型 | Capability-gated + 信任生命周期 | 待定 |
| 扩展数量 | 1001 | 0 |
| 验证管线 | 三轨 (224/224 通过) | 无 |
| 包管理 | ✅ pi install/remove/update | ❌ |
| 成熟度 | 生产级 | 框架就绪 |

## 8. 独有特性对比

### 8.1 Pi Agent Rust 独有

| 特性 | 说明 |
| --- | --- |
| 数学驱动决策系统 | CUSUM + BOCPD 制度转换检测，conformal prediction 包络，PAC-Bayes 安全边界 |
| SSE 状态机 | 12 事件变体，零拷贝，处理 TCP 包边界 |
| Session Store V2 | 分段日志 + 偏移索引，大 session 极速恢复 |
| OAuth 认证 | 6 家 provider OAuth 流程 |
| `pi doctor` | 诊断 CLI |
| `pi migrate` | 会话迁移 CLI |
| Swarm/Multi-agent | 多 agent 协作（规划/实验性） |
| 进程树管理 | sysinfo 进程追踪，信号传播，孤儿回收（uncode 已实现等价的进程组管理） |
| RPC 模式 | `--mode rpc`，结构化 JSON 协议，IDE 集成 |
| 主题系统 | 可定制 TUI 主题 |

### 8.2 Uncode 独有

| 特性 | 说明 |
| --- | --- |
| Web Platform | React 19 + TanStack Router/Query + Vite 前端 |
| axum REST 后端 | HTTP API，面向 Web 用户 |
| SurrealDB v3 存储 | 嵌入式文档-图数据库，纯 Rust |
| 分支摘要 | 移动 leaf 时自动生成被弃分支结构化摘要 |
| JSONL 导入 | 旧格式自动迁移 |
| 多 crate 架构 | 严格分层，独立编译，可复用 |
| 事件驱动集成 | `broadcast::Receiver<AgentEvent>` 跨层通信 |
| Semantic Workspace Graph | 代码符号图谱 + Context Bundle 自动注入，regex 提取 + 评分选取 + TTL 缓存 |
| Web Fetch / Web Search | reqwest + html2text (URL→文本) + Tavily API (可选搜索) |
| Diff 引擎 | 独立结构化 diff（Myers 算法），Patch/Hunk/DiffLine 类型供 TUI 直接消费 |

## 9. 可借鉴的设计

从 pi_agent_rust 中识别到以下值得 uncode 借鉴的设计模式：

### 9.1 进程树管理 ✅ 已实现

Pi 对 Bash 工具启动的子进程进行树状管理，支持超时、信号传播、孤儿进程回收。uncode 已通过 `process_group(0)` + `libc::kill(-pgid, SIGKILL)` 实现等价的进程组管理。

**状态**: 已实现 — `crates/uncode-agent/src/tools/bash.rs`

### 9.2 Compaction 文件操作追踪 ✅ 已实现

压缩上下文时跟踪哪些文件被操作过，在压缩结果中保留文件操作上下文。uncode 已实现 `extract_files_from_entries()` 追踪 files_read/files_modified，并在摘要 prompt 中传入文件上下文。支持迭代摘要和 split-turn 检测。

**状态**: 已实现 — `crates/uncode-agent/src/compaction.rs`

### 9.3 Capability-based 扩展安全模型

QuickJS 扩展通过 capability 声明权限，运行时验证，信任生命周期管理。uncode 的 WASM 扩展可以借鉴类似的声明式安全模型。

**优先级**: 中 — 扩展系统实现前需要确定安全模型

### 9.4 hashline_edit 精确编辑 ✅ 已实现

行级定位的编辑工具，避免字符串匹配替换的不确定性。uncode 已实现双模式 edit 工具：hashline 模式（行级锚点 + xxHash32 校验）和 legacy 模式（字符串替换）。

**状态**: 已实现 — `crates/uncode-agent/src/tools/edit.rs` + `hashline.rs`

### 9.5 SSE 状态机

12 事件变体的零拷贝 SSE 解析器，处理真实网络分块。比正则/行解析更健壮。

**优先级**: 低 — 当前流式解析在正常条件下可用

## 10. uncode 的优势

### 10.1 多 crate 架构

比单 crate 更适合：
- 团队协作（不同 crate 可独立开发）
- 增量编译（改一个 crate 不影响其他）
- 复用发布（core/ai crate 可被其他项目使用）
- 依赖隔离（每个 crate 只引入需要的依赖）

### 10.2 多用户界面

同时支持 TUI、Web Platform、JSON-RPC 三种前端：

| 前端 | 面向用户 | Pi Agent Rust |
| --- | --- | --- |
| TUI | 部署工程师 / 终端用户 | ✅ |
| Web Platform | 软件工程师 / 团队协作 | ❌ |
| JSON-RPC | IDE 集成 | ✅ (RPC 模式) |

### 10.3 协议复用

4 种 API 协议抽象覆盖 7+ provider，新 Provider 只需添加 CompatConfig。代码复用度高。

### 10.4 SurrealDB 统一存储

相比 Pi 的三层存储（JSONL + SQLite + V2 Sidecar），uncode 用单一的 SurrealDB 统一了：
- 会话元数据（session 表 + 索引）
- 消息条目（entry 表 + 索引）
- Leaf 指针（leaf 表）
- 分支查询（parent_entry_id 索引）

无多文件同步问题，无 SQLite WAL/锁管理，schema 强类型校验。

### 10.5 严格的代码质量

CI 中强制 `RUSTFLAGS="-D warnings"`，禁止 unsafe code，clippy + fmt 检查，344 测试全通过。

## 11. 总结

两个项目都是 Rust 原生 AI Agent 编码系统，设计哲学有根本性差异：

| 维度 | Pi Agent Rust | Uncode |
| --- | --- | --- |
| 设计哲学 | 深度定制，自研运行时，功能全面 | 架构优先，标准生态，分层解耦 |
| 代码组织 | 单体深度 | 分层广度 |
| 目标用户 | 个人开发者，终端用户 | 团队，多界面（TUI/Web/RPC） |
| 技术栈 | asupersync + charmed_rust + rich_rust (自研) | tokio + ratatui + axum + SurrealDB (生态) |
| 成熟度 | 高（972 stars, 3336+ commits, 1001 扩展） | 中（架构完成，功能迭代中） |
| 扩展性 | 扩展生态丰富，QuickJS | WASM 多语言潜力 |
| 存储 | 三层混合（JSONL + SQLite + Sidecar） | 统一 SurrealDB |

uncode 应在保持架构优势的同时，优先借鉴以下设计：
1. ~~**进程树管理**~~ ✅ 已实现 — 进程组管理 + 信号传播
2. ~~**文件操作追踪 Compaction**~~ ✅ 已实现 — turn 边界 + split-turn 检测 + 迭代摘要
3. ~~**hashline_edit 精确编辑**~~ ✅ 已实现 — 双模式 edit 工具
4. ~~**Web Fetch / Web Search**~~ ✅ 已实现 — reqwest + html2text + Tavily API
5. ~~**Semantic Workspace Graph**~~ ✅ 已实现 — 代码符号图谱 + Context Bundle 自动注入
6. **Capability-based 安全模型** — WASM 扩展实现前的设计参考
