# OpenCode 架构总览

> 系列文档索引 | 基于 [anomalyco/opencode](https://github.com/anomalyco/opencode) 源码分析（本地路径示例：`~/EA/opencode`）  
> 与 Pi / uncode 的横向对比见 [`../technologies/OPENCODE_VS_PI.md`](../technologies/OPENCODE_VS_PI.md)

OpenCode 是 **Bun + TypeScript** 产品型 monorepo：以 `packages/opencode` 为内核（CLI、Agent 循环、SQLite 会话、工具、HTTP 服务、TUI），叠加 Web/桌面/Console 等多端交付与 `@opencode-ai/llm` 协议层。工程上大量使用 **Effect**（Service/Layer/Stream/Schema），会话持久化为 **SQLite + Drizzle**，LLM 运行时以 **Vercel AI SDK** 为主，并正在收敛到 schema-first 的 `@opencode-ai/llm`。

---

## 分层架构（产品视角）

```
┌─────────────────────────────────────────────────────────────────┐
│  交付面（Clients）                                               │
│  TUI (OpenTUI+Solid) │ Web (@opencode-ai/app) │ Desktop (Electron) │
│  VS Code SDK │ Slack │ ACP stdio │ 移动端式 attach               │
├─────────────────────────────────────────────────────────────────┤
│  控制面（Server）                                                │
│  opencode serve → Effect HttpApiServer / OpenAPI / WebSocket     │
│  多项目 / 实例上下文 / 同步 (SyncEvent) / mDNS 发现               │
├─────────────────────────────────────────────────────────────────┤
│  Agent 运行时（packages/opencode）                               │
│  SessionPrompt → SessionProcessor → LLM.stream + ToolRegistry    │
│  Agent 配置 / Permission / Plugin / MCP / Snapshot / Compaction    │
├─────────────────────────────────────────────────────────────────┤
│  基础设施包                                                      │
│  @opencode-ai/core（路径、日志、Flag、FS）                        │
│  @opencode-ai/llm（协议中立 LLMRequest/Event，适配器藏 quirks）   │
│  @opencode-ai/plugin（扩展工具与 TUI 钩子）                       │
└─────────────────────────────────────────────────────────────────┘
```

与 **Pi** 的「Harness → Agent → agentLoop」三层库拆分不同，OpenCode 将 **循环、会话、权限、MCP、HTTP** 交织在 `packages/opencode` 单包内，通过 **长驻 server + 多客户端 attach** 实现产品化，而非仅分发可嵌入库。

---

## 系列文档索引

| 文档 | 内容 |
|------|------|
| [OPENCODE_TECHNOLOGIES_GLOSSARY.md](OPENCODE_TECHNOLOGIES_GLOSSARY.md) | **术语索引**（中英对照，读本系列前的速查表） |
| [OPENCODE_AGENT_ARCHITECTURE.md](OPENCODE_AGENT_ARCHITECTURE.md) | Monorepo 包职责、Effect Service 分层、Agent 多角色配置 |
| [OPENCODE_LOOP_ENGINE.md](OPENCODE_LOOP_ENGINE.md) | SessionPrompt / SessionProcessor、工具循环、doom-loop、压缩触发 |
| [OPENCODE_SESSION_MODEL.md](OPENCODE_SESSION_MODEL.md) | SQLite 表结构、Message/Part 模型、父子会话、revert/snapshot |
| [OPENCODE_LLM_LAYER.md](OPENCODE_LLM_LAYER.md) | 运行时 AI SDK 与 `@opencode-ai/llm` 双轨、Provider、缓存策略 |
| [OPENCODE_EVENT_SYSTEM.md](OPENCODE_EVENT_SYSTEM.md) | Instance Bus、`session.next.*` v2 事件、SyncEvent |
| [OPENCODE_TOOL_SYSTEM.md](OPENCODE_TOOL_SYSTEM.md) | ToolRegistry、内置工具、MCP、Plugin、Permission |
| [OPENCODE_SERVER_CLIENT.md](OPENCODE_SERVER_CLIENT.md) | `serve`、HTTP API、WebSocket、客户端 attach 模式 |

---

## 核心设计决策

| 决策 | 内容 | 理由 |
|------|------|------|
| **Client/Server** | TUI/Web/Desktop 连接 `opencode serve` | 多 UI 共享同一会话与工具执行环境 |
| **Effect 贯穿** | Session、Processor、Bus、Provider 均为 `Context.Service` + `Layer` | 可测试、可组合依赖；统一 Schema 校验 |
| **SQLite 会话** | `session` / `message` / `part` 关系表 + JSON 列 | 查询、索引、多客户端同步；自 JSON 存储迁移 |
| **Message + Part** | 消息元数据与流式片段分表 | 细粒度增量 UI（text/reasoning/tool 分 part） |
| **双 LLM 层** | 产品路径 `session/llm.ts`（AI SDK）；库路径 `@opencode-ai/llm` | 渐进迁移到 API-first 协议与统一事件形状 |
| **MCP 一等公民** | CLI、配置、Prompt 路径集成 | 平台化扩展，与 Pi「非 MCP 主路径」形成对照 |
| **多 Agent 配置** | `build` / `plan` / `general` + Task 子会话 | 产品内建 plan 模式与 subagent，非纯单 Agent |
| **Snapshot + Revert** | Git worktree 快照、会话级 revert 字段 | 文件变更可回滚，强于纯 transcript |
| **v2 流式协议** | `session.next.*` 细粒度事件 | Web/TUI 增量渲染、与 Pi 分阶段 tool 事件同族 |

---

## Monorepo 包一览（精选）

| 包 | 路径 | 职责 |
|----|------|------|
| `opencode` | `packages/opencode` | 主产品：CLI、循环、会话 DB、工具、server、TUI |
| `@opencode-ai/core` | `packages/core` | Global 路径、日志、Flag、Effect 工具、安装/版本 |
| `@opencode-ai/llm` | `packages/llm` | Schema-first LLM；协议目录 `src/protocols/` |
| `@opencode-ai/app` | `packages/app` | Solid Web 客户端（连本地 server） |
| `@opencode-ai/ui` | `packages/ui` | 共享 UI 组件 |
| `@opencode-ai/desktop` | `packages/desktop` | Electron 壳 |
| `@opencode-ai/sdk` | `packages/sdk/js` | OpenAPI 生成 HTTP 客户端 |
| `@opencode-ai/plugin` | `packages/plugin` | 插件 API（工具、TUI） |
| `packages/console/*` | 云 Console | SolidStart + 后端函数 |
| `packages/web` | 文档站 | Astro + Starlight（用户文档） |

完整列表见仓库根 `package.json` workspaces。

---

## 与 Pi / uncode 的关系（阅读指引）

| 维度 | OpenCode | Pi | uncode |
|------|----------|-----|--------|
| 语言 | TypeScript / Bun | TypeScript / Node | Rust |
| 会话存储 | SQLite（message/part） | JSONL 树（逻辑） | SurrealDB + JSONL 互操作 |
| 架构重心 | 产品 + Server | 可复用 Harness 库 | Pi 对齐的 Rust Harness |
| MCP | 一等公民 | 非主路径 | 非主路径 |
| 术语表 | 本系列 `OPENCODE_TECHNOLOGIES_GLOSSARY` | `PI_TECHNOLOGIES_GLOSSARY` | `UNCODE_TECHNOLOGIES_GLOSSARY` |

四表对照：[`../technologies/GLOSSARIES_COMPARISON.md`](../technologies/GLOSSARIES_COMPARISON.md)

---

## 源码锚点（快速跳转）

| 主题 | 路径（相对 `packages/opencode/src/`） |
|------|--------------------------------------|
| CLI 入口 | `index.ts` |
| 循环编排 | `session/prompt.ts` |
| 流式处理 | `session/processor.ts` |
| LLM 调用 | `session/llm.ts` |
| 会话 ORM | `session/session.sql.ts` |
| 消息模型 | `session/message-v2.ts` |
| v2 事件 | `v2/session-event.ts` |
| 工具注册 | `tool/registry.ts` |
| Provider | `provider/provider.ts` |
| HTTP 服务 | `server/server.ts` |

---

## 相关文档

| 文档 | 说明 |
|------|------|
| [OPENCODE_VS_PI.md](../technologies/OPENCODE_VS_PI.md) | OpenCode 与 Pi 架构/哲学对比 |
| [OPENCODE_TECHNOLOGIES_GLOSSARY.md](OPENCODE_TECHNOLOGIES_GLOSSARY.md) | 本系列术语索引 |
| [HARNESS_ENGINEERING_GLOSSARY.md](../technologies/HARNESS_ENGINEERING_GLOSSARY.md) | 行业 Harness 术语 |
| [UNCODE_PI_ALIGNMENT_AND_EVALUATION.md](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) | uncode 对 Pi 对齐（含 OpenCode 脚注） |

---

*文档类型：系列索引。路径：`docs/opencode-technologies/OPENCODE_OVERVIEW.md`。分析基准：OpenCode 上游 main 线本地克隆。*
