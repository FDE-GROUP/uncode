# Pi Agent 架构总览

> 系列文档索引 | 基于 Pi (`@earendil-works/pi-agent-core`) 源码分析

---

## 三层架构

Pi Agent 是**三层架构**，每层有独立的状态管理和职责边界：

```
┌──────────────────────────────────────────────────────────────┐
│  AgentHarness（高层 — 生产编排器）                             │
│  session 持久化 / compaction / skills / templates / resources │
│  hook 系统（20+ 事件） / ExecutionEnv / tree navigation       │
│  prompt() / skill() / compact() / navigateTree() / steer()   │
├──────────────────────────────────────────────────────────────┤
│  Agent 类（中层 — 有状态封装）                                  │
│  transcript 管理 / steering & follow-up 队列                   │
│  事件订阅 / ActiveRun 追踪 / 生命周期管理                       │
│  prompt() / continue() / abort() / reset() / subscribe()     │
├──────────────────────────────────────────────────────────────┤
│  agentLoop()（底层 — 无状态引擎）                               │
│  双层 while 循环 / 工具执行 / 事件发射 / 上下文转换              │
│  返回 EventStream<AgentEvent>                                  │
├──────────────────────────────────────────────────────────────┤
│  @earendil-works/pi-ai（LLM 抽象层）                           │
│  9 个内置 API / 25+ provider / 延迟加载 / 兼容层               │
│  streamSimple() / stream() / complete() / Model / Context     │
└──────────────────────────────────────────────────────────────┘
```

### 三层关系

| 层 | 状态 | 队列 | 会话 | 适用场景 |
|---|---|---|---|---|
| **AgentHarness** | 完整（含 session tree） | 三种（steer/followUp/nextTurn） | 树状持久化 | 生产应用（CLI/IDE） |
| **Agent** | 轻量（transcript） | 两种（steer/followUp） | 无 | 自定义状态管理 |
| **agentLoop** | 无（调用者维护） | 通过 config 注入 | 无 | proxy 后端、测试 |

---

## 系列文档索引

| 文档 | 内容 |
|------|------|
| [PI_LOOP_ENGINE.md](PI_LOOP_ENGINE.md) | 双层循环架构、Turn 生命周期、agentLoop API、Agent 生命周期管理 |
| [PI_LLM_LAYER.md](PI_LLM_LAYER.md) | pi-ai 抽象层、Provider 注册、高级特性、Stream Options、Proxy Stream |
| [PI_EVENT_SYSTEM.md](PI_EVENT_SYSTEM.md) | AgentEvent（10 种）、AgentHarness 事件（20+）、Hook 返回值语义、订阅模型 |
| [PI_TOOL_SYSTEM.md](PI_TOOL_SYSTEM.md) | AgentTool 定义、执行模式、执行流水线、ExecutionEnv、Shell 输出处理 |
| [PI_MESSAGE_SYSTEM.md](PI_MESSAGE_SYSTEM.md) | AgentMessage 抽象、convertToLlm 桥接、消息队列（三种/QueueMode）、Agent 状态 |
| [PI_SESSION_MODEL.md](PI_SESSION_MODEL.md) | Session 树状模型（10 种 entry）、核心操作、存储、Compaction、Branch Summary |
| [PI_EXTENSIONS.md](PI_EXTENSIONS.md) | Skills 系统、Prompt Templates、Resources 容器 |
| [PI_HARNESS_API.md](PI_HARNESS_API.md) | AgentHarness 完整 API（核心方法/运行时配置/事件注册/Phase 守卫/Pending Write） |
| [PI_ERROR_HIERARCHY.md](PI_ERROR_HIERARCHY.md) | 6 种结构化错误类 + stable error codes |

---

## 核心设计决策

| 决策 | 内容 | 理由 |
|------|------|------|
| **三层架构** | Harness → Agent → agentLoop | 分离持久化、状态管理、纯执行 |
| **双层循环** | 外层 follow-up，内层 tool-call + steering | 分离"修正方向"和"追加任务" |
| **AgentMessage 抽象** | TypeScript declaration merging 扩展 | 编译期类型安全，支持非 LLM 消息 |
| **convertToLlm 桥接** | 在 LLM 调用边界转换 | 内部保持富类型，LLM 只看到标准消息 |
| **事件驱动** | 10 种 AgentEvent + 20+ Harness 事件 | UI 精确响应 + hook 扩展 |
| **Hook 返回值语义** | 事件监听器可返回 typed result | 非侵入式行为修改（block/patch/cancel） |
| **工具抛出异常** | execute() 失败时 throw | Agent 自动包装为 isError |
| **ExecutionEnv 抽象** | FileSystem + Shell 接口 | 解耦运行环境，支持沙箱/远程 |
| **Proxy Stream** | 服务端路由 LLM 调用 | 企业部署（认证/审计/限流） |
| **Session 树** | parentId 链形成会话树 | 分支/fork/导航，非平坦日志 |
| **Pending Write** | turn 边界 flush | 并发安全，防止 mid-turn 损坏 |
| **Parallel 执行** | execute 并发，事件按完成顺序 | 减少延迟，UI 实时反馈 |
| **QueueMode one-at-a-time** | 默认每次只取一条 | 防止大量消息淹没 Agent |

---

## 模块依赖关系

```
packages/
├── agent/
│   ├── src/
│   │   ├── agent.ts                ← Agent 类（有状态封装）
│   │   ├── agent-loop.ts           ← runAgentLoop() 核心引擎
│   │   ├── types.ts                ← 全部类型定义
│   │   ├── index.ts                ← 公共导出
│   │   ├── proxy.ts                ← 服务端 LLM 路由
│   │   └── harness/                ← 生产编排层
│   │       ├── agent-harness.ts    ←   AgentHarness 完整 API
│   │       ├── types.ts            ←   20+ hook 事件 + 类型
│   │       ├── env/
│   │       │   └── nodejs.ts       ←   NodeExecutionEnv 实现
│   │       ├── compaction/
│   │       │   ├── compaction.ts   ←   上下文压缩 + split-turn
│   │       │   ├── branch-summarization.ts ← 分支摘要
│   │       │   └── utils.ts        ←   file operation tracking
│   │       ├── session/
│   │       │   ├── session.ts      ←   树状会话模型
│   │       │   ├── jsonl-repo.ts   ←   JSONL 存储后端
│   │       │   └── memory-repo.ts  ←   内存存储后端
│   │       ├── messages.ts         ←   消息转换 + 自定义类型
│   │       ├── prompt-templates.ts ←   模板加载与占位符
│   │       ├── skills.ts           ←   技能加载与注入
│   │       ├── system-prompt.ts    ←   系统提示词构建
│   │       └── utils/
│   │           ├── shell-output.ts ←   shell 输出捕获/截断
│   │           └── truncate.ts     ←   通用截断工具
│   └── test/
└── ai/
    ├── src/
    │   ├── types.ts                ← LLM 类型（Model, Context, ThinkingLevel）
    │   ├── api-registry.ts         ← provider 注册表 + 延迟加载
    │   ├── stream.ts               ← 流式调用核心
    │   ├── models.ts               ← ThinkingLevel 映射 + clamping
    │   ├── providers/
    │   │   ├── register-builtins.ts ← 9 个内置 API 注册
    │   │   ├── openai-completions/  ← OpenAI + 25+ 兼容 provider
    │   │   ├── anthropic-messages/  ← Anthropic Messages API
    │   │   ├── google/              ← Google GenAI + Vertex
    │   │   ├── mistral/             ← Mistral Conversations
    │   │   └── bedrock/             ← AWS Bedrock
    │   └── utils/
    │       └── event-stream.ts     ← EventStream 泛型
    └── test/
```

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
