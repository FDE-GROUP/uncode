# OpenCode Server 与客户端

> `opencode serve`、HTTP API、多客户端 attach

---

## 1. 设计动机

OpenCode README 强调与 **单进程 CLI** 的差异：核心 Agent 跑在 **长驻 Server**，多种 **Client** attach：

| 客户端 | 包/入口 | 连接方式 |
|--------|---------|----------|
| TUI | `cli/cmd/tui/` | attach 本地 server |
| Web | `@opencode-ai/app` | HTTP + WebSocket |
| Desktop | `packages/desktop` | Electron 内嵌 Web |
| VS Code | `sdks/vscode` | 扩展调 API |
| ACP | `src/acp/` | stdio Agent Client Protocol |
| 脚本/SDK | `@opencode-ai/sdk` | OpenAPI 生成客户端 |

**Pi** 提供 **RPC/可嵌入 SDK**，但无同等规模的 **云 Console + 桌面矩阵**；**uncode** 规划 Platform + TUI，server 形态在 `uncode-platform` 演进中。

---

## 2. Server 启动

| 命令 | 文件 | 行为 |
|------|------|------|
| `opencode serve` | `cli/cmd/serve.ts` | 启动 HTTP 监听 |
| 核心 | `server/server.ts` | `listen({ port, hostname, mdns? })` |

### 2.1 运行时

- **Effect HttpApiServer**（`#httpapi-server`）+ **OpenAPI**（`PublicApi`）。
- 可选 **mDNS** 局域网发现（`server/mdns.ts`）。
- 全局禁用 AI SDK stdout 警告。

### 2.2 路由结构

```
server/routes/instance/httpapi/
├── handlers/session.ts      # v1 会话 API
├── handlers/v2/session.ts   # v2 流式会话 API
├── public.ts                # OpenAPI 定义
├── websocket-tracker.ts     # WS 连接
└── lifecycle.ts             # dispose 中间件
```

**多项目**：`specs/project.md` — `GET/POST /project`，会话按 `project_id` 隔离；请求头携带目录/项目上下文。

---

## 3. 客户端工作流

```
1. 启动 server（或 CLI 隐式启动）
2. Client 获取 instance URL（localhost / mDNS）
3. 创建或选择 session
4. POST prompt / 打开 WebSocket
5. 订阅 session.next.* 事件流
6. 渲染 text/tool/reasoning；处理 permission 提示
```

**attach**：TUI 可连接已运行实例，避免重复加载项目上下文。

---

## 4. SDK 与 API 版本

| 资产 | 路径 |
|------|------|
| OpenAPI | `server/server.ts` → `openapi()` |
| JS SDK | `packages/sdk/js`（v1 + v2 生成代码） |
| 内部 spec | `specs/v2/session.md` |

v2 会话 API 与 `SessionEvent` 枚举同步演进；避免保留仅包装 `/init` 的冗余路由（见 spec 说明）。

---

## 5. 同步与一致性

- **SyncEvent** + DB sync 表：多客户端同时打开同一会话时的增量同步。
- **WebSocketTracker**：管理订阅生命周期，断开时清理。

---

## 6. 部署形态（仓库内）

| 组件 | 说明 |
|------|------|
| `packages/console/*` | 托管 OpenCode 云控制台 |
| `packages/enterprise` | 企业版前端 |
| `infra/` | SST/AWS 等基础设施 |
| `packages/function` | Serverless 函数 |

本地开发：`bun dev`（opencode）、`bun dev:web`、`bun dev:desktop`。

---

## 7. 与 uncode Platform 的对照（规划）

| 能力 | OpenCode（现状） | uncode（文档规划） |
|------|------------------|-------------------|
| HTTP API | 成熟（Effect HttpApi） | `uncode-platform`（axum） |
| 事件流 | `session.next.*` | `AgentEvent` broadcast |
| 会话存储 | SQLite | SurrealDB |
| 前端 | Solid app | React Platform |

uncode 可参考 OpenCode 的 **v2 事件粒度** 与 **server-first** 产品策略，但保持 Rust 事件模型与存储取舍。

---

## 相关文档

- [OPENCODE_AGENT_ARCHITECTURE.md](OPENCODE_AGENT_ARCHITECTURE.md)
- [OPENCODE_EVENT_SYSTEM.md](OPENCODE_EVENT_SYSTEM.md)
- [OPENCODE_OVERVIEW.md](OPENCODE_OVERVIEW.md)
- [../PLATFORM_DESIGN.md](../PLATFORM_DESIGN.md)（uncode Platform 设计）
