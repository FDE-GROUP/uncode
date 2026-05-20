# OpenCode 与 Pi：架构、功能与哲学对比

本文档仅对比两个开源仓库（本地路径示例：`~/EA/opencode`、`~/EA/pi`），作为独立技术分析，不涉及其他项目。

| 项目 | 上游 / 定位 |
|------|-------------|
| **OpenCode** | [anomalyco/opencode](https://github.com/anomalyco/opencode) — 开源 AI 编程 Agent 产品（CLI / TUI / 桌面 / Web / 商业组件） |
| **Pi** | [earendil-works/pi](https://github.com/earendil-works/pi) — **Agent Harness** 单仓：可扩展的终端 Coding Agent + 可复用的 Agent 运行时与 LLM 库 |

---

## 1. 仓库与分层架构

### 1.1 OpenCode：产品型单体 + 多面交付

OpenCode 是 **以 `packages/opencode` 为核心的产品 monorepo**，围绕「一条 Agent 产品体验」堆叠多种交付形态与基础设施：

- **`packages/opencode`**：主 CLI（`opencode`）、TUI、会话、工具注册、HTTP API、同步、权限、快照、技能、插件宿主等。
- **`@opencode-ai/core`**：跨包共享能力（文件系统、安装版本、日志、Effect 相关基础设施等）。
- **`@opencode-ai/llm`**：**Schema-first、协议中立** 的 LLM 核心；`LLM.request` / `LLMClient.stream`；适配器消化各供应商差异。
- **其他 workspace 成员**：桌面端（`packages/desktop`）、Web/Console（`packages/app`、`packages/console/*`）、企业版（`packages/enterprise`）、Slack、VS Code SDK、文档站、UI 组件库等。

**工程特征**：包管理器为 **Bun**，大量使用 **Effect**（typed errors、Layer、Stream、Schema），本地 **SQLite（Drizzle）** 做状态与迁移，并具备完整的 **HTTP API / 控制面** 思路（路由、实例上下文、中间件等）。

### 1.2 Pi：库优先的三层拆分

Pi 明确拆成 **三个可独立发布的 npm 包** + 终端与 Web UI 库：

| 包 | 职责 |
|----|------|
| **`@earendil-works/pi-ai`** | 统一多供应商 LLM API、工具调用、流式事件、OAuth、计费/Token 等 |
| **`@earendil-works/pi-agent-core`** | **有状态 Agent**：消息模型、工具执行、事件流、`transformContext` / `convertToLlm` 管线 |
| **`@earendil-works/pi-coding-agent`** | **终端 Coding Harness**：交互模式、RPC、SDK 入口、默认工具、会话、扩展/技能加载 |
| **`@earendil-works/pi-tui`** | 终端 UI 库（差分渲染） |
| **`@earendil-works/pi-web-ui`** | Web Components 聊天 UI |

**工程特征**：**Node.js（≥22）+ npm workspaces**，TypeScript + Biome；**Agent 核心与 CLI 解耦**，便于被第三方应用（如 OpenClaw）以 SDK/RPC 嵌入。

### 1.3 架构对照小结

| 维度 | OpenCode | Pi |
|------|----------|-----|
| 组织方式 | 产品 monorepo，多应用共仓 | 库 + Harness 分层，核心可单独依赖 |
| 运行时 | Bun 为主 | Node 为主 |
| 并发与错误模型 | Effect 贯穿业务与 IO | 传统 async/Promise + 清晰事件 API |
| LLM 抽象位置 | 独立包 `@opencode-ai/llm`，强调协议与事件形状统一 | 独立包 `pi-ai`，强调工具能力与供应商列表 |
| Agent 循环 | 实现在 `opencode` 包内（会话、工具、权限、插件等交织） | 集中在 `pi-agent-core`，UI 只订阅事件 |

---

## 2. LLM 与工具系统

### 2.1 LLM 层

- **OpenCode**：`@opencode-ai/llm` 提供 **单一类型语言**（request / response / event / tool），文档明确「quirks 在 adapter，不在调用方」；默认开启 **prompt caching** 策略（`cache: "auto"`），按协议映射到 Anthropic / Bedrock / OpenAI / Gemini 等。
- **Pi**：`pi-ai` 提供 **工具调用优先** 的统一 API，内置 **大量供应商**（含订阅登录、Vertex、Bedrock、OpenRouter、Copilot 等），并支持 **会话中途跨模型 handoff**、上下文序列化等偏「长期会话运营」的能力。

### 2.2 内置工具（Coding Agent）

- **OpenCode**（`tool/registry` 一类模块）：除读写编辑、grep、glob、shell 外，还内置 **任务/子代理（task）**、**待办（todo）**、**提问（question）**、**计划进出（plan）**、**WebFetch/WebSearch**、**代码语义搜索（codesearch）**、**仓库克隆/概览**、**LSP**、**apply_patch**、**技能（skill）** 等；并可通过 **MCP**、**插件** 扩展。
- **Pi**（官方 README）：默认强调 **read / write / edit / bash** 四件套；源码中还有 **grep、find、ls** 等文件系统类工具，整体 **刻意保持核心精简**，其余能力交给 **Skills / Extensions / Pi Packages**。

### 2.3 扩展机制

- **OpenCode**：**MCP 为一等公民**（CLI `mcp`、HTTP handlers、配置）；另有 **插件系统**（`@opencode-ai/plugin`）、Skills、多 Agent 配置等，偏「一体化平台能力」。
- **Pi**：官方哲学写明 **不做 MCP**（建议用带 README 的 CLI 技能或自建 Extension）；扩展主路径是 **TypeScript Extension、Skill、Prompt 模板、主题、Pi Package（npm/git）**。

---

## 3. 交互形态与产品功能

### 3.1 终端与 UI

- **OpenCode**：TUI 基于 **OpenTUI + Solid**；README 宣传 **多内置 Agent**（如 build / plan 切换、Tab 切换）、**@general** 子代理等。
- **Pi**：自研 **`pi-tui`**（差分渲染）；**显式不写 sub-agent、不写 plan mode**，鼓励用扩展或外部多进程方案替代。

### 3.2 非终端交付

- **OpenCode**：**桌面应用（Beta）**、**Web**、**Console 云控制面**、企业组件、Slack 等，属于完整 **产品矩阵**。
- **Pi**：核心交付是 **CLI + TUI**；另提供 **`pi-web-ui`** 组件库；聊天自动化在单独仓库 **pi-chat**。

### 3.3 会话与数据

- **OpenCode**：会话与项目数据与 **SQLite schema、迁移、同步（sync）**、快照（基于独立 git-dir 的 **snapshot**）等深度集成；偏「可审计、可同步的会话基础设施」。
- **Pi**：会话强调 **分支（branching）、压缩（compaction）** 等与 Agent 消息流一致的模型；数据模型更贴近 **Agent 事件 + 文件**，而非大型内置商业后端。

### 3.4 权限与安全姿态

- **OpenCode**：内置 **permission** 流程、question 工具、plan 模式等，面向「在工具调用前拦截/确认」的产品体验。
- **Pi**：README **哲学**中写明 **不做权限弹窗**，建议容器化或自行用 Extension 做确认流——把策略选择权交给用户与环境。

---

## 4. 架构哲学差异（核心）

以下是对照双方 **自述目标** 与 **实现重心** 的归纳，而非优劣评判。

### 4.1 「一体化产品」 vs 「Harness + 可组合库」

- **OpenCode** 走向 **完整开发者产品**：安装渠道多、桌面/Web、企业、控制面、Zen/路由生态等，**功能默认丰富**，通过配置和插件继续扩张。
- **Pi** 走向 **最小 Harness + 强扩展**：**「适应你的工作流，而不是反过来」**；刻意 **不内置** MCP、子 Agent、Plan 模式、权限 UI、后台 bash、内置 TODO 等，用 **扩展生态** 换 **内核简单**。

### 4.2 集成哲学：MCP / 插件 vs Extension / Package

- **OpenCode**：认同 **MCP、Agent Client Protocol（ACP）**、VS Code SDK 等 **跨工具标准**，降低与编辑器、云端、第三方工具的集成成本。
- **Pi**：对 **MCP 采取排斥立场**（文档链接到「为何可能不需要 MCP」的论述），更偏向 **npm 包、git 包、本地 TS 扩展** 的可控组合。

### 4.3 运行时与类型系统哲学

- **OpenCode**：选择 **Bun + Effect**，把副作用、重试、Schema、依赖注入 **形式化**，适合超大型单仓长期演进与严格 tracing。
- **Pi**：选择 **成熟 Node LTS 线 + 经典 TS 栈**，降低嵌入成本，**Agent 与 UI 边界清晰**（事件订阅即可接 UI）。

### 4.4 默认「智能」边界

- **OpenCode**：愿意在核心中加入 **LSP、语义搜索、多 Agent、任务委托** 等「更重的默认智能」，减少用户拼装。
- **Pi**：默认 **四工具 + 文件操作类工具**，把「更重的流程」推到 **社区包或自建扩展**，避免模型被过多内置结构干扰（例如官方提到 **内置 TODO 对模型的干扰**）。

---

## 5. 选型与阅读路径建议

| 若你更关注…… | 更适合深入…… |
|--------------|----------------|
| 单一安装即可用、桌面/Web、企业功能、MCP/插件生态 | OpenCode 仓库：`packages/opencode`、`@opencode-ai/llm` |
| 把 Agent 嵌进自有产品、RPC/SDK、最小内核与强定制 | Pi 仓库：`packages/agent`、`packages/coding-agent`、`packages/ai` |
| LLM 协议统一与 Effect 化基础设施 | OpenCode `@opencode-ai/llm` + opencode 内 `session/llm` |
| 事件驱动的 Agent 循环与工具并行语义 | Pi `pi-agent-core` README 中的事件序列与 `toolExecution` 说明 |

---

## 6. 参考阅读（两仓库内）

**OpenCode**

- 根目录 `README.md`：Agent 模式、安装与桌面版说明  
- `packages/llm/README.md`：Schema-first LLM 与缓存策略  
- `packages/opencode/AGENTS.md`：Effect 模块约定与数据库迁移说明  
- **uncode 仓库内实现层文档**（基于 `~/EA/opencode` 源码）：[`../opencode-technologies/OPENCODE_OVERVIEW.md`](../opencode-technologies/OPENCODE_OVERVIEW.md) 系列  

**Pi**

- 根目录 `README.md`：包一览与 pi.dev 文档入口  
- `packages/coding-agent/README.md`：**Philosophy** 一节（MCP/子 Agent/权限/Plan/TODO/后台 bash 等明确取舍）  
- `packages/agent/README.md`：消息流、`transformContext` / `convertToLlm`、事件与工具执行模式  
- `packages/ai/README.md`：供应商列表与工具调用流式协议说明  

---

*文档基于上述两仓库的公开 README、包说明与源码目录结构整理；版本随上游演进可能变化，请以各项目官方文档为准。*
