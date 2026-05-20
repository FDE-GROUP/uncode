# OpenCode 会话模型

> SQLite 持久化、Message/Part 拆分、父子会话与 Revert

---

## 1. 存储概览

| 项 | 说明 |
|----|------|
| **引擎** | SQLite（Bun sqlite） |
| **ORM** | Drizzle（`drizzle-orm/sqlite-core`） |
| **默认路径** | `~/.local/share/opencode/opencode.db`（经 `@opencode-ai/core/global`） |
| **迁移** | 自早期 JSON 存储一次性迁移（`storage/json-migration.ts`） |

**与 Pi**：Pi 逻辑为 **树状 SessionEntry + JSONL 文件**；OpenCode 为 **关系型 message/part**，树关系用 `session.parent_id` 表达。

**与 uncode**：uncode 主存 **SurrealDB**，逻辑树与 Pi 对齐；OpenCode 无「每会话一个 .jsonl」主路径。

---

## 2. 核心表结构

**文件**：`packages/opencode/src/session/session.sql.ts`

### 2.1 `session`

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | SessionID | 主键 |
| `project_id` | ProjectID | 所属项目 |
| `workspace_id` | WorkspaceID? | 托管工作区 |
| `parent_id` | SessionID? | **子会话**父指针（Task 子 Agent） |
| `slug` / `directory` / `path` | text | 工作目录标识 |
| `title` | text | 展示标题 |
| `agent` | text | 当前 Agent 名 |
| `model` | json | `{ id, providerID, variant? }` |
| `permission` | json | `Permission.Ruleset` |
| `revert` | json | 回滚指针 + snapshot/diff |
| `summary_*` | int/json | 变更统计与 diff 摘要 |
| `time_compacting` / `time_archived` | int | 压缩/归档时间戳 |

### 2.2 `message`

- 一行 = 一条逻辑消息（user / assistant 等）。
- `data` JSON 列存 `MessageV2.Info`（不含 id/sessionID）。

### 2.3 `part`

- 一行 = 消息的一个 **片段**（text delta、tool、reasoning、file、snapshot…）。
- `data` JSON 列存 `MessageV2.Part` 载荷。
- 支持 **流式增量**：UI 订阅 `session.next.text.delta` 等，对应更新 part。

### 2.4 `todo`

- 会话级待办列表（与 **TodoWriteTool** 联动）。

---

## 3. MessageV2 模型

**文件**：`packages/opencode/src/session/message-v2.ts`

### 3.1 设计要点

- **Info + Part 分离**：Info 描述消息角色、模型、错误；Part 描述可流式更新的内容块。
- **与 AI SDK 互操作**：`convertToModelMessages` 将 DB 状态转为 `ModelMessage[]` 供 `streamText`。
- **结构化输出**：`OutputFormatJsonSchema` + 强制 `StructuredOutput` 工具（见 `prompt.ts` 常量）。
- **错误类型**：`ContextOverflowError`、`ProviderAuthError`、`APIError`、`AbortedError` 等（Effect Schema）。

### 3.2 Part 类型（节选）

| type | 用途 |
|------|------|
| `text` | 助手/用户文本 |
| `reasoning` | 推理链（o1 类等） |
| `tool` | 工具调用与结果元数据 |
| `file` | 附件 |
| `snapshot` / `patch` | 文件系统快照与补丁 |

---

## 4. 会话树与子会话

```
父 session (build)
    └── 子 session (task / subagent, parent_id = 父 id)
            └── 独立 message/part 流
```

- **TaskTool** 创建子会话并在完成后把摘要返回父会话工具结果。
- 查询时可按 `parent_id` 索引（`session_parent_idx`）。

Pi 的 **BranchSummary / navigateTree** 在 OpenCode 中部分由 **summary 字段 + revert** 与产品 UI 承担，而非同一套 SessionEntry 枚举。

---

## 5. Revert 与 Snapshot

| 机制 | 说明 |
|------|------|
| **Snapshot** | `snapshot/` 模块，Git worktree；Processor 前后捕获 |
| **revert 列** | `{ messageID, partID?, snapshot?, diff? }` 指向可回滚点 |
| **SessionRevert** | `session/revert.ts` 业务逻辑 |

文件变更可与会话消息绑定，强于「仅 transcript 回滚」。

---

## 6. v2 Session API

**文件**：`packages/opencode/src/v2/session.ts`、`session-message.ts`

- 面向 HTTP/WebSocket 客户端的 **类型化消息与事件**。
- 内部 spec：`specs/v2/session.md`（如移除专用 `session.init` 路由，统一走 `/init` 命令）。

---

## 相关文档

- [OPENCODE_LOOP_ENGINE.md](OPENCODE_LOOP_ENGINE.md)
- [OPENCODE_EVENT_SYSTEM.md](OPENCODE_EVENT_SYSTEM.md)
- [../pi-technologies/PI_SESSION_MODEL.md](../pi-technologies/PI_SESSION_MODEL.md)
- [../uncode-technologies/UNCODE_SESSION_MODEL.md](../uncode-technologies/UNCODE_SESSION_MODEL.md)
