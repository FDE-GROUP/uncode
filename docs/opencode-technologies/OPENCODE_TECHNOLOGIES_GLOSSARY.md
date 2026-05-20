# OpenCode 技术术语索引

> 与 [`OPENCODE_OVERVIEW.md`](OPENCODE_OVERVIEW.md) 及本系列实现文档配套的中英对照术语表。  
> 分析基准：`~/EA/opencode`（[anomalyco/opencode](https://github.com/anomalyco/opencode)）。  
> 与 Pi / uncode / Harness 术语对照见 [`../technologies/GLOSSARIES_COMPARISON.md`](../technologies/GLOSSARIES_COMPARISON.md)。

| 项 | 说明 |
|----|------|
| **文档类型** | 术语索引 / Glossary |
| **路径** | `docs/opencode-technologies/OPENCODE_TECHNOLOGIES_GLOSSARY.md` |
| **最后更新** | 2026-05 |

---

## 一、产品与仓库

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| OpenCode | OpenCode | 开源 AI 编程 Agent 产品 | [OVERVIEW](OPENCODE_OVERVIEW.md) |
| opencode 包 | `packages/opencode` | 主运行时：CLI、循环、会话、server | [ARCHITECTURE](OPENCODE_AGENT_ARCHITECTURE.md) |
| Bun monorepo | Bun monorepo | 包管理与运行时 | OVERVIEW §Monorepo |
| Turborepo | Turborepo | 构建任务编排 | 根 `turbo.json` |
| Client/Server 架构 | Client/Server architecture | 长驻 server + 多客户端 attach | [SERVER_CLIENT](OPENCODE_SERVER_CLIENT.md) |

---

## 二、Agent 与循环

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| SessionPrompt | SessionPrompt | 编排层：组 prompt、工具、权限 | [LOOP](OPENCODE_LOOP_ENGINE.md) |
| SessionProcessor | SessionProcessor | 执行层：消费 LLM 流、跑工具 | LOOP |
| 工具循环 | Tool loop | `process` 返回 continue 直至无工具 | LOOP |
| Doom loop | Doom loop | 连续相似工具调用阈值（3） | `processor.ts` |
| 压缩结果 | Processor Result `compact` | 触发 SessionCompaction | LOOP |
| 子会话 | Child session | `parent_id` + TaskTool | [SESSION](OPENCODE_SESSION_MODEL.md) |
| build / plan Agent | build / plan agents | 产品内建多角色 | ARCHITECTURE |
| Task 工具 | TaskTool | 子 Agent 会话工具 | TOOL |

---

## 三、会话与存储

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| SessionID | SessionID | 会话主键（ULID 等） | SESSION |
| MessageV2 | MessageV2 | 消息 + Part 模型 | SESSION |
| Part | Part | 可流式更新的消息片段 | SESSION |
| SQLite 会话库 | SQLite session DB | `~/.local/share/opencode/opencode.db` | SESSION |
| Drizzle ORM | Drizzle ORM | `session.sql.ts` 表定义 | SESSION |
| session 表 | SessionTable | 会话元数据、parent_id、revert | SESSION |
| message 表 | MessageTable | 消息 Info JSON | SESSION |
| part 表 | PartTable | Part JSON | SESSION |
| JSON 迁移 | JSON migration | 旧 JSON 存储 → SQLite | `json-migration.ts` |
| Revert | Revert | 会话级回滚指针 | SESSION |
| Snapshot | Snapshot | Git worktree 文件快照 | SESSION |

---

## 四、LLM 与 Provider

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| @opencode-ai/llm | @opencode-ai/llm | Schema-first 协议库 | [LLM](OPENCODE_LLM_LAYER.md) |
| LLMRequest | LLMRequest | 协议中立请求 | `packages/llm` |
| LLMEvent | LLMEvent | 协议中立流事件 | LLM |
| AI SDK 运行时 | AI SDK runtime | `session/llm.ts` streamText | LLM |
| Provider | Provider | models.dev + @ai-sdk 适配 | LLM |
| ProviderTransform | ProviderTransform | 请求/响应变换 | `provider/transform.ts` |
| Prompt caching | Prompt caching | 默认 `cache: "auto"` | LLM §2.3 |
| 上下文溢出 | Context overflow | ContextOverflowError / isOverflow | LLM |

---

## 五、事件

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| session.next.* | session.next.* | v2 会话流式事件命名空间 | [EVENT](OPENCODE_EVENT_SYSTEM.md) |
| SessionEvent | SessionEvent | v2 事件模块 | `v2/session-event.ts` |
| text.delta | session.next.text.delta | 文本增量 | EVENT |
| tool.called | session.next.tool.called | 工具调用开始 | EVENT |
| compaction.* | session.next.compaction.* | 压缩过程事件 | EVENT |
| BusEvent | BusEvent | 实例 Bus 事件定义 | EVENT |
| Instance Bus | Instance Bus | Effect PubSub | `bus/` |
| SyncEvent | SyncEvent | 多客户端同步 | EVENT |

---

## 六、工具与扩展

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| ToolRegistry | ToolRegistry | 内置 + 自定义 + MCP 聚合 | [TOOL](OPENCODE_TOOL_SYSTEM.md) |
| Tool.Def | Tool.Def | 工具定义 | `tool/tool.ts` |
| MCP | MCP | Model Context Protocol，一等公民 | TOOL |
| Plugin | Plugin | @opencode-ai/plugin 扩展 | TOOL |
| Permission.Ruleset | Permission.Ruleset | 工具/读写的规则集 | TOOL |
| StructuredOutput 工具 | StructuredOutput tool | 强制 JSON schema 终答 | `prompt.ts` |

---

## 七、Server 与客户端

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| opencode serve | opencode serve | 启动 HTTP server | SERVER_CLIENT |
| HttpApiServer | HttpApiServer | Effect HTTP API 后端 | SERVER_CLIENT |
| WebSocket | WebSocket | 推送 session.next 流 | SERVER_CLIENT |
| @opencode-ai/app | @opencode-ai/app | Solid Web 客户端 | OVERVIEW |
| @opencode-ai/sdk | @opencode-ai/sdk | OpenAPI 生成 SDK | SERVER_CLIENT |
| ACP | Agent Client Protocol | stdio 协议集成 | `src/acp/` |
| mDNS 发现 | mDNS discovery | 局域网发现 server | `server/mdns.ts` |

---

## 八、Effect 与工程

| 中文 | English | 说明 | 参见 |
|------|---------|------|------|
| Effect Service | Effect Service | `Context.Service` + `Layer` | ARCHITECTURE |
| InstanceState | InstanceState | 当前项目/目录上下文 | ARCHITECTURE |
| @opencode-ai/core | @opencode-ai/core | 路径、日志、Flag、FS | OVERVIEW |
| Flag | Flag | 功能开关（如 Exa 搜索） | `core/flag` |

---

## 附录：英文 A–Z 速查

| English | 中文 | 章节 |
|---------|------|------|
| Agent Client Protocol (ACP) | ACP | 七 |
| AI SDK runtime | AI SDK 运行时 | 四 |
| Bun monorepo | Bun monorepo | 一 |
| build / plan agents | build/plan Agent | 二 |
| BusEvent | BusEvent | 五 |
| Child session | 子会话 | 二 |
| Client/Server architecture | Client/Server 架构 | 一 |
| Context overflow | 上下文溢出 | 四 |
| Doom loop | Doom loop | 二 |
| Drizzle ORM | Drizzle ORM | 三 |
| Instance Bus | Instance Bus | 五 |
| LLMEvent | LLMEvent | 四 |
| LLMRequest | LLMRequest | 四 |
| MCP | MCP | 六 |
| MessageV2 | MessageV2 | 三 |
| OpenCode | OpenCode | 一 |
| Part | Part | 三 |
| Permission.Ruleset | Permission 规则集 | 六 |
| Plugin | Plugin | 六 |
| Prompt caching | Prompt caching | 四 |
| Provider | Provider | 四 |
| Revert | Revert | 三 |
| session.next.* | session.next 事件 | 五 |
| SessionProcessor | SessionProcessor | 二 |
| SessionPrompt | SessionPrompt | 二 |
| Snapshot | Snapshot | 三 |
| SQLite session DB | SQLite 会话库 | 三 |
| SyncEvent | SyncEvent | 五 |
| TaskTool | Task 工具 | 二 |
| ToolRegistry | ToolRegistry | 六 |

---

## 相关文档

| 文档 | 说明 |
|------|------|
| [GLOSSARIES_COMPARISON.md](../technologies/GLOSSARIES_COMPARISON.md) | 四份术语索引对照（含本表） |
| [OPENCODE_VS_PI.md](../technologies/OPENCODE_VS_PI.md) | OpenCode 与 Pi 对比 |
| [PI_TECHNOLOGIES_GLOSSARY.md](../pi-technologies/PI_TECHNOLOGIES_GLOSSARY.md) | Pi 术语表 |
| [UNCODE_TECHNOLOGIES_GLOSSARY.md](../uncode-technologies/UNCODE_TECHNOLOGIES_GLOSSARY.md) | uncode 术语表 |

---

*路径：`docs/opencode-technologies/OPENCODE_TECHNOLOGIES_GLOSSARY.md`*
