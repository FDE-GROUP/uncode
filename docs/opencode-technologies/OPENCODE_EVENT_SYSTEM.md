# OpenCode 事件系统

> Instance Bus、v2 `session.next.*`、SyncEvent 与 UI 订阅

---

## 1. 事件分层

```
┌──────────────────────────────────────────────────────────┐
│  v2 SessionEvent（session.next.*）                        │
│  面向 Web/TUI/API 的细粒度流式协议                         │
│  定义：v2/session-event.ts                                │
├──────────────────────────────────────────────────────────┤
│  Instance Bus（BusEvent.define）                          │
│  实例内 PubSub，Effect Schema 载荷                         │
│  定义：bus/bus-event.ts                                   │
├──────────────────────────────────────────────────────────┤
│  SyncEvent                                                │
│  多客户端 / 同步表一致性                                   │
├──────────────────────────────────────────────────────────┤
│  Server WebSocket / HTTP SSE                               │
│  server/event.ts、websocket-tracker                        │
└──────────────────────────────────────────────────────────┘
```

**与 Pi**：Pi **AgentEvent**（10 种）+ **Harness Hook**（20+）；OpenCode 将 **UI 流式** 集中在 v2 命名空间，Bus 承载实例级副作用。

**与 uncode**：uncode **`AgentEvent`（18 variants）** + `broadcast`；OpenCode v2 事件更细（text/reasoning/tool 分阶段 delta）。

---

## 2. v2 SessionEvent（`session.next.*`）

**文件**：`packages/opencode/src/v2/session-event.ts`

基于 `EventV2.define`，聚合键多为 `sessionID`，带 `timestamp`。

### 2.1 事件清单

| type | 用途 |
|------|------|
| `session.next.agent.switched` | 切换 Agent |
| `session.next.model.switched` | 切换模型 |
| `session.next.prompted` | 用户 Prompt 已提交 |
| `session.next.synthetic` | 合成文本（系统注入） |
| `session.next.shell.started` / `ended` | Shell 工具生命周期 |
| `session.next.step.started` / `ended` / `failed` | AI SDK step 边界 |
| `session.next.text.started` / `delta` / `ended` | 文本流 |
| `session.next.reasoning.started` / `delta` / `ended` | 推理流 |
| `session.next.tool.input.started` / `delta` / `ended` | 工具参数流式输入 |
| `session.next.tool.called` | 工具即将执行 |
| `session.next.tool.progress` | 工具进度 |
| `session.next.tool.success` / `failed` | 工具结果 |
| `session.next.retried` | 重试 |
| `session.next.compaction.started` / `delta` / `ended` | 上下文压缩 |

### 2.2 与 Pi ToolCall 三阶段

| Pi | OpenCode v2 |
|----|-------------|
| `ToolCallStart` | `tool.input.started` + `tool.called` |
| `ToolCallDelta` | `tool.input.delta` |
| `ToolCallEnd` | `tool.success` / `tool.failed` |

文本/推理由 **独立 started/delta/ended** 序列表达，便于 Web 客户端增量渲染。

---

## 3. Instance Bus

**目录**：`packages/opencode/src/bus/`

- `BusEvent.define(type, properties)` 注册全局事件定义（Effect Schema）。
- `effectPayloads()` 生成联合 Schema 供 OpenAPI/校验。
- `Bus` Service：实例 scoped **PubSub**，支持通配订阅。

用于 **非 UI 专精** 的横切通知（配置变更、实例生命周期等），与 v2 会话流互补。

---

## 4. SyncEvent

**目录**：`packages/opencode/src/sync/`

- 配合 DB **sync 表**（见 storage schema），使多 attach 客户端看到一致会话状态。
- `SessionProcessor` 依赖 `SyncEvent.Service`，在关键写入后发布。

---

## 5. 服务端下发

- **HTTP API**：`server/routes/instance/httpapi/handlers/v2/session.ts`
- **WebSocket**：`websocket-tracker` 跟踪连接，推送 v2 事件
- **OpenAPI**：`server/server.ts` → `PublicApi`

客户端（`@opencode-ai/app`、TUI attach）订阅流而非轮询 `message` 表。

---

## 6. 订阅模型对照

| 消费者 | 推荐订阅 |
|--------|----------|
| TUI | v2 流 + 本地 OpenTUI 状态机 |
| Web App | WebSocket / SDK 生成的客户端 |
| 插件 | Bus + Plugin 钩子（见 plugin 包） |
| 自动化测试 | `LLMClient.stream` 或录制 `http-recorder` 包 |

---

## 相关文档

- [OPENCODE_LOOP_ENGINE.md](OPENCODE_LOOP_ENGINE.md)
- [OPENCODE_SERVER_CLIENT.md](OPENCODE_SERVER_CLIENT.md)
- [OPENCODE_SESSION_MODEL.md](OPENCODE_SESSION_MODEL.md)
- [../pi-technologies/PI_EVENT_SYSTEM.md](../pi-technologies/PI_EVENT_SYSTEM.md)
- [../uncode-technologies/UNCODE_EVENT_SYSTEM.md](../uncode-technologies/UNCODE_EVENT_SYSTEM.md)
