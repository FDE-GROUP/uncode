# uncode 技术文档术语索引

> 从 [`docs/uncode-technologies/`](.) 系列文档抽取的 **中英对照术语表**，便于阅读 uncode 实现层文档及与 Pi 对齐说明时统一用语。  
> Pi 侧对应术语见 [`../pi-technologies/PI_TECHNOLOGIES_GLOSSARY.md`](../pi-technologies/PI_TECHNOLOGIES_GLOSSARY.md)；机制一页纸见 [`UNCODE_PI_MECHANISM_MAP.md`](UNCODE_PI_MECHANISM_MAP.md)；对齐评价见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

| 项 | 说明 |
|----|------|
| **文档类型** | 术语索引 / Glossary |
| **路径** | `docs/uncode-technologies/UNCODE_TECHNOLOGIES_GLOSSARY.md` |
| **来源** | `UNCODE_OVERVIEW`、`UNCODE_LOOP_ENGINE`、`UNCODE_LLM_LAYER`、`UNCODE_TOOL_SYSTEM`、`UNCODE_SESSION_MODEL`、`UNCODE_EVENT_SYSTEM`、`UNCODE_TUI_ARCHITECTURE`、`TUI_EVENT_FLOW`、`UNCODE_REQUEST_LIFECYCLE` |
| **最后更新** | 2026-05 |

---

## 使用说明

- 每条格式：**中文** | **English** | **Pi 对应** | **OpenCode 对应** | 定义 | **参见**（L1 机制词条须填 Pi 列；无直接概念填 `—`）。
- 保留 crate/API 专名（如 `SessionStore`、`AgentEvent`）；**L2** 不改 API 名为 Pi 专名。
- 查法：按 **主题分类** 浏览；机制总表见 [`UNCODE_PI_MECHANISM_MAP.md`](UNCODE_PI_MECHANISM_MAP.md)；英文速查见 **附录（英文 A–Z）**。

---

## 一、文档与对象

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| uncode | uncode | — | OpenCode（独立产品） | Rust 原生 Agent Coding 系统（CLI + TUI + Platform 规划）。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| uncode 技术文档系列 | uncode technologies doc series | — | opencode-technologies | `docs/uncode-technologies/` 下与源码同步的实现层说明。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| 与 Pi 对齐 | Pi alignment | Pi alignment | — | 逻辑会话树、双层循环、事件驱动 Harness 等心智对齐 Pi；物理存储等有工程取舍。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md)、[UNCODE_PI_ALIGNMENT](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) |
| 逻辑 vs 物理（会话） | Logical vs physical (session) | JSONL 主存 vs 树逻辑 | MessageV2 / 存储后端 | **逻辑**：`SessionEntry` 树与 Pi 同构；**物理**：默认 SurrealDB，JSONL 仅互操作。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |

---

## 二、架构与 Crate 分层

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| 三层架构 | Three-layer architecture | Pi 分层（agent/ai/ui） | monorepo 包分层 | Entry（CLI/TUI）→ Agent Engine → Foundation（ai/core/shared）。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| uncode-cli | uncode-cli | pi CLI | opencode CLI | 唯一入口：clap 解析、模式路由、工具/API 注册。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| uncode-tui | uncode-tui | pi-tui | TUI 应用 | ratatui + crossterm 全屏 TUI。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |
| uncode-agent | uncode-agent | `packages/agent` | `packages/opencode` Agent | AgentLoop、Harness、Session、Tools、Compaction。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| uncode-ai | uncode-ai | `pi-ai` | Provider 层 | `Api` trait、4 协议实现、`StreamEvent`、模型注册。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| uncode-core | uncode-core | 共享类型包 | core / util | 共享类型：event、tool、session、skill；再导出 ai 类型。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| uncode-shared | uncode-shared | — | — | 叶子 crate：`UncodeError`、`AppConfig`。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| uncode-macros | uncode-macros | — | — | `#[tool]` 过程宏，编译期生成 Schema。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| uncode-extensions | uncode-extensions | Extension | Plugin | 生命周期 Hook、WASM 扩展（scaffold）。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| uncode-platform | uncode-platform | — | Web / Server | Platform 服务端（axum，规划中/演进中）。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| 依赖方向约束 | Dependency direction | 同向分层 | 同向分层 | 严格自上而下；跨层用 broadcast 或 trait，不反向依赖实现。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| API-first（LLM） | API-first (LLM) | API-first | Provider 协议 | 以协议（openai-completions 等）组织供应商，非每厂商一驱动。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |

---

## 三、循环引擎与编排

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| AgentLoop / LoopEngine | AgentLoop | `agentLoop` | SessionProcessor（工具循环） | 双层循环核心（`loop_engine.rs`）；文档亦称 LoopEngine。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| AgentHarness | AgentHarness | `AgentHarness` | — | 高层编排：steer、会话、压缩等（与 AgentLoop 协作）。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md)、[UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| 双层循环 | Dual-loop | dual `while` | tool loop in processor | 外层 `'outer` + follow-up；内层 `while` + tool-call + steering。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| run_inner | run_inner | `agentLoop` 主体 | — | Agent 主循环体：会话初始化 → 持久化 → build_context → LLM → 工具。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| MAX_TURNS | MAX_TURNS | turn 上限 | — | 单轮 run 最大 turn 数（如 50），防无限工具循环。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| active_run（原子锁） | active_run (AtomicBool) | — | — | 防止并发 `run()`；忙时返回 `HarnessError::Busy`。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| should_stop_after_turn | should_stop_after_turn | — | — | 外部回调：turn 后是否终止循环。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| prepare_next_turn | prepare_next_turn | — | — | 外部回调：turn 间切换 context/model。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| transform_context | transform_context | `transformContext` | — | 发送 LLM 前最后修改 `Vec<Message>`。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| Context Builder | context_builder | `buildContext()` | MessageV2 组装 | `build_context()`：SessionStore → LLM 消息 + 有效 model/thinking。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md)、[UNCODE_REQUEST_LIFECYCLE](UNCODE_REQUEST_LIFECYCLE.md) |
| BuiltContext | BuiltContext | `SessionContext` | — | `messages`、`effective_model`、`effective_thinking_level`。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| System Prompt Builder | SystemPromptBuilder | system prompt bundle | system 注入 | 组装 Agent 系统提示（含工作目录、技能等）。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| Workspace Graph | WorkspaceGraph | — | — | 项目文件结构图，注入 system 消息 bundle。 | [UNCODE_REQUEST_LIFECYCLE](UNCODE_REQUEST_LIFECYCLE.md) |
| ContextLoader | ContextLoader | — | rules / AGENTS | 加载 AGENTS.md / UNCODE.md 等项目上下文。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |

---

## 四、Turn、Steering 与终止

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| Turn | Turn | Turn | step / 轮次 | 一轮 LLM 调用 + 工具执行；`TurnStart` / `TurnEnd`。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| MessageQueue（三通道） | MessageQueue (three channels) | steering / followUp / nextTurn | — | steering / follow_up / next_turn 三个 `mpsc`（容量 64）。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| Steering | Steering | `steering` | — | 每 turn 结束后 drain，中途纠偏。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| Follow-up | Follow-up | `followUp` | — | 内层退出后 drain，会话延续。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| NextTurn | NextTurn | `nextTurn` | — | 首次进内层前 drain，预排队。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| pending_messages | pending_messages | pending queue | — | 待注入内层循环的消息缓冲。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| MessageQueued / MessageDelivered | MessageQueued / MessageDelivered | queue visibility | — | 用户消息入队/投递事件。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| CancellationToken | CancellationToken | abort signal | abort | tokio 取消令牌；流式与工具执行多检查点。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md)、[UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| terminate（工具，AND 语义） | terminate (tool, AND semantics) | terminate AND | — | 批次内**全部**工具 `terminate=true` 才结束内层循环。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |
| AgentInterrupted | AgentInterrupted | interrupt | — | 用户/系统取消导致的中断事件。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| AgentSettled | AgentSettled | idle / settled | — | 会话结束后的安定状态事件。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |

---

## 五、会话：SessionEntry 与存储

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| SessionEntry | SessionEntry | `SessionTreeEntry` | MessageV2 / Part | 树状会话条目枚举（serde 外部 tag）。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| SessionHeader | SessionHeader | session metadata | Session 元数据 | 会话元数据：id、version、model、working_dir、title 等。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| parent_id | parent_id | `parentId` | parent 链 | 条目父指针，构成树。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| Leaf / leaf 指针 | Leaf / leaf pointer | `leafId` / `leaf` | — | `LeafEntry` / `get_leaf_id` / `set_leaf` 树导航。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| SessionStore | SessionStore | Session 存储 API | Storage 抽象 | 异步会话存储门面，封装 `SurrealSessionStore`。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| SurrealSessionStore | SurrealSessionStore | JSONL 文件主存 | DB 后端 | 嵌入式 SurrealDB v3（`kv-rocksdb`）实现。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| append_entry | append_entry | append entry | 写入消息 | 原子追加一条 `SessionEntry`。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| load_entries | load_entries | load session | 读取会话 | 加载会话完整条目序列。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| get_path_to_root | get_path_to_root | `getBranch()` | — | 沿 `parent_id` 回溯到根。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| fork_session | fork_session | `fork()` | — | 创建子会话并建立 Branch 语义。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| SessionManager | SessionManager | — | — | 对 `SessionStore` 的高级包装 API。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| JSONL 互操作 | JSONL interoperability | JSONL 主存 | — | **非主存**：`import_jsonl_dir` 导入；TUI `/export jsonl` 导出。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| migrate_v1_to_v2 | migrate_v1_to_v2 | — | — | 为 v1 条目补 `parent_id` 链（`migration.rs`）。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| working_dir 校验 | working_dir validation | working dir | project cwd | 读 header 时缺失 CWD 打 warn（非 Pi 式引导）。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |

### SessionEntry 类型

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| Message 条目 | Message entry | `message` | MessageV2 | 用户/助手/工具消息。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| System 条目 | System entry | system events | — | Start/End/PhaseSummary/Error/Compaction 等系统事件。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| Branch 条目 | Branch entry | 隐含分支 | — | 分支：parent_session_id、reason。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| Compaction 条目 | Compaction entry | `compaction` | compaction | 压缩摘要与 first_kept_entry_id 等。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| BranchSummary 条目 | BranchSummary entry | `branch_summary` | — | 被遗弃分支的结构化摘要。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| ModelChange 条目 | ModelChange entry | `model_change` | — | 模型切换记录。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| ThinkingLevelChange 条目 | ThinkingLevelChange entry | `thinking_level_change` | — | 推理级别变更记录。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| Label 条目 | Label entry | `label` | — | 为条目打标签。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| Custom / CustomMessage | Custom / CustomMessage | `custom` / `custom_message` | — | 扩展数据或消息。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| SessionInfo 条目 | SessionInfo entry | `session_info` | — | 会话元信息。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |

---

## 六、压缩与分支摘要

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| Compaction | Compaction | Compaction | compaction / prune | 上下文过长时 LLM 摘要历史并写 `CompactionEntry`。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| should_compact_session | should_compact_session | compact 触发 | — | 估算 token > context_window × 80% 触发。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| find_cut_point | find_cut_point | cut point | — | 从末尾向前找 turn 边界截断点。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| CompactionComplete | CompactionComplete | compaction event | — | 压缩完成 Agent 事件。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md)、[UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| 迭代摘要 | Incremental summarization | incremental summary | — | 存在旧 Compaction 时用 UPDATE 提示词。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| 文件操作追踪（压缩） | File ops in compaction | file ops in summary | — | files_read / files_modified 写入摘要。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| branch_with_summary | branch_with_summary | `moveTo` + summary | — | 分支时生成并持久化 BranchSummary。 | [UNCODE_SESSION_MODEL](UNCODE_SESSION_MODEL.md) |
| Branch Summarization | Branch summarization | branch summary | — | agent 模块分支摘要流程。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| compact_if_needed | compact_if_needed | compact hook | — | 每 turn 检查并可能压缩（loop_engine）。 | [UNCODE_LOOP_ENGINE](UNCODE_LOOP_ENGINE.md) |

---

## 七、LLM 层（uncode-ai）

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| Api trait | Api trait | `pi-ai` Api | Provider.stream | `stream` / `complete` 统一 LLM 接口。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| StreamEvent | StreamEvent | 流式 delta 协议 | AI SDK stream | 流式事件：Text/Thinking/ToolCall*/Usage/Error/Done。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| 工具调用三阶段 | Tool-call three-stage protocol | toolcall delta 链 | tool call parts | ToolCallStart → ToolCallDelta → ToolCallEnd；流须以 Done 结束。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| collect_assistant_message | collect_assistant_message | 流消费组装 | — | 消费整流组装 `Message`。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| openai-completions | openai-completions | 同协议 | OpenAI 兼容 | OpenAI Chat Completions 协议实现。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| anthropic-messages | anthropic-messages | 同协议 | Anthropic | Anthropic Messages API。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| google-generative-ai | google-generative-ai | Gemini API | Google | Gemini Generative AI。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| ollama-native | ollama-native | Ollama | Ollama | Ollama 原生 `/api/chat`（JSONL 行协议，非会话 JSONL）。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| ApiRegistry | ApiRegistry | — | Provider 注册 | Eager / Lazy / Unregister 注册 API 实现。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| ModelRegistry | ModelRegistry | Model 表 | Model 配置 | 内置 + 用户模型，`merge_user_models` 覆盖。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| Model | Model | Model | Model | id、api、provider、context_window、compat 等。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| CompatConfig | CompatConfig | compat 字段 | — | 约 16 字段刻画 OpenAI 兼容差异。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| Context | Context | LLM context | 请求上下文 | system_prompt + messages + tools 等请求容器。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| StreamOptions | StreamOptions | streamOptions | 生成参数 | temperature、max_tokens、thinking_level 等。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| ThinkingLevel | ThinkingLevel | thinkingLevel | reasoning | minimal / low / medium / high 等。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| ThinkingFormat | ThinkingFormat | thinking 格式 | — | DeepSeek / OpenRouter / Anthropic 等 thinking 形态。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| StopReason | StopReason | stopReason | finish reason | 流结束原因（length、error、aborted 等）。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |
| UncodeError（LLM） | UncodeError (LLM variants) | — | — | LlmAuth、LlmRateLimit、Llm 等。 | [UNCODE_LLM_LAYER](UNCODE_LLM_LAYER.md) |

---

## 八、工具系统

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| ToolExecutor | ToolExecutor | `AgentTool` | Tool 定义 + 执行 | 工具 trait：`definition` + `execute` / `execute_with_context`。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ToolDefinition | ToolDefinition | tool schema | Zod schema | name、JSON Schema parameters、execution_mode、label。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ToolResult | ToolResult | tool result | tool output | content、is_error、details、terminate。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ToolRegistry | ToolRegistry | 工具注册表 | ToolRegistry | 运行时注册与查找 `ToolExecutor`。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| #[tool] 宏 | #[tool] macro | — | — | 从函数签名生成 `ToolDefinition` 与 schema 函数。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ExecutionMode | ExecutionMode | 并行/串行批次 | 执行策略 | Parallel（默认）或 Sequential；任一为 Sequential 则整批串行。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| 沙箱 / resolve_path | Sandbox / resolve_path | ExecutionEnv | Permission + cwd | CWD 内 `canonicalize`，越界 `SandboxViolation`。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ToolContext | ToolContext | tool 上下文 | — | cancel_token、on_progress、tool_call_id。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ToolHooks | ToolHooks | `tool_call` / `tool_result` hook | Plugin hook | before_tool_call / after_tool_call。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ExecutionEnv | ExecutionEnv | ExecutionEnv | 沙箱环境 | FileSystem + Shell trait；`LocalFileSystem` + `LocalShell`。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| hashline | hashline | — | — | 行哈希锚点精确编辑（EditTool）。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| 默认 CLI 注册工具 | Default CLI-registered tools | 内置工具集 | 内置工具 | read、write、edit、grep、bash、web_fetch；可选 web_search。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| find / ls（未默认注册） | find / ls (optional) | — | — | 已实现，demo/自定义入口可注册。 | [UNCODE_TOOL_SYSTEM](UNCODE_TOOL_SYSTEM.md) |
| ToolRenderer | ToolRenderer | TUI 工具展示 | UI 渲染 | TUI 侧 per-tool 零分配静态渲染。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |

---

## 九、事件系统

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| AgentEvent | AgentEvent | 10 种四层 + Harness | Bus 事件 | 跨层通信枚举（18 variants，含 Session/Turn/Message/Tool 等）。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| broadcast 通道 | broadcast channel | subscribe 模型 | 事件总线 | `broadcast::Sender<AgentEvent>` 发布-订阅。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| ContentDelta | ContentDelta | `message_update` | stream part | 流式 Thinking/Text 增量（DeltaType）。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| ToolCallStart/Progress/End | ToolCallStart/Progress/End | `tool_execution_*` | tool 事件 | 工具执行生命周期事件。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| EventRouter | EventRouter | Harness `on()` | Plugin 路由 | 观察型 handler + 控制型 hook handler。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| HookResult | HookResult | hook 返回值 | — | Continue、Block、PatchMessages、PatchToolResult、CancelCompaction。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| event_tag | event_tag | event.type | — | 按 serde tag 名匹配，避免序列化开销。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| LifecycleHook | LifecycleHook | Harness Hook 子集 | Plugin 生命周期 | extensions 层 8 个生命周期钩子枚举。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| Extension trait | Extension trait | Extension | Plugin | WASM/扩展实现 `on_hook`。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| HookRegistry | HookRegistry | — | Plugin 注册 | 基于 DashMap 的扩展钩子注册调度。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| SessionStart / SessionEnd | SessionStart / SessionEnd | `agent_start` / `agent_end`（近似） | session 生命周期 | 会话级生命周期事件。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |
| ErrorCategory | ErrorCategory | — | — | Llm / Tool / Network / Config。 | [UNCODE_EVENT_SYSTEM](UNCODE_EVENT_SYSTEM.md) |

---

## 十、TUI 与请求链路

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| TuiEngine | TuiEngine | pi-tui | TUI 应用 | TUI 唯一入口：主循环、渲染、快捷键。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md)、[TUI_EVENT_FLOW](TUI_EVENT_FLOW.md) |
| ChatState | ChatState | 消息列表状态 | UI state | 消息列表 + 虚拟滚动 + 行数缓存。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |
| 虚拟滚动 | Virtual scrolling | — | — | prefix_sum + 二分 `visible_range`，增量渲染 tail。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |
| InputEditor | InputEditor | 输入组件 | prompt UI | 输入框：历史、撤销、补全、UTF-8 光标。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |
| ToolRendererRegistry | ToolRendererRegistry | 工具 UI | tool UI | 9 类工具自定义渲染 + syntect 高亮。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |
| PermissionManager | PermissionManager | 确认流 | Permission | 工具执行前权限确认。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |
| SlashCommands | SlashCommands | 命令 | slash 命令 | 可扩展斜杠命令（/model、/clear 等）。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md)、[UNCODE_REQUEST_LIFECYCLE](UNCODE_REQUEST_LIFECYCLE.md) |
| expand_file_refs | expand_file_refs | @ 引用 | @ 引用 | 展开用户输入中的 `@file` 引用。 | [UNCODE_REQUEST_LIFECYCLE](UNCODE_REQUEST_LIFECYCLE.md) |
| agent_busy | agent_busy | busy 时 steer | 忙碌排队 | TUI 状态：忙碌时消息入 FollowUp/Steering 队列。 | [UNCODE_REQUEST_LIFECYCLE](UNCODE_REQUEST_LIFECYCLE.md) |
| on_submit | on_submit | prompt 提交 | 用户提交 | TUI → Agent 回调：文本 + CancellationToken + model + session_id。 | [UNCODE_REQUEST_LIFECYCLE](UNCODE_REQUEST_LIFECYCLE.md)、[TUI_EVENT_FLOW](TUI_EVENT_FLOW.md) |
| tokio::select!（biased） | tokio::select! (biased) | — | — | UI 事件优先于 Agent 事件。 | [UNCODE_TUI_ARCHITECTURE](UNCODE_TUI_ARCHITECTURE.md) |
| flush_queue | flush_queue | — | — | TurnEnd/SessionEnd 后刷新 TUI 消息队列。 | [TUI_EVENT_FLOW](TUI_EVENT_FLOW.md) |

---

## 十一、配置、技能与运行模式

| 中文 | English | Pi 对应 | OpenCode 对应 | 定义 | 参见 |
|------|---------|---------|----------------|------|------|
| AppConfig | AppConfig | Pi 配置 | opencode 配置 | `~/.uncode/config.toml` 解析的配置结构。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| UncodeError | UncodeError | — | — | 14 variants 结构化错误（shared）。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| SkillRegistry | SkillRegistry | Skills | SkillTool | 技能加载与注入（core）。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| TUI 模式 | TUI mode | TUI 启动 | TUI | 无参数启动全屏 UI。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| CLI one-shot | CLI one-shot | one-shot | CLI | `uncode "prompt"` 单次执行。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| Issue 模式 | Issue mode | — | — | `uncode --issue N` 拉取 GitHub Issue 执行。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| 流式优先 | Streaming-first | streaming-first | streaming | 所有 Provider 返回 `BoxStream<StreamEvent>`。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |
| MSRV 1.85 | MSRV 1.85 | — | — | Rust 2024 edition 最低工具链要求。 | [UNCODE_OVERVIEW](UNCODE_OVERVIEW.md) |

---

## 附录：英文 A–Z（速查）

| English | 中文 | 参见章节 |
|---------|------|----------|
| AgentEvent | 代理事件 | 九 |
| AgentHarness | 编排器 | 三 |
| AgentLoop | 循环引擎 | 三 |
| Api trait | LLM API 特质 | 七 |
| append_entry | 追加会话条目 | 五 |
| Branch summarization | 分支摘要 | 六 |
| broadcast channel | 广播通道 | 九 |
| BuiltContext | 构建后上下文 | 三 |
| CancellationToken | 取消令牌 | 四 |
| Compaction | 上下文压缩 | 六 |
| CompatConfig | 兼容配置 | 七 |
| ContentDelta | 内容增量事件 | 九 |
| ExecutionEnv | 执行环境 | 八 |
| ExecutionMode | 执行模式 | 八 |
| Follow-up | 后续消息通道 | 四 |
| HookResult | 钩子结果 | 九 |
| JSONL interoperability | JSONL 互操作 | 五 |
| MessageQueue | 三通道消息队列 | 四 |
| SessionEntry | 会话树条目 | 五 |
| SessionStore | 会话存储门面 | 五 |
| Steering | 中途纠偏 | 四 |
| StreamEvent | 流式事件 | 七 |
| SurrealSessionStore | SurrealDB 存储 | 五 |
| ToolExecutor | 工具执行器 | 八 |
| TuiEngine | TUI 引擎 | 十 |
| Turn | 轮次 | 四 |
| UncodeError | 统一错误类型 | 十一 |
| Workspace Graph | 工作区图 | 三 |

---

## 相关文档

| 文档 | 说明 |
|------|------|
| [../technologies/GLOSSARIES_COMPARISON.md](../technologies/GLOSSARIES_COMPARISON.md) | 四份术语索引对照说明 |
| [UNCODE_OVERVIEW.md](UNCODE_OVERVIEW.md) | 系列索引与 Crate 一览 |
| [UNCODE_PI_MECHANISM_MAP.md](UNCODE_PI_MECHANISM_MAP.md) | L1 机制对照一页纸 |
| [../pi-technologies/PI_TECHNOLOGIES_GLOSSARY.md](../pi-technologies/PI_TECHNOLOGIES_GLOSSARY.md) | Pi 侧术语索引 |
| [../opencode-technologies/OPENCODE_TECHNOLOGIES_GLOSSARY.md](../opencode-technologies/OPENCODE_TECHNOLOGIES_GLOSSARY.md) | OpenCode 侧术语索引 |
| [../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) | uncode 对 Pi 对齐与评价 |
| [../technologies/HARNESS_ENGINEERING_GLOSSARY.md](../technologies/HARNESS_ENGINEERING_GLOSSARY.md) | 广义 Harness 工程术语 |

---

*术语表随 `docs/uncode-technologies/` 与源码演进更新；与实现不一致时以 `crates/` 源码为准。*
