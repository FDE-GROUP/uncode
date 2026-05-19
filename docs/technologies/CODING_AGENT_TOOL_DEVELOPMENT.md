# 基于 Harness Engineering 的 Coding Agent 工具开发指南

> 在 [`HARNESS_ENGINEERING.md`](HARNESS_ENGINEERING.md) 与 [`HARNESS_ENGINEERING_GLOSSARY.md`](HARNESS_ENGINEERING_GLOSSARY.md) 的概念框架下，说明如何从零设计并实现一款 **Coding Agent 工具**（CLI / IDE / 无头服务），并对照 **Pi**、**Claude Code**、**OpenCode**、**Cursor** 等主流产品的常见做法。  
> 本文**不**绑定任何特定内部项目实现，仅讨论可复用的产品与工程路径。

| 项 | 说明 |
|----|------|
| **文档类型** | 技术方案 / 产品开发指南 |
| **路径** | `docs/technologies/CODING_AGENT_TOOL_DEVELOPMENT.md` |
| **前置阅读** | `HARNESS_ENGINEERING.md`、`HARNESS_ENGINEERING_GLOSSARY.md` |
| **最后更新** | 2026-05 |

---

## 一、我们要做的是什么

### 1.1 产品定义

**Coding Agent 工具** = 在开发者本地或受控远程环境中，以**多步自主循环**完成软件工程任务（读代码、改代码、跑命令、开 PR）的 Agent 产品。

它不是：

- 纯聊天补全（无工具、无持久状态）；
- 单次「生成一整文件」的 Copilot 补全（无闭环验证）；
- 仅云端黑盒（开发者看不到工具链与上下文策略）。

它应当是：

> **Agent = Model + Harness**  
> 模型负责推理；**Harness** 负责环境、工具、上下文、权限、验证与可恢复会话。

### 1.2 设计立场（来自 Harness Engineering）

| 原则 | 含义 | 对产品的影响 |
|------|------|----------------|
| **Humans steer, agents execute** | 人定意图与规则；Agent 在 Harness 内执行 | UI 突出任务/验收/规则配置，而非替代 IDE 思考 |
| **行为 → Harness 设计** | 先定义 Agent 应如何工作，再反推组件 | 先写「用户故事 + 失败模式」，再写代码 |
| **Guides + Sensors** | 前馈降低犯错；反馈纠正结果 | 仓库规则 + 测试/Linter 必须一等公民 |
| **地图，不是百科全书** | `AGENTS.md` 作目录，SSOT 在 `docs/` | 知识系统可版本化、可机械校验 |
| **从简单开始** | 模型变强后 Harness **迁移**，非一次性堆满 | 分阶段交付，持续做 load-bearing 分析 |

### 1.3 与主流产品的关系（对标，非复制）

下表覆盖四种常见形态：**极简可嵌入 Harness（Pi）**、**厂商深度终端 Agent（Claude Code）**、**开源多面产品（OpenCode）**、**IDE 原生 Agent（Cursor）**。Harness 思想相通，**裁剪与扩展哲学**不同。

| 维度 | Pi | Claude Code | OpenCode | Cursor |
|------|----|-------------|----------|--------|
| **主入口** | 终端交互（`pi-tui`）；另支持 **print/JSON**、**RPC**、**嵌入 SDK** | 终端 CLI + Agent SDK | 终端 TUI/CLI + 可选 Web/Desktop | IDE 深度集成（Composer / Agent） |
| **模型** | **`pi-ai`**：统一多供应商（Anthropic、OpenAI、Google、Azure、DeepSeek、Bedrock…） | 以 Anthropic 为主，SDK 可编排 | 多供应商（Models.dev 等），`opencode.json` 配置 | 多模型路由，产品侧封装 |
| **工具** | 默认 **read / write / edit / bash**；用 **TypeScript Extensions**、**Skills**、**Prompt Templates**、**Pi Packages**（npm/git）扩展；核心刻意**不内置**子 Agent / Plan Mode（由扩展或包补齐） | 内置 Read/Edit/Bash/Glob 等 + MCP + Skills | 内置工具 + MCP + 可自定义 Agent | 工具 + 代码库索引 + 终端/浏览器能力 |
| **会话** | 持久化、**分支（branching）**、**compaction** | 持久会话、压缩、子 Agent | `session` 管理、多会话并行 | 多轮 Agent 对话、Background Agent |
| **知识** | **Context Files**、Skills、模板；仓库惯例 `AGENTS.md` | `CLAUDE.md` / 项目记忆、Skills | 项目配置 + 规则文件 | Rules、`.cursor/rules`、代码索引 |
| **验证** | 主要依赖 **bash** 与用户/扩展脚本；无单一强制 MCP 叙事 | 权限门、测试命令、用户确认 | LSP、命令执行、用户确认 | 终端输出、diff 审查、CI 集成 |
| **开源** | **MIT**，mono：`coding-agent` / `agent` / `ai` / `tui` / `web-ui` | SDK/协议开放程度随版本变化；核心是商业 CLI | **开源**（常见自建 fork 基础） | 闭源客户端 + 部分开放扩展点 |

**Pi 的设计取向（摘自其 `coding-agent` 文档）**：「**minimal terminal coding harness**」——默认少内置高级编排，鼓励**不 fork 内核**、用扩展与 Pi 包适配团队工作流；与「大而全一体机」是不同产品赌注。

**对照源码（可选）**：Pi 上游为 [earendil-works/pi](https://github.com/earendil-works/pi)（README 称 *Pi Agent Harness Mono Repo*）。本地克隆阅读包结构时，常见路径示例：`~/EA/pi`（`packages/coding-agent`、`packages/agent`、`packages/ai` 等）。

**结论**：自研工具不必在「CLI vs IDE」上二选一；应选定**主战场**（例如终端优先 + 轻量 LSP，或 Pi 式「薄内核 + 扩展包」），其余用插件或协议（MCP、LSP、JSON-RPC、**嵌入 SDK**）扩展。Harness 层可共享，UI 层可多样。

---

## 二、目标用户与能力边界

### 2.1 典型用户

|  persona | 诉求 | Harness 重点 |
|----------|------|----------------|
| 个人开发者 | 在仓库里快速改 bug、小功能 | 低配置启动、可靠 bash/编辑、清晰 diff |
| 团队工程师 | 遵守架构与规范 | `AGENTS.md`、Linter、SSOT、可审计会话 |
| 平台/工具团队 | 嵌入 CI、内部平台 | 无头 API、`run` 模式、可观测、策略注入 |

### 2.2 建议的首版能力边界（MVP）

**做：**

- 单仓库、单会话、单 Agent 循环（ReAct）；
- 读/写/编辑文件、执行 shell（可配置白名单）；
- 流式输出 + 工具调用可视化；
- 会话持久化（嵌入式库 + 导出 JSONL；uncode 为 SurrealDB，见 `docs/uncode-technologies/UNCODE_SESSION_MODEL.md`）；
- 项目级 `AGENTS.md` + 可选 `docs/` 渐进加载；
- 跑测试/Linter 作为 Sensor，失败则自动重试（有上限）。

**暂缓：**

- 全自动无人值守合并 PR；
- 复杂多 Agent（Planner/Evaluator）——除非目标就是长时自治；
- 完整浏览器 E2E（可二期用 Playwright MCP 接入）；
- 替代 IDE 的全部功能（补全、重构快捷键等）。

### 2.3 非目标（避免范围蔓延）

- 不追求「零人工代码」叙事；人始终在 steer 环内；
- 不默认连接生产环境执行破坏性命令；
- 不把「模型越强 Harness 越少」当作产品路线图——应做**可拆卸脚手架**。

---

## 三、系统架构：推荐分层

以下分层与 Harness 文献一致，便于团队分工与测试。

```
┌─────────────────────────────────────────────────────────────┐
│  触达层 Surfaces                                             │
│  CLI (TUI) │ IDE 插件 │ headless `run` │ HTTP/WebSocket API │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│  编排层 Orchestration                                          │
│  AgentLoop │ 阶段/权限 │ 取消/并发 │ 子 Agent（可选）           │
└───────────────────────────┬─────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
┌───────────────┐  ┌───────────────┐  ┌───────────────────┐
│ Context       │  │ Tools +       │  │ Knowledge           │
│ compaction    │  │ Sandbox       │  │ AGENTS.md / Skills  │
│ reset / RAG   │  │ MCP 宿主      │  │ 代码索引（可选）     │
└───────────────┘  └───────────────┘  └───────────────────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Model 层：Provider API（流式、工具协议、多模型路由）            │
└─────────────────────────────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│  Sensors：测试 / Linter / 类型检查 / 自定义 hook / LLM Judge   │
└─────────────────────────────────────────────────────────────┘
```

### 3.1 核心循环（与 Pi / Claude Code / OpenCode 同构）

主流产品的内核都是 **Agent Loop**，差异在工具集与策略：

```
组装 Context（系统提示 + 规则 + 历史 + 工具列表）
    → 调用 Model（流式）
    → 解析 tool_calls
    → 权限检查
    → 执行工具（读并行、写串行 是常见策略）
    → 将 tool_result 写回历史
    → 若接近窗口上限 → compaction / 卸载大输出到文件
    → 若任务完成或达上限 → 结束，否则继续
```

**实现要点：**

- 流式事件协议统一（文本 delta、tool_start/delta/end、usage、error、done）；
- 每次循环结束必须有明确 `Done` 或终止原因；
- 工具调用三阶段（start / delta / end）便于 TUI 展示与日志回放。

### 3.2 Model 层：API-first

参考 OpenCode 的多供应商策略，建议**以协议组织，而非以厂商组织**：

| 协议族 | 覆盖 |
|--------|------|
| OpenAI Chat Completions 兼容 | OpenAI、DeepSeek、GLM、Groq、多数开源托管 |
| Anthropic Messages | Claude |
| Google Generative AI | Gemini |
| 可选：本地 Ollama 原生 API | 离线/内网 |

每层实现统一 `stream(messages, tools, options) -> EventStream`，上层只认 `Model` 配置（id、api、base_url、上下文长度）。

### 3.3 Context 层

| 能力 | 说明 | 对标 |
|------|------|------|
| 会话历史 | 持久化、可分支（可选） | Pi sessions / branching、Claude Code session、OpenCode session |
| Compaction | 摘要旧消息，同 Agent 继续 | Pi compaction、Claude Code compact |
| 大工具输出卸载 | 全文落盘，上下文留摘要+路径 | LangChain 实践 |
| 渐进披露 | 启动只注入 `AGENTS.md` 摘要，按需读 `docs/` | OpenAI SSOT |
| 代码索引（可选） | 向量/符号索引注入检索片段 | Cursor codebase index |

**Context reset**（清空窗口 + 交接文件）仅在长时自治或多 Agent 场景需要；MVP 可只做 compaction。

### 3.4 Tools 与 Sandbox

**内置工具（建议最小集）：**

| 工具 | 类型 | 说明 |
|------|------|------|
| `read` | 只读 | 读文件，支持 offset/limit |
| `write` | 写 | 创建或覆盖 |
| `edit` | 写 | 基于 search/replace 或 patch，减少全文件重写 |
| `glob` / `grep` | 只读 | 搜索路径与内容 |
| `bash` | 执行 | 必须 sandbox + 策略 |
| `list_dir` | 只读 | 目录结构 |

**执行策略（推荐）：**

- 只读工具可并行；
- 写操作与 bash **串行**，避免竞态；
- 路径解析限制在 **workspace 根**（含符号链接策略）；
- bash：命令白名单 / 禁止 `rm -rf` 等（可配置 `--yolo` 仅供高级用户）。

**MCP：** 作为扩展总线（数据库、浏览器、Jira、自定义企业工具），与 Claude Code、OpenCode、Cursor 方向一致。Pi 则更强调整 **TypeScript Extensions / Pi Packages** 路线；MCP 可作为其中一种扩展面，而非唯一扩展故事。MVP 可晚于内置工具，但应预留 **MCP 宿主接口**（list_tools / call_tool / 权限）。

**Sandbox 形态：**

| 级别 | 适用 |
|------|------|
| 进程级（cwd + env 限制） | 个人本地 MVP |
| 容器 / 远程沙箱 | 团队、CI、`run` 无头模式 |
| 每任务 ephemeral 环境 | 高隔离（OpenAI 实验方向） |

### 3.5 Knowledge（Guides）

| 机制 | 作用 |
|------|------|
| `AGENTS.md`（或 `TOOL.md`） | ~100 行地图：构建、测试、目录约定、禁止项 |
| `docs/` SSOT | 架构、规范、计划；Agent 按需读取 |
| **Skills** | 可复用工作流（如「发版检查清单」） |
| **Rules 文件** | 对标 Cursor rules：路径级/全局约束 |

加载顺序建议：**全局默认 → 仓库 `AGENTS.md` → 用户本地覆盖 → 当前 Skill**。

### 3.6 Sensors（反馈）

| Sensor | 类型 | 触发时机 |
|--------|------|----------|
| `cargo test` / `npm test` / `pytest` | 计算型 | 工具调用后或 turn 结束 |
| Linter / formatter | 计算型 | 编辑后或 PR 前 |
| 类型检查 | 计算型 | 同上 |
| 自定义 hook | 计算型 | 用户脚本 exit code |
| LLM Judge（可选） | 推理型 | 高价值任务二次评审 |

错误信息应**面向 Agent 可修复**书写（含文件、规则、建议命令），而非仅给人看。

### 3.7 编排扩展（二期）

当 MVP 稳定后，再考虑 Anthropic 式 **Planner / Generator / Evaluator**：

- **Planner**：仅当用户任务模糊且范围大；
- **Evaluator**：仅当 Solo 质量不足且愿付 10×+ 成本；
- **Sprint Contract**：长时功能开发时，Generator 与 Evaluator 先对齐验收标准。

原则：**load-bearing analysis**——每加一层 Agent，用实验证明显著降失败率。

---

## 四、触达层：CLI、IDE、无头 API 如何选

### 4.1 终端 CLI（推荐首发）

**理由：** 与 Pi、Claude Code、OpenCode 一致；易调试；天然接近 shell 与 git。

| 模式 | 用途 | 参考 |
|------|------|------|
| 交互 TUI | 日常开发 | `opencode`；`pi`（`pi-tui`） |
| 单次 `run "prompt"` / print、JSON | 脚本/CI | `opencode run`；Pi **print / JSON** 模式 |
| 无头 `serve` / **RPC** | 远程/集成、进程编排 | `opencode serve`；Pi **RPC**（便于被其他进程驱动） |
| **嵌入 SDK** | 自有产品内嵌 Agent | Pi `Agent`（`@earendil-works/pi-agent-core`）+ `getModel`（`pi-ai`）；Claude Agent SDK |

**TUI 应展示：** 流式回复、工具调用卡片、diff 预览、当前模型/成本、权限确认提示。

### 4.2 IDE 插件（二期）

Cursor 的优势在于**编辑体验与索引在同一视线**。插件可提供：

- 选中代码作为上下文；
- 一键 Apply patch；
- 与本地 LSP 诊断联动（OpenCode 亦强调 LSP）。

插件不必重复实现 Agent Loop——应通过 **JSON-RPC / stdio** 连接本地 daemon（与 Claude Code 的 bridge 思路类似）。若目标是「被其他应用托管」而非独立 CLI，可参考 Pi 的 **SDK + 事件流**（`Agent`、`subscribe`）把 Harness 当作库集成。

### 4.3 与 Cursor 的差异化（若自研）

| 若不做 | 若做 |
|--------|------|
| 全仓库向量索引 | 轻量 ripgrep + 可选索引 |
| 云 Agent 农场 | 本地优先、数据不出境 |
| 闭源全家桶 | 开源核心 + 企业策略插件 |

---

## 五、安全、权限与信任

### 5.1 权限模型（参考 Claude Code）

| 级别 | 行为 |
|------|------|
| 只读 | 自动允许 read/grep/glob |
| 写入 | 需确认或仓库内 `allowed_tools` |
| bash | 默认确认；可配置 allowlist |
| 网络 | 默认限制；MCP 单独授权 |

**建议：** 默认 **interactive approve**；提供 `--auto-approve` 仅用于受信环境。

### 5.2 审计与合规

- 会话存储记录 tool 调用与结果（可脱敏）；uncode 默认 SurrealDB，可导出 JSONL 做审计；
- 企业版：策略包（禁止文件路径、强制 Sensor）；
- 密钥不进仓库、不进日志。

---

## 六、分阶段实施路线图

### Phase 0：可运行的 Agent Loop（4–8 周量级，视团队而定）

- [ ] 单模型 Provider + 流式
- [ ] read / write / edit / bash（workspace 沙箱）
- [ ] 最小 CLI：`run` + 简单 REPL
- [ ] 会话落盘
- [ ] 项目根 `AGENTS.md` 注入

**验收：** 能在真实仓库完成「修一个带测试的失败用例」闭环。

### Phase 1：工程可信度（+4–6 周）

- [ ] Compaction + 工具输出卸载
- [ ] 权限提示与配置
- [ ] 集成 1 个测试命令 + 1 个 Linter 作为 Sensor
- [ ] TUI：diff、工具时间线
- [ ] 多模型配置（至少 2 个 Provider）

**验收：** 10 个内部仓库任务，成功率显著高于「纯 chat 粘贴」。

### Phase 2：生态与知识（+4–8 周）

- [ ] MCP 宿主
- [ ] Skills 目录约定与加载
- [ ] `docs/` 渐进披露 + 可选 doc lint
- [ ] LSP 诊断注入上下文（对标 OpenCode）
- [ ] `session list/resume`、多会话

**验收：** 第三方 MCP（如 Playwright）可插拔；团队可共享 Skill 包。

### Phase 3：团队与无头（+6–10 周）

- [ ] headless API / `serve`
- [ ] 远程沙箱或 CI 适配器
- [ ] 策略包：组织级 `AGENTS.md` 覆盖
- [ ] 可选：子 Agent（grep 专家、测试专家）
- [ ] 可选：Evaluator 模式（长任务）

**验收：** CI 中 `agent run --plan fix-ci.md` 可自动提交 PR 草稿。

### Phase 4：持续治理（ ongoing ）

- [ ] Golden principles + 定期扫描 bot
- [ ] Harness 指标：每 task 工具次数、Sensor 失败率、人工介入率
- [ ] 模型升级后的脚手架回归套件

---

## 七、技术选型参考（中立）

| 领域 | 常见选择 | 备注 |
|------|----------|------|
| 语言 | Rust / TypeScript / Go | Rust：性能与安全；TS：与 MCP/CLI 生态近 |
| 异步 | tokio / Node async | 长连接流式 |
| TUI | ratatui / blessed / ink | 参考 OpenCode、Claude Code、**pi-tui**（差分渲染思路） |
| 配置 | TOML + 分层（全局/项目） | 对标 `opencode.json` |
| 会话 | JSONL / SQLite / 嵌入式 DB + 导出 JSONL | 需支持追加写与回放；uncode 默认 SurrealDB（见 `UNCODE_SESSION_MODEL.md`） |
| Diff | similar / difftastic | 展示 edit 结果 |
| 沙箱 | bubblewrap / Docker / Firecracker | 按威胁模型选 |

不必一开始选对语言；**Harness 接口稳定**比语言更重要。

---

## 八、质量与评估：如何知道 Harness 有效

### 8.1 离线基准（可选）

- 公开 benchmark（如 Terminal Bench 类）仅作参考；
- 自建 **10–30 个真实仓库任务**（修 bug、加小功能、升级依赖）更有说服力。

### 8.2 产品指标

| 指标 | 含义 |
|------|------|
| 任务成功率 | 无需人类改代码即通过测试 |
| 人工介入次数 | steer 频率 |
| 每任务成本 / 时长 | Solo vs 带 Sensor |
| Sensor 捕获率 | 有 bug 时被测试/Linter 拦下的比例 |
| 重复错误率 | 同类错误是否因 `AGENTS.md`/工具 下降（狭义 Harness） |

### 8.3 A/B：Harness 变更

LangChain 与 OpenAI 均指出：**换 Harness 可比换模型提升更大**。应对每次 Harness 改动做回归任务集。

---

## 九、组织与流程：谁做什么

| 角色 | 职责 |
|------|------|
| **产品** | 定义触达面、权限默认值、MVP 边界 |
| **Agent 平台** | Loop、Context、Provider、会话 |
| **工具与安全** | Sandbox、工具实现、MCP |
| **开发者体验** | TUI/CLI、文档、`AGENTS.md` 模板 |
| **客户仓库 Champion** | 维护 SSOT、Golden principles、Sensor 配置 |

工程文化上采纳 Hashimoto **狭义 Harness**：每次线上失败任务，复盘是「模型笨」还是「缺 Sensor/Guide」，并**一周内**沉淀进仓库或产品。

---

## 十、常见陷阱

| 陷阱 | 对策 |
|------|------|
| 巨型系统 Prompt 替代 Harness | 改 `AGENTS.md` 地图 + `docs/` + 工具 |
| 无 Sensor 就追求自治 | 先接测试/Linter 再延长 autonomous 时间 |
| 照抄 Cursor 全索引 | MVP 用 grep；索引作为可选加速 |
| 过早多 Agent | Solo + 强 Sensor 往往性价比更高 |
| 忽视模型-Harness 耦合 | 记录推荐模型；升级模型时跑回归集 |
| bash 无沙箱 | 默认确认 + 路径限制 + 审计 |

---

## 十一、与 Harness 文档的映射

| 本文模块 | Harness 文献概念 |
|----------|------------------|
| §3.1 Agent Loop | 编排循环、ReAct |
| §3.3 Context | Context Engineering、compaction、渐进披露 |
| §3.5 Knowledge | Guides、`AGENTS.md`、SSOT |
| §3.6 Sensors | Sensors、计算型/推理型 |
| §3.7 多 Agent | Full Harness、Sprint Contract |
| §五 权限 | 约束维度 |
| §六 Phase 4 | Garbage collection、golden principles |
| §八 评估 | load-bearing analysis、harness moves |

---

## 十二、总结

开发 Coding Agent 工具，本质是在交付一个 **Harness**：

1. **模型可替换**（API-first）；  
2. **循环可观测**（流式 + 会话）；  
3. **环境可约束**（沙箱 + 权限）；  
4. **知识可版本化**（地图 + SSOT）；  
5. **结果可验证**（Sensors）；  
6. **触达可扩展**（CLI → MCP → IDE → 无头）。

Pi、Claude Code、OpenCode、Cursor 已在不同触达面上验证了这些零件的组合方式（**薄内核 + 扩展包** vs **厂商全家桶** vs **开源多形态** vs **IDE 一体**）。自研产品的任务不是「再做一个聊天窗口」，而是明确：**你的用户在哪 steering、Agent 在哪 executing、哪些 Guides/Sensors 让失败可修复**。

建议从 **Phase 0 单循环 + 三四个工具 + 一个测试 Sensor** 开始，用真实仓库迭代；再按 load-bearing 证据决定是否引入多 Agent、浏览器 QA 与企业治理。

---

## 相关文档

- [Harness Engineering 深度解读](HARNESS_ENGINEERING.md)
- [Harness Engineering 术语索引](HARNESS_ENGINEERING_GLOSSARY.md)

## 外部参考（产品公开信息）

- [Pi 官网与文档](https://pi.dev)（含 [docs/latest](https://pi.dev/docs/latest)）
- [Pi 仓库（Agent Harness mono）](https://github.com/earendil-works/pi) — 本地源码树示例：`~/EA/pi`
- [Claude Code / Agent SDK 文档](https://platform.claude.com/docs/en/agent-sdk/overview)
- [OpenCode 文档](https://opencode.ai/docs/)
- [Cursor 文档](https://cursor.com/docs)（Rules、Agent、MCP 等，以官网为准）

---

*文档类型：技术方案。路径：`docs/technologies/CODING_AGENT_TOOL_DEVELOPMENT.md`。最后更新：2026-05。*
