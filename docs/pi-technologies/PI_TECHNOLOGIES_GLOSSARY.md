# Pi 技术文档术语索引

> 从 [`docs/pi-technologies/`](.) 系列文档抽取的 **中英对照术语表**，便于阅读 Pi 架构分析与 uncode 对齐文档时统一用语。  
> 术语定义以 Pi（`@earendil-works/pi-agent-core` / `pi-ai`）为准；对比类文档中的 uncode 侧术语见 [`../uncode-technologies/`](../uncode-technologies/) 与 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

| 项 | 说明 |
|----|------|
| **文档类型** | 术语索引 / Glossary |
| **路径** | `docs/pi-technologies/PI_TECHNOLOGIES_GLOSSARY.md` |
| **来源** | `PI_OVERVIEW`、`PI_AGENT_ARCHITECTURE`、`PI_LOOP_ENGINE`、`PI_LLM_LAYER`、`PI_EVENT_SYSTEM`、`PI_TOOL_SYSTEM`、`PI_MESSAGE_SYSTEM`、`PI_SESSION_MODEL`、`PI_EXTENSIONS`、`PI_HARNESS_API`、`PI_ERROR_HIERARCHY`、`PI_SOURCE_CORRIGENDUM`、`SESSION_LAYER_COMPARISON_PI`（Pi 侧）等 |
| **最后更新** | 2026-05 |

---

## 使用说明

- 每条格式：**中文** | **English** — 简要定义；**参见** 指向本目录内主文档（可点击跳转）。
- 保留 Pi 源码/API 专名（如 `AgentHarness`、`convertToLlm`），英文栏给出可读译名或业界通用说法。
- 查法：按下方 **主题分类** 浏览；需要按英文检索时使用文末 **附录（英文 A–Z）**。

---

## 一、文档与对象

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| Pi Agent | Pi Agent | earendil-works 体系的终端/IDE Agent 运行时，本系列分析对象。 | [PI_OVERVIEW](PI_OVERVIEW.md) |
| pi-agent-core | `@earendil-works/pi-agent-core` | Pi Agent 核心包（Harness、Agent、agentLoop、session 等）。 | [PI_OVERVIEW](PI_OVERVIEW.md) |
| pi-ai | `@earendil-works/pi-ai` | Pi 的 LLM 抽象层（Model、Context、Provider、流式调用）。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| Pi 技术文档系列 | Pi technologies doc series | `docs/pi-technologies/` 下对 Pi 源码的结构化中文说明。 | [PI_OVERVIEW](PI_OVERVIEW.md) |
| 源码勘误 | Source corrigendum | 对外部「Pi 机制」描述与仓库事实不一致处的修正说明。 | [PI_SOURCE_CORRIGENDUM](PI_SOURCE_CORRIGENDUM.md) |

---

## 二、架构与分层

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 三层架构 | Three-layer architecture | **AgentHarness**（编排）→ **Agent**（有状态）→ **agentLoop**（无状态）→ **pi-ai**。 | [PI_OVERVIEW](PI_OVERVIEW.md)、[PI_AGENT_ARCHITECTURE](PI_AGENT_ARCHITECTURE.md) |
| AgentHarness | AgentHarness | 高层生产编排器：会话树、压缩、技能/模板、Hook、ExecutionEnv、树导航。 | [PI_OVERVIEW](PI_OVERVIEW.md)、[PI_HARNESS_API](PI_HARNESS_API.md) |
| Agent 类 | Agent (class) | 中层有状态封装：transcript、steer/follow-up 队列、事件订阅、ActiveRun。 | [PI_AGENT_ARCHITECTURE](PI_AGENT_ARCHITECTURE.md)、[PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| agentLoop / runAgentLoop | agentLoop | 底层无状态引擎：双层 while、工具执行、事件发射、上下文转换。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| 双层循环 | Dual while-loop | 外层由 follow-up 驱动；内层由 tool-call + steering 驱动。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md)、[PI_SOURCE_CORRIGENDUM](PI_SOURCE_CORRIGENDUM.md) |
| 无状态引擎 | Stateless engine | agentLoop 不持有会话树，由调用方维护 context/transcript。 | [PI_OVERVIEW](PI_OVERVIEW.md) |
| 生产编排器 | Production orchestrator | AgentHarness 定位：CLI/IDE 等完整产品路径。 | [PI_OVERVIEW](PI_OVERVIEW.md) |
| Proxy Stream | Proxy Stream | 经后端服务路由 LLM 请求（认证、审计、限流），非客户端直连。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| EventStream | EventStream | Pi 泛型异步事件流抽象（如 LLM 流、Agent 事件流）。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |

---

## 三、Turn、循环与生命周期

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| Turn | Turn | 一轮 LLM 调用及其触发的工具执行；对应 `turn_start` / `turn_end`。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md)、[PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| Agent 运行 / Run | Agent run | 一次 `prompt()` 或 `continue()` 触发的完整 agentLoop 执行。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| ActiveRun | ActiveRun | Agent 当前运行句柄（promise、resolve、AbortController）；同一时刻仅一个。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| prepareNextTurn | prepareNextTurn | turn 结束后回调，可替换 context/model/thinkingLevel。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| shouldStopAfterTurn | shouldStopAfterTurn | turn 结束后若为 true 则优雅退出外层循环。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| hasMoreToolCalls | hasMoreToolCalls | 内层循环是否因待执行工具调用而继续。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| pendingMessages | pendingMessages | 待注入内层循环的消息缓冲（steer / follow-up / nextTurn）。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| agent_start / agent_end | agent_start / agent_end | 单次 prompt/continue 生命周期的起止事件。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| turn_start / turn_end | turn_start / turn_end | 单轮 Turn 生命周期的起止事件。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| abort | abort | 中断当前运行并清空队列（`agent.abort()` / Harness `abort()`）。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md)、[PI_HARNESS_API](PI_HARNESS_API.md) |
| waitForIdle | waitForIdle | 等待 ActiveRun 完全结束（含 agent_end 监听器）。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| terminate（工具） | terminate (tool flag) | 工具结果标记；当整批工具均 terminate 时内层循环退出。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md)、[PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| Phase 守卫 | Phase guard | Harness 在 `idle` 外拒绝 `prompt()` 等（busy）。 | [PI_HARNESS_API](PI_HARNESS_API.md) |
| AgentHarnessPhase | AgentHarnessPhase | `idle` \| `turn` \| `compaction` \| `branch_summary` \| `retry`。 | [PI_HARNESS_API](PI_HARNESS_API.md) |

---

## 四、消息、队列与 LLM 桥接

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| AgentMessage | AgentMessage | 比 LLM 原生 Message 更丰富的应用层消息联合类型。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| Message（LLM） | Message (LLM-native) | user / assistant / toolResult 等供应商协议消息。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| convertToLlm | convertToLlm | 将 AgentMessage[] 转为 LLM 可见 Message[] 的必选桥接。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md)、[PI_OVERVIEW](PI_OVERVIEW.md) |
| transformContext | transformContext | 可选：在 convertToLlm 前修剪/注入 AgentMessage[]。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| Declaration merging | Declaration merging | TypeScript 合并扩展自定义 AgentMessage 类型的机制。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| BashExecutionMessage | BashExecutionMessage | role=`bashExecution`；可 `excludeFromContext` 不进 LLM。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| CustomMessage | CustomMessage | role=`custom`；类型化扩展消息。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| BranchSummaryMessage | BranchSummaryMessage | 分支导航产生的摘要消息。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md)、[PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| CompactionSummaryMessage | CompactionSummaryMessage | 上下文压缩产生的摘要消息。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md)、[PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| Steering 队列 | Steering queue | 内层循环每轮 turn 后注入，用于中途修正方向。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| Follow-up 队列 | Follow-up queue | 内层循环退出后注入，用于追加新任务。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| NextTurn 队列 | NextTurn queue | 首次进入内层循环前 prepend 到下一轮 prompt。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| QueueMode | QueueMode | `one-at-a-time`（默认）或 `all`；steer/follow-up 可分别配置。 | [PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| transcript | transcript | Agent 层维护的消息/transcript 状态（无完整 session 树时）。 | [PI_OVERVIEW](PI_OVERVIEW.md) |
| Context（LLM） | Context | pi-ai 中发给模型的消息列表与相关选项容器。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |

---

## 五、会话、存储与树

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| Session 树 | Session tree | 由 `parentId` 链接的条目树，非平坦日志。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md)、[PI_OVERVIEW](PI_OVERVIEW.md) |
| SessionTreeEntry | SessionTreeEntry | 树节点：`id`、`parentId`、`timestamp`、`type` 等。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| Leaf / leafId | Leaf / leafId | 当前活跃叶节点指针；分支通过切换 leaf 隐含实现。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| getBranch | getBranch | 从 leaf 到 root 的路径条目列表。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| moveTo | moveTo | 切换活跃叶节点，可选生成分支摘要。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| buildContext（会话） | buildSessionContext / buildContext | 从树重建消息数组与有效 model/thinkingLevel。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| fork（会话） | fork (session) | 从分支点创建新 session。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| JsonlSessionStorage | JsonlSessionStorage | 生产用 JSONL 文件存储（CWD 编码目录）。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| InMemorySessionStorage | InMemorySessionStorage | 测试用内存存储后端。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| SessionStorage | SessionStorage | 存储抽象接口（JSONL / Memory 等实现）。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md)、[SESSION_LAYER_COMPARISON_PI](SESSION_LAYER_COMPARISON_PI.md) |
| Label 系统 | Label system | `LabelEntry` + label cache 标记与查找条目。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| Pending Session Write | Pending Session Write | turn 边界前缓冲 session 写入，防 mid-turn 损坏。 | [PI_HARNESS_API](PI_HARNESS_API.md)、[PI_OVERVIEW](PI_OVERVIEW.md) |
| UUIDv7 | UUIDv7 | Session 条目 id 采用时间可排序 UUID。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| 工作目录 / CWD | Working directory (CWD) | 会话关联目录；Pi 提供缺失 CWD 检测与引导。 | [SESSION_LAYER_COMPARISON_PI](SESSION_LAYER_COMPARISON_PI.md) |
| 会话资源清理 | Session resource cleanup | 扩展注册清理回调，会话结束时聚合执行。 | [SESSION_LAYER_COMPARISON_PI](SESSION_LAYER_COMPARISON_PI.md) |
| 版本迁移（会话） | Session format migration | Pi JSONL v1→v2→v3 等原位迁移链。 | [SESSION_LAYER_COMPARISON_PI](SESSION_LAYER_COMPARISON_PI.md) |

### 会话条目类型（Entry types）

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| message 条目 | message entry | 用户/助手/工具相关消息条目。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| thinking_level_change 条目 | thinking_level_change entry | 思考级别变更记录。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| model_change 条目 | model_change entry | 模型切换记录。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| compaction 条目 | compaction entry | 压缩摘要持久化条目。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| branch_summary 条目 | branch_summary entry | 分支摘要条目。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| custom / custom_message 条目 | custom / custom_message entry | 扩展自定义数据或消息。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| label 条目 | label entry | 标签标记条目。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| session_info 条目 | session_info entry | 会话元数据条目。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| leaf 条目 | leaf entry | 当前活跃叶指针条目。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |

---

## 六、压缩与分支摘要

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| Compaction | Compaction | 上下文过长时摘要历史并写入 session 树。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| shouldCompact | shouldCompact | 是否达到压缩阈值（contextWindow − reserveTokens）。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| findCutPoint | findCutPoint | 在 turn 边界找截断点。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| split-turn | split-turn | 截断点跨 turn 中间时的双路摘要策略。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| reserveTokens | reserveTokens | 压缩预留 token（如 16384）。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| keepRecentTokens | keepRecentTokens | 压缩后保留的近期 token（如 20000）。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| 增量摘要 | Incremental summarization | 在 previousSummary 上 UPDATE 而非全量重写。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| 文件操作追踪 | File operation tracking | 压缩摘要中附带 read/write/edit 的 XML 元数据。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| Branch Summarization | Branch summarization | 树导航切换分支时对独有条目生成摘要。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| 公共祖先 | Common ancestor | 分支摘要时新旧路径的最近公共节点。 | [PI_SESSION_MODEL](PI_SESSION_MODEL.md) |
| navigateTree | navigateTree | Harness API：分支导航并可触发摘要。 | [PI_HARNESS_API](PI_HARNESS_API.md) |
| session_before_compact | session_before_compact | Hook：可 cancel 或提供预计算压缩结果。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| session_before_tree | session_before_tree | Hook：可 cancel 或提供自定义分支摘要。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |

---

## 七、工具与执行环境

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| AgentTool | AgentTool | 工具定义：name、schema、execute、可选 parallel/sequential。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| AgentToolResult | AgentToolResult | 工具返回：content、details、可选 terminate。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| 串行执行 | Sequential execution | 工具逐个 prepare→execute→finalize。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| 并行执行 | Parallel execution | prepare 串行、execute 并发；事件按完成顺序发射。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md)、[PI_OVERVIEW](PI_OVERVIEW.md) |
| beforeToolCall / afterToolCall | beforeToolCall / afterToolCall | 工具执行前后 hook（block/patch 结果）。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| prepareArguments | prepareArguments | 工具参数兼容性垫片。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| validateToolArguments | validateToolArguments | TypeBox 参数校验。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| ExecutionEnv | ExecutionEnv | `FileSystem + Shell` 运行环境抽象。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md)、[PI_OVERVIEW](PI_OVERVIEW.md) |
| FileSystem（接口） | FileSystem (interface) | 读写文件、列目录等，返回 `Result<T>`。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| Shell（接口） | Shell (interface) | 执行命令，支持流式 stdout/stderr。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| NodeExecutionEnv | NodeExecutionEnv | Node.js 版 ExecutionEnv 参考实现。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| Shell 输出捕获 | Shell output capture | 截断、二进制清理、溢出到临时文件。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |
| tool_execution_* 事件 | tool_execution_start/update/end | 单工具执行生命周期事件。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| isError（工具结果） | isError (tool result) | 工具失败时由 Agent 包装为错误 toolResult。 | [PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |

---

## 八、事件与 Hook

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| AgentEvent | AgentEvent | Agent 层 10 种事件（agent/turn/message/tool 四层）。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| message_start/update/end | message_start / message_update / message_end | 单条消息流式生命周期。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| Harness Hook | Harness hook | `on(type, handler)` 注册、可返回 typed result 的非侵入修改。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md)、[PI_OVERVIEW](PI_OVERVIEW.md) |
| before_agent_start | before_agent_start | 可注入 messages、覆盖 systemPrompt。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| context（Hook） | context (hook) | 可替换发往 LLM 前的消息数组。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| before_provider_request | before_provider_request | 可 patch streamOptions。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| before_provider_payload | before_provider_payload | 可修改原始 HTTP payload。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| after_provider_response | after_provider_response | 观察 HTTP status/headers。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| tool_call / tool_result（Hook） | tool_call / tool_result (hook) | 阻止执行或 patch 工具结果。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| 纯观察事件 | Observability-only events | 如 queue_update、save_point、session_compact 等无返回值。 | [PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| subscribe（Harness） | subscribe | 通配订阅所有 Harness 事件。 | [PI_HARNESS_API](PI_HARNESS_API.md) |

---

## 九、LLM 层（pi-ai）

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| ApiRegistry | ApiRegistry | Provider/API 注册表，支持延迟加载。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| Provider | Provider | 具体模型供应商（OpenAI、Anthropic 等）接入单位。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| 内置 API | Built-in API | Pi 预置的 9 种协议实现（如 openai-completions、anthropic-messages）。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| OpenAI 兼容层 | OpenAI compatibility layer | `OpenAICompletionsCompat` 及多 vendor 自动检测标志。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| streamSimple / stream | streamSimple / stream | 流式补全入口（Simple 自动 reasoning 处理）。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| complete / completeSimple | complete / completeSimple | 非流式等待完整回复。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| Model | Model | 模型元数据：id、api、thinkingLevelMap、定价等。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| StreamOptions | StreamOptions | 超时、重试、transport、cacheRetention 等。 | [PI_LLM_LAYER](PI_LLM_LAYER.md)、[PI_HARNESS_API](PI_HARNESS_API.md) |
| ThinkingLevel | ThinkingLevel | minimal / low / medium / high 等等级。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| ThinkingBudgets | ThinkingBudgets | 各级别 token 预算配置。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| clampThinkingLevel | clampThinkingLevel | 将请求级别降级到模型支持的最高级别。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| Cache Retention | Cache retention | none / short / long；映射供应商缓存参数。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| Session ID（缓存亲和） | Session ID (cache affinity) | 贯穿请求用于 prompt cache 亲和。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| Transport | Transport | sse / websocket / websocket-cached / auto。 | [PI_LLM_LAYER](PI_LLM_LAYER.md) |
| StopReason | StopReason | 流结束原因：length、error、aborted 等。 | [PI_LOOP_ENGINE](PI_LOOP_ENGINE.md) |
| Usage / Token 用量 | Usage / token usage | 流中或消息上的 input/output token 统计。 | [PI_LLM_LAYER](PI_LLM_LAYER.md)、[PI_SESSION_MODEL](PI_SESSION_MODEL.md) |

---

## 十、扩展：Skills、模板与资源

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| Skills | Skills | 从 `.pi/skills` 或 `SKILL.md` 加载的能力说明包。 | [PI_EXTENSIONS](PI_EXTENSIONS.md) |
| formatSkillsForSystemPrompt | formatSkillsForSystemPrompt | 生成 `<available_skills>` 注入 system prompt。 | [PI_EXTENSIONS](PI_EXTENSIONS.md) |
| disable-model-invocation | disable-model-invocation | 技能仅应用可见、模型不可直接调用。 | [PI_EXTENSIONS](PI_EXTENSIONS.md) |
| Prompt Template | Prompt template | 带占位符的 `.md` 模板，经 `promptFromTemplate` 实例化。 | [PI_EXTENSIONS](PI_EXTENSIONS.md) |
| promptFromTemplate | promptFromTemplate | 加载模板并替换 `$1`、`$@` 等占位符。 | [PI_EXTENSIONS](PI_EXTENSIONS.md)、[PI_HARNESS_API](PI_HARNESS_API.md) |
| Resources 容器 | AgentHarnessResources | Skills + Templates 的泛型容器，每 turn 快照。 | [PI_EXTENSIONS](PI_EXTENSIONS.md) |
| resources_update | resources_update | Resources 变更时发射的观察事件。 | [PI_EXTENSIONS](PI_EXTENSIONS.md)、[PI_EVENT_SYSTEM](PI_EVENT_SYSTEM.md) |
| skill()（Harness） | skill() | Harness 调用已加载技能并可选附加 instructions。 | [PI_HARNESS_API](PI_HARNESS_API.md) |

---

## 十一、错误与 Result

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| Stable error codes | Stable error codes | 跨版本稳定的数字错误码，便于程序化处理。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md) |
| FileError | FileError | 文件操作失败。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md) |
| ExecutionError | ExecutionError | Shell/执行失败。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md) |
| CompactionError | CompactionError | 上下文压缩失败。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md) |
| BranchSummaryError | BranchSummaryError | 分支摘要失败。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md) |
| SessionError | SessionError | 会话读写/操作失败。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md) |
| AgentHarnessError | AgentHarnessError | Harness 层错误（如 busy guard）。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md) |
| Result 类型 | Result type | ExecutionEnv 等 API 用 Result 传错而非抛异常。 | [PI_ERROR_HIERARCHY](PI_ERROR_HIERARCHY.md)、[PI_TOOL_SYSTEM](PI_TOOL_SYSTEM.md) |

---

## 十二、Harness 常用 API（选）

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| prompt | prompt | 开始新 turn（可带 images）。 | [PI_HARNESS_API](PI_HARNESS_API.md) |
| continue | continue | 从当前 transcript 继续。 | [PI_HARNESS_API](PI_HARNESS_API.md) |
| compact | compact | 手动触发上下文压缩。 | [PI_HARNESS_API](PI_HARNESS_API.md) |
| steer / followUp / nextTurn | steer / followUp / nextTurn | 向三类队列注入消息。 | [PI_HARNESS_API](PI_HARNESS_API.md)、[PI_MESSAGE_SYSTEM](PI_MESSAGE_SYSTEM.md) |
| setModel / setThinkingLevel | setModel / setThinkingLevel | 运行时切换模型与思考级别。 | [PI_HARNESS_API](PI_HARNESS_API.md) |
| setActiveTools / setTools | setActiveTools / setTools | 启用或替换工具集。 | [PI_HARNESS_API](PI_HARNESS_API.md) |
| appendMessage | appendMessage | 运行中追加消息（经 pending writes）。 | [PI_HARNESS_API](PI_HARNESS_API.md) |

---

## 附录：英文 A–Z（速查）

| English | 中文 | 参见章节 |
|---------|------|----------|
| abort | 中止 | 三 |
| ActiveRun | 活跃运行句柄 | 三 |
| Agent | Agent 类 | 二 |
| AgentEvent | Agent 事件 | 八 |
| AgentHarness | 生产编排器 | 二 |
| AgentMessage | 应用层消息 | 四 |
| AgentTool | 代理工具 | 七 |
| agentLoop | 无状态循环引擎 | 二、三 |
| ApiRegistry | API 注册表 | 九 |
| Branch summarization | 分支摘要 | 六 |
| Compaction | 上下文压缩 | 六 |
| convertToLlm | 转为 LLM 消息 | 四 |
| ExecutionEnv | 执行环境 | 七 |
| EventStream | 事件流 | 二 |
| Follow-up queue | Follow-up 队列 | 四 |
| Hook | 钩子 | 八 |
| Leaf | 活跃叶节点 | 五 |
| NextTurn queue | NextTurn 队列 | 四 |
| Pending Session Write | 待刷写会话 | 五 |
| Proxy Stream | 代理流式路由 | 二、九 |
| QueueMode | 队列模式 | 四 |
| Result | 结果类型（错误传递） | 十一 |
| Session tree | 会话树 | 五 |
| Steering queue | Steering 队列 | 四 |
| ThinkingLevel | 思考级别 | 九 |
| Turn | 轮次 | 三 |
| UUIDv7 | 时间可排序 UUID | 五 |

---

## 相关文档

| 文档 | 说明 |
|------|------|
| [../technologies/GLOSSARIES_COMPARISON.md](../technologies/GLOSSARIES_COMPARISON.md) | 四份术语索引对照说明 |
| [PI_OVERVIEW.md](PI_OVERVIEW.md) | 系列索引与核心设计决策 |
| [SESSION_LAYER_COMPARISON_PI.md](SESSION_LAYER_COMPARISON_PI.md) | Pi 与 uncode 会话层对比（含勘误） |
| [../technologies/HARNESS_ENGINEERING_GLOSSARY.md](../technologies/HARNESS_ENGINEERING_GLOSSARY.md) | 广义 Harness Engineering 术语（跨项目） |
| [../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) | uncode 对 Pi 的对齐与评价 |

---

*术语表随 `docs/pi-technologies/` 文档演进更新；若与 Pi 上游源码不一致，以源码为准。*
