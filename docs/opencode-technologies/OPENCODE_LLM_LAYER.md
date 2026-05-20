# OpenCode LLM 层

> 运行时 AI SDK 与 `@opencode-ai/llm` 双轨、Provider 与缓存

---

## 1. 双轨架构

OpenCode 存在 **两条 LLM 抽象线**，处于渐进统一过程中：

```
┌─────────────────────────────────────────────────────────────┐
│  产品运行时（packages/opencode/src/session/llm.ts）          │
│  Vercel AI SDK：streamText / generateObject                 │
│  @ai-sdk/openai, @ai-sdk/anthropic, …（provider/provider.ts）│
└─────────────────────────────────────────────────────────────┘
                            ↕ 概念对齐
┌─────────────────────────────────────────────────────────────┐
│  协议库（packages/llm — @opencode-ai/llm）                   │
│  LLM.request / LLMClient.stream / LLMEvent                   │
│  protocols/: openai-chat, anthropic-messages, gemini, …     │
└─────────────────────────────────────────────────────────────┘
```

| 轨 | 用途 | 特点 |
|----|------|------|
| **AI SDK 运行时** | 当前 `SessionProcessor` 主路径 | 与 `MessageV2`、工具 schema 深度集成 |
| **@opencode-ai/llm** | 新代码、测试、未来收敛 | Schema-first；quirks 仅在 adapter |

与 **uncode** 的「仅 4 协议 + `Api` trait」类似，OpenCode 产品侧 Provider 面更宽（models.dev + 大量 `@ai-sdk/*` 包）。

---

## 2. @opencode-ai/llm（协议库）

**README**：`packages/llm/README.md`

### 2.1 公共 API

| API | 说明 |
|-----|------|
| `LLM.request({...})` | 构建 `LLMRequest` |
| `LLMClient.generate` / `stream` | 同步 / 流式调用 |
| `LLM.user` / `assistant` / `toolMessage` | 消息构造 |
| `LLMClient.prepare` | 仅编译请求，不发 HTTP |
| `LLMEvent.is.*` | 流事件类型守卫 |

### 2.2 协议目录

`packages/llm/src/protocols/` 典型成员：

- `openai-chat` / `openai-responses` / `openai-compatible-chat`
- `anthropic-messages`
- `gemini`
- `bedrock-converse`

### 2.3 Prompt Caching（默认开启）

- 默认 `cache: "auto"`：在 tool 定义末、system 末、**最新 user 消息** 处设断点。
- 工具循环内多轮 API 共享前缀，读缓存成本低。
- 可 `cache: "none"` 或细粒度 `cache: { tools, system, messages, ttlSeconds }`。
- Anthropic/Bedrock 映射 `cache_control` / `cachePoint`；OpenAI/Gemini 服务端隐式缓存时 auto 可为 no-op。

---

## 3. 产品 Provider 层

**文件**：`packages/opencode/src/provider/provider.ts`、`transform.ts`、`schema.ts`

- 聚合 **models.dev** 元数据与用户配置（`opencode.json`）。
- 注册大量 **ProviderID**（OpenAI、Anthropic、Google、Bedrock、Groq、Azure、OpenRouter、OpenCode Zen…）。
- **ProviderTransform**：请求/响应变换（工具格式、推理块、图片等）。
- **Auth**：`auth/` 模块处理 API Key / OAuth。

**Agent 生成**（`agent/agent.ts`）：可用 `generateObject` / `streamObject` 动态生成子 Agent 描述（`PROMPT_GENERATE` 等）。

---

## 4. session/llm.ts（运行时）

- 输入：`LLM.StreamInput`（消息、工具、模型、生成参数）。
- 输出：`Stream` of `LLM.Event`，供 `SessionProcessor` 消费。
- 与 **Plugin**、**Permission**、**Flag** 协作。
- 全局关闭 AI SDK 警告：`globalThis.AI_SDK_LOG_WARNINGS = false`（`index.ts` / `server.ts`）。

---

## 5. 错误与溢出

| 模块 | 说明 |
|------|------|
| `provider/error.ts` | 供应商错误归一化；溢出检测参考 pi-mono |
| `session/overflow.ts` | `isOverflow` → 触发压缩 |
| `MessageV2.ContextOverflowError` | 用户可见错误 |

---

## 6. 与 Pi / uncode 对照

| 维度 | OpenCode | Pi (`pi-ai`) | uncode (`uncode-ai`) |
|------|----------|--------------|----------------------|
| 运行时 | AI SDK | 自研 stream API | `Api` trait + 4 协议 |
| 协议库 | `@opencode-ai/llm` | 合在 pi-ai | 合在 uncode-ai |
| 供应商数量 | 很多（AI SDK 生态） | 25+ 内置 | Model 声明接入 |
| Handoff | 产品/配置层 | pi-ai 支持会话中途换模型 | 配置切换 |
| Caching | llm 包默认 auto | 依 provider | 依实现 |

---

## 相关文档

- [OPENCODE_LOOP_ENGINE.md](OPENCODE_LOOP_ENGINE.md)
- [OPENCODE_AGENT_ARCHITECTURE.md](OPENCODE_AGENT_ARCHITECTURE.md)
- [../technologies/LLM_DRIVER_DESIGN.md](../technologies/LLM_DRIVER_DESIGN.md)（uncode 方案）
- [../pi-technologies/PI_LLM_LAYER.md](../pi-technologies/PI_LLM_LAYER.md)
