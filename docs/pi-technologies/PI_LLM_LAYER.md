# Pi LLM 抽象层

> pi-ai Provider 注册、内置 API、OpenAI 兼容层、高级特性、Stream Options、Proxy Stream

---

## Provider 注册表

```
ApiRegistry
├── registerApiProvider()        ← 注册 provider
├── unregisterApiProviders()     ← 动态卸载
├── clearApiProviders()          ← 清空
└── 延迟加载：provider 首次使用时才 import()
```

---

## 内置 API（9 个）

| API | 说明 |
|-----|------|
| `anthropic-messages` | Anthropic Messages API |
| `openai-completions` | OpenAI Chat Completions |
| `openai-responses` | OpenAI Responses API |
| `azure-openai-responses` | Azure OpenAI Responses |
| `openai-codex-responses` | OpenAI Codex |
| `mistral-conversations` | Mistral Conversations |
| `google-generative-ai` | Google Generative AI |
| `google-vertex` | Google Vertex AI |
| `bedrock-converse-stream` | AWS Bedrock |

---

## OpenAI 兼容层（25+ provider）

`OpenAICompletionsCompat` 提供 ~15 个自动检测标志，覆盖所有 OpenAI 兼容 provider：

amazon-bedrock, deepseek, github-copilot, xai, groq, cerebras, openrouter, vercel-ai-gateway, mistral, minimax, moonshotai, huggingface, fireworks, together, kimi-coding, cloudflare-workers-ai, zai 等。

---

## 流式调用入口

```typescript
// 统一入口（自动 reasoning 支持）
streamSimple(model, context, options): EventStream<AssistantMessageEvent>

// 原始入口（provider 特定选项）
stream(model, context, options): EventStream<AssistantMessageEvent>

// 同步等待结果
complete() / completeSimple()
```

---

## 高级 LLM 特性

| 特性 | 说明 |
|------|------|
| **Transport** | `sse | websocket | websocket-cached | auto` |
| **Cache Retention** | `none | short | long`，映射到 provider 特定参数（Anthropic `cache_control.ttl`，OpenAI `prompt_cache_retention`） |
| **ThinkingBudgets** | per-level token 预算（minimal/low/medium/high） |
| **ThinkingLevel clamping** | `clampThinkingLevel()` 自动降级到模型最近支持级别 |
| **Session ID** | 贯穿全栈用于 provider cache affinity |
| **ThinkingLevel 映射** | `Model.thinkingLevelMap` 将 Pi 级别映射到 provider 特定值（如 Anthropic 的 budget tokens） |

---

## Stream Options 管理

`AgentHarnessStreamOptions` 提供对 LLM 请求参数的细粒度控制：

| 字段 | 说明 |
|------|------|
| `transport` | `"sse" | "websocket" | "websocket-cached" | "auto"` |
| `timeout` | 请求超时 |
| `retries` / `retryDelayCap` | 重试策略 |
| `headers` | 自定义 HTTP 头 |
| `metadata` | 请求元数据 |
| `cacheRetention` | `"none" | "short" | "long"` |

每个 turn 开始时快照 options，`before_provider_request` hook 可 patch，然后传给 stream function。

---

## Proxy Stream 架构

`streamProxy()` 支持通过后端服务器路由 LLM 调用（非客户端直连）：

```
客户端                         服务端
┌──────────────┐              ┌──────────────┐
│  streamProxy │─── SSE ────→│  LLM Provider│
│  解析 delta  │              │  API Key 管理 │
│  重建消息    │←── events ──│  速率限制     │
│  带宽优化    │              │  审计日志     │
└──────────────┘              └──────────────┘
```

- 自定义 SSE 解析和 partial message 重建
- 带宽优化：delta 事件中剥离 `partial` 字段
- `ProxyAssistantMessageEvent` 类型（缩减 payload）
- 适用场景：服务端认证、速率限制、审计日志

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
