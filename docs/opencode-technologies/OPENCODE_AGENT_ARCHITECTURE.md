# OpenCode Agent 架构

> Monorepo 分层、Effect Service 依赖、多 Agent 配置与实例模型

---

## 1. 仓库组织

OpenCode 不是「单 crate / 单包 Agent 库」，而是 **以 Agent 产品为中心的多包 monorepo**：

```
opencode/                          # Bun workspace 根
├── packages/opencode/             # ★ Agent 运行时 + CLI + Server
├── packages/core/                 # 共享基础设施
├── packages/llm/                  # 协议中立 LLM 库
├── packages/app/                  # Web UI
├── packages/desktop/              # Electron
├── packages/plugin/               # 插件 SDK
├── packages/sdk/js/               # HTTP 客户端
├── packages/console/              # 云控制台
├── specs/                         # 内部 API 设计（v2 session 等）
└── sdks/vscode/                   # VS Code 扩展
```

**与 Pi 对照**：Pi 将 `pi-ai`、`pi-agent-core`、`pi-coding-agent` 拆成可独立发布的 npm 包；OpenCode 将 **可嵌入能力** 收敛在 `@opencode-ai/sdk` + HTTP API，核心循环留在 `packages/opencode` 内部。

---

## 2. Effect Service 分层

`packages/opencode` 内子系统普遍采用 Effect 模式：

```typescript
// 典型形态
export class Service extends Context.Service<Service, Interface>()("@opencode/…") {}
export const layer: Layer.Layer<Service, never, Dependencies> = Layer.effect(Service, …)
```

### 2.1 Agent 循环相关 Service

| Service | 标识 | 职责 |
|---------|------|------|
| `Session` | `@opencode/Session` | 会话 CRUD、树关系、权限字段 |
| `SessionProcessor` | `@opencode/SessionProcessor` | 消费 `LLM.stream`，执行工具，发 v2 事件 |
| `LLM` | `@opencode/LLM` | 封装 `streamText`（AI SDK），Provider 变换 |
| `Agent` | `@opencode/Agent` | 多 Agent 配置（build/plan/…）、默认 Agent |
| `ToolRegistry` | （tool/registry） | 内置 + 自定义 + MCP 工具列表 |
| `Permission` | `@opencode/Permission` | 按 Agent/会话的规则集 |
| `Plugin` | `@opencode/Plugin` | 插件加载与钩子 |
| `Snapshot` | `@opencode/Snapshot` | Git worktree 快照 |
| `Bus` | `@opencode/Bus` | 实例级 PubSub |
| `SyncEvent` | Sync 子系统 | 多客户端一致性事件 |

`SessionProcessor.layer` 的依赖声明（节选）体现了 **编排层聚合**：

```86:101:packages/opencode/src/session/processor.ts
export const layer: Layer.Layer<
  Service,
  never,
  | Session.Service
  | Config.Service
  | Bus.Service
  | Snapshot.Service
  | Agent.Service
  | LLM.Service
  | Permission.Service
  | Plugin.Service
  | SessionSummary.Service
  | SessionStatus.Service
  | SyncEvent.Service
> = Layer.effect(
```

### 2.2 实例与项目

- **InstanceState**：单进程内「当前打开的目录 / 项目」上下文（`effect/instance-state`）。
- **Project**：`project` 表 + `specs/project.md` 描述的多项目 HTTP 路由（`X-Project` 等头）。
- **Workspace / Control-plane**：托管场景下的 workspace 字段（`session.workspace_id`）。

---

## 3. Agent 配置模型

`Agent.Info`（`agent/agent.ts`）描述可切换的 Agent **角色**，而非 Pi 的单一 `Agent` 类实例：

| 字段 | 含义 |
|------|------|
| `name` | 标识符（如 `build`、`plan`） |
| `mode` | `primary` / `subagent` / `all` |
| `permission` | `Permission.Ruleset`（plan 默认更只读） |
| `model` | 可选绑定 `providerID` + `modelID` |
| `prompt` | 可选系统提示覆盖 |
| `steps` | 可选最大步数 |

**产品行为**：

- 用户可在 TUI 中切换 Agent（触发 `session.next.agent.switched`）。
- **TaskTool** 为 subagent 创建 **子 session**（`parent_id`），在独立会话中跑子任务。

**与 Pi 对照**：Pi 文档明确避免内建 sub-agent / plan mode；OpenCode 将其作为 **一等产品功能**。

---

## 4. 请求路径总览

```
用户输入（TUI / Web / API）
    → SessionPrompt（prompt.ts）
        → 权限 / MCP / 工具列表 / 系统提示 / 附件
        → SessionProcessor.create
            → LLM.stream（session/llm.ts）
            → 工具执行（ToolRegistry + Permission）
            → MessageV2 + Part 持久化
            → SessionEvent（v2）+ Bus + SyncEvent
```

HTTP 路径：`server/routes/instance/httpapi/handlers/session.ts` 与 `handlers/v2/session.ts` 将上述能力暴露为 REST/WebSocket。

详见 [OPENCODE_LOOP_ENGINE.md](OPENCODE_LOOP_ENGINE.md)、[OPENCODE_SERVER_CLIENT.md](OPENCODE_SERVER_CLIENT.md)。

---

## 5. 扩展与集成面

| 机制 | 包/目录 | 说明 |
|------|---------|------|
| **Plugin** | `@opencode-ai/plugin` | 声明式工具、TUI 扩展 |
| **MCP** | `packages/opencode/src/mcp` | 配置化 MCP 服务器，并入 Prompt |
| **Skills** | `skill/` | 与 Pi/opencode 路径兼容的技能发现 |
| **ACP** | `src/acp/` | Agent Client Protocol（stdio） |
| **Commands** | `command/` | `/init` 等斜杠命令 |

---

## 相关文档

- [OPENCODE_OVERVIEW.md](OPENCODE_OVERVIEW.md)
- [OPENCODE_LOOP_ENGINE.md](OPENCODE_LOOP_ENGINE.md)
- [OPENCODE_TOOL_SYSTEM.md](OPENCODE_TOOL_SYSTEM.md)
- [../technologies/OPENCODE_VS_PI.md](../technologies/OPENCODE_VS_PI.md)
