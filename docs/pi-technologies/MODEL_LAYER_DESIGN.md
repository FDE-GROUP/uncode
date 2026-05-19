# 模型层技术方案

> 本文档基于 `uncode-core` + `uncode-ai` 当前代码事实，阐述模型层的架构设计、核心类型、协议实现与扩展方法。

## 1. 架构定位

```
┌───────────────────────────────────────────────────────┐
│  uncode-agent (Loop Engine)                           │
│  ┌─────────────┐  ┌───────────┐  ┌────────────────┐  │
│  │ system_prompt│  │ctx compress│  │ token estimate │  │
│  └──────┬──────┘  └─────┬─────┘  └───────┬────────┘  │
│         └────────────────┼────────────────┘           │
│                          ▼                            │
│              Context + StreamOptions                  │
└──────────────────────┬────────────────────────────────┘
                       ▼
┌───────────────────────────────────────────────────────┐
│  uncode-ai (LLM 供应商抽象层)                         │
│                                                       │
│  ┌─────────────┐   ┌──────────────┐                   │
│  │ ApiRegistry │   │ ModelRegistry│                   │
│  └──────┬──────┘   └──────┬───────┘                   │
│         │                 │                           │
│         ▼                 ▼                           │
│  ┌─────────────────────────────────────────────────┐  │
│  │              Api trait (4 实现)                  │  │
│  │  OpenAiCompletions │ AnthropicMessages          │  │
│  │  GeminiGenerative  │ OllamaNative               │  │
│  └─────────────────────────────────────────────────┘  │
└──────────────────────┬────────────────────────────────┘
                       ▼
┌───────────────────────────────────────────────────────┐
│  uncode-core + uncode-shared                          │
│  会话 / 工具 / 事件等共享面；多处对 `uncode-ai` 再导出   │
│  （如 `model`、`api_types`），以单一来源避免类型分叉   │
└───────────────────────────────────────────────────────┘
```

**依赖方向**：`uncode-agent` **同时**依赖 `uncode-core` 与 `uncode-ai`；`uncode-core` 依赖 `uncode-ai` 与 `uncode-shared`；`uncode-ai` 依赖 `uncode-shared`。`StreamEvent`、`Api`、`ModelRegistry`、内置 `Model` 数据均在 **`uncode-ai`** 定义。

**设计原则**：API-first。一个 API 协议对应一个 `Api` 实现，通过 `Model.api` 字段路由。新增供应商只需声明 `Model` 数据 + 必要时提供 `CompatConfig`，无需编写驱动代码。

## 2. 核心类型

### 2.1 Model

`crates/uncode-core/src/model.rs`

纯数据结构，存储模型元数据，不含密钥。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `String` | 唯一标识符，如 `"deepseek-chat"` |
| `name` | `String` | 显示名称 |
| `api` | `String` | API 协议标识，路由到 `ApiRegistry` |
| `provider` | `String` | 供应商标识 |
| `base_url` | `String` | API 端点 |
| `context_window` | `u32` | 最大上下文长度（默认 128,000） |
| `max_output_tokens` | `u32` | 最大输出 token（默认 8,192） |
| `reasoning` | `bool` | 是否支持推理/思考模式 |
| `thinking_format` | `Option<ThinkingFormat>` | 思考内容格式偏好 |
| `input_modalities` | `Vec<InputModality>` | 支持的输入模态（Text/Image/Audio） |
| `pricing` | `ModelPricingPerMillion` | 每百万 token 定价 |
| `headers` | `HashMap<String, String>` | 请求级自定义 Header |
| `compat` | `CompatConfig` | 供应商兼容性参数 |
| `thinking_level_map` | `HashMap<ThinkingLevel, Option<String>>` | ThinkingLevel → 供应商特定值映射 |

### 2.2 Context

`crates/uncode-core/src/api_types.rs`

对话状态容器，独立于请求参数。

```rust
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}
```

### 2.3 StreamOptions

`crates/uncode-core/src/api_types.rs`

每次调用可独立设置的请求参数。

| 字段 | 类型 | 说明 |
|------|------|------|
| `api_key` | `Option<String>` | API 密钥 |
| `temperature` | `Option<f32>` | 采样温度 |
| `max_tokens` | `Option<u32>` | 最大输出 token |
| `signal` | `Option<CancellationToken>` | 取消令牌 |
| `timeout_ms` | `Option<u64>` | 请求超时 |
| `max_retries` | `Option<u32>` | 最大重试次数 |
| `max_retry_delay_ms` | `Option<u64>` | 最大重试延迟 |
| `headers` | `Option<HashMap<String, String>>` | 自定义 Header |
| `thinking_level` | `Option<ThinkingLevel>` | 思考级别 |
| `thinking_budget_tokens` | `Option<u32>` | 思考 token 预算 |
| `session_id` | `Option<String>` | 会话 ID（用于缓存亲和） |
| `cache_retention` | `Option<CacheRetention>` | 缓存保留策略 |
| `on_payload` | `Option<Arc<dyn Fn(&Value)>>` | 请求载荷回调 |
| `on_response` | `Option<Arc<dyn Fn(u16, &Headers)>>` | 响应回调 |

### 2.4 StopReason

```rust
pub enum StopReason {
    Stop,    // 正常结束
    Length,  // 达到最大 token 限制
    ToolUse, // 模型请求调用工具
    Error,   // 内容过滤或安全原因
    Aborted, // 被外部取消
}
```

各供应商原始值 → `StopReason` 映射：

| 供应商 | 原始值 | → StopReason |
|--------|--------|-------------|
| OpenAI | `stop`, `end` | Stop |
| OpenAI | `length`, `max_tokens` | Length |
| OpenAI | `tool_calls`, `function_call` | ToolUse |
| OpenAI | `content_filter` | Error |
| Anthropic | `end_turn`, `stop_sequence` | Stop |
| Anthropic | `tool_use` | ToolUse |
| Anthropic | `max_tokens` | Length |
| Gemini | `STOP`, `FINISH_REASON_STOP` | Stop |
| Gemini | `MAX_TOKENS`, `FINISH_REASON_MAX_TOKENS` | Length |
| Gemini | `SAFETY`, `RECITATION`, `FINISH_REASON_SAFETY` | Error |
| Ollama | `length` | Length |
| Ollama | 其他（`load`/`unload`） | Stop |

### 2.5 ThinkingLevel

```rust
pub enum ThinkingLevel {
    Off,      // 关闭思考
    Minimal,  // 最低
    Low,      // 低
    Medium,   // 中
    High,     // 高
    XHigh,    // 最高
}
```

通过 `Model.thinking_level_map` 映射为供应商特定值。例如 DeepSeek 的映射：

| ThinkingLevel | DeepSeek 值 |
|---------------|------------|
| Minimal | `None`（不支持） |
| Low | `None`（不支持） |
| Medium | `None`（不支持） |
| High | `"high"` |
| XHigh | `"max"` |

### 2.6 ThinkingFormat

```rust
pub enum ThinkingFormat {
    OpenAi,
    DeepSeek,
    Anthropic,
    Gemini,
    OpenRouter,
    Together,
    ZAi,
    Qwen,
    QwenChatTemplate,
}
```

### 2.7 CacheRetention

```rust
pub enum CacheRetention {
    None,   // 不缓存
    Short,  // 短期缓存（默认）
    Long,   // 长期缓存（24h）
}
```

### 2.8 CompatConfig

16 字段的扁平结构，控制供应商特定行为，避免条件类型。

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `supports_developer_role` | `true` | 系统消息使用 `developer` 角色而非 `system` |
| `supports_reasoning_effort` | `false` | 支持 `reasoning_effort` 参数 |
| `supports_usage_in_streaming` | `true` | SSE 流中包含 usage 信息 |
| `supports_strict_mode` | `false` | 支持严格模式 |
| `max_tokens_field` | `MaxTokens` | 使用 `max_tokens` 还是 `max_completion_tokens` |
| `requires_tool_result_name` | `false` | 工具结果需携带工具名称 |
| `requires_assistant_after_tool_result` | `false` | 工具结果后需要 Assistant 消息 |
| `requires_thinking_as_text` | `false` | 思考内容作为普通文本 |
| `done_breaks_stream` | `false` | `data: [DONE]` 后终止流 |
| `thinking_format` | `None` | 思考内容格式 |
| `send_session_affinity_headers` | `false` | 发送 `session_id` Header |
| `supports_long_cache_retention` | `false` | 支持长期缓存保留 |
| `supports_store` | `false` | 支持 `store` 参数 |
| `requires_reasoning_content_on_assistant_messages` | `false` | Assistant 消息需包含 reasoning_content |
| `supports_eager_tool_input_streaming` | `false` | 支持即时工具输入流 |
| `supports_cache_control_on_tools` | `false` | 工具定义支持 cache_control |

各供应商 CompatConfig 差异：

| 字段 | OpenAI | DeepSeek | Anthropic | Gemini | GLM | Groq/Cerebras | xAI |
|------|--------|----------|-----------|--------|-----|--------------|-----|
| `supports_developer_role` | true | false | true | true | true | true | true |
| `max_tokens_field` | MaxTokens | MaxTokens | — | — | MaxTokens | MaxCompletion | MaxCompletion |
| `done_breaks_stream` | false | false | — | false | true | false | false |
| `send_session_affinity_headers` | true | false | true | false | false | false | false |
| `supports_long_cache_retention` | true | false | true | false | false | false | false |
| `supports_cache_control_on_tools` | false | false | true | false | false | false | false |
| `thinking_format` | — | DeepSeek | Anthropic | — | — | — | OpenAi |

## 3. 流式事件协议

`crates/uncode-ai/src/api.rs`

```rust
pub enum StreamEvent {
    TextDelta(String),                                    // 文本增量
    ThinkingDelta(String),                                // 思考内容增量
    ToolCallStart { id: String, name: String },           // 工具调用开始
    ToolCallDelta { id: String, arguments: String },      // 工具参数增量
    ToolCallEnd { id: String, name: String, arguments: Value }, // 工具调用完成
    Usage(UsageInfo),                                     // Token 用量
    Error { reason: StopReason, message: String },        // 错误事件
    Done { reason: StopReason },                          // 流结束
}
```

**工具调用三阶段协议**：`ToolCallStart` → `ToolCallDelta`（可多次） → `ToolCallEnd`。每个流必须以 `Done` 事件结束。

**`collect_assistant_message()`**：消费整个流，构建完整 `Message`。将 TextDelta 合并为文本、ThinkingDelta 合并为思考内容、工具调用阶段组装为 `ToolCall` 结构体。

## 4. API 协议实现

所有实现满足 `Api` trait：

```rust
#[async_trait]
pub trait Api: Send + Sync {
    fn api_name(&self) -> &'static str;
    async fn stream(&self, model: &Model, context: &Context, options: &StreamOptions)
        -> Result<BoxStream<'static, StreamEvent>, UncodeError>;
    async fn complete(&self, model: &Model, context: &Context, options: &StreamOptions)
        -> Result<Message, UncodeError>;  // 默认实现：消费 stream
}
```

### 4.1 OpenAI Completions (`openai-completions`)

**文件**：`crates/uncode-ai/src/providers/openai_completions.rs`

**端点**：`POST {base_url}/chat/completions`

**覆盖供应商**：OpenAI、DeepSeek、GLM、OpenRouter、Groq、Cerebras、Mistral、xAI

**请求构建**：

- 系统消息：`CompatConfig.supports_developer_role` 为 true 时使用 `developer` 角色
- Thinking 参数根据 `CompatConfig.thinking_format` 分三路：
  - **DeepSeek**：`{ thinking: { type: "enabled" }, reasoning_effort: <mapped_value> }`
  - **OpenRouter**：`{ reasoning: { effort: <mapped_value> } }`
  - **通用**（supports_reasoning_effort）：`{ reasoning_effort: <mapped_value> }`
- 会话亲和：`send_session_affinity_headers` → `session_id` Header + `prompt_cache_key` + `prompt_cache_retention`

**SSE 解析**：解析 `data: {choices: [{delta: {content, tool_calls}}]}` 格式。`StreamState` 跟踪进行中的工具调用，`finish_reason` 触发 `Done` 事件。

**ThinkingDelta 提取**：当 `thinking_format == DeepSeek` 时，从 `delta.reasoning_content` 提取思考内容。

### 4.2 Anthropic Messages (`anthropic-messages`)

**文件**：`crates/uncode-ai/src/providers/anthropic_messages.rs`

**端点**：`POST {base_url}/messages`

**认证**：`x-api-key` Header + `anthropic-version: 2023-06-01`

**请求构建**：

- 消息格式为 Anthropic 原生 content blocks：`[{type: "text", text}, {type: "tool_use", id, name, input}]`
- 工具结果以 `role: "user"` + `content: [{type: "tool_result", tool_use_id, content}]` 传递
- 图片支持 base64 编码
- Thinking 参数：`{ thinking: { type: "enabled", budget_tokens: N } }`

**SSE 解析**：Anthropic 使用结构化事件类型：

| 事件类型 | 处理 |
|----------|------|
| `message_start` | 提取初始 usage |
| `content_block_start` | 识别 text/tool_use 块，触发 ToolCallStart |
| `content_block_delta` | 文本增量或 `input_json_delta` 工具参数 |
| `content_block_stop` | 工具参数 JSON 解析完成，触发 ToolCallEnd |
| `message_delta` | 提取 usage + stop_reason → Done |

### 4.3 Gemini Generative AI (`google-generative-ai`)

**文件**：`crates/uncode-ai/src/providers/gemini_generative.rs`

**端点**：`POST {base_url}/models/{model_id}:streamGenerateContent?alt=sse`

**认证**：`x-goog-api-key` Header

**请求构建**：

- 系统提示作为 `role: "user"` + `System: {prompt}`，紧接 `role: "model"` + `"Understood."`
- 工具定义映射为 `functionDeclarations`
- 输出配置：`maxOutputTokens`、`temperature`

**SSE 解析**：`candidates[0].content.parts` 提取文本和 `functionCall`。`finishReason` 触发 `Done` 事件。

**工具调用**：一次性发射 ToolCallStart + ToolCallDelta + ToolCallEnd（Gemini 在单个 chunk 中返回完整参数）。

### 4.4 Ollama Native (`ollama-native`)

**文件**：`crates/uncode-ai/src/providers/ollama_native.rs`

**端点**：`POST {base_url}/api/chat`

**请求构建**：

- 消息格式与 OpenAI 类似但温度放入 `options: { temperature }`
- 工具定义使用 `tools` 字段

**流解析**：非 SSE，每行一个 JSON 对象。`done: true` 表示结束，`done_reason` 映射到 StopReason。工具调用在 `message.tool_calls` 中返回。

## 5. 注册表

### 5.1 ApiRegistry

`crates/uncode-ai/src/api_registry.rs`

```rust
pub struct ApiRegistry {
    apis: HashMap<String, Arc<dyn Api>>,
}
```

启动时构建，运行时只读。提供 `register()`、`get()`、`has()`、`names()` 方法。

### 5.2 ModelRegistry

`crates/uncode-ai/src/model_registry.rs`

```rust
pub struct ModelRegistry {
    models: HashMap<String, Model>,
}
```

`from_builtin()` 加载内置模型，`merge_user_models()` 合并用户自定义模型（同名覆盖）。通过 `get(id)` 按 ID 查找模型，`all_models()` 返回所有模型。

### 5.3 路由流程

```
用户请求 model_id = "deepseek-chat"
    │
    ▼ ModelRegistry.get("deepseek-chat")
Model { api: "openai-completions", ... }
    │
    ▼ ApiRegistry.get("openai-completions")
OpenAiCompletionsApi
    │
    ▼ api.stream(model, context, options)
BoxStream<StreamEvent>
```

## 6. Thinking 系统

### 6.1 参数构建流程

```
options.thinking_level = Some(High)
    │
    ▼ model.thinking_level_map.get(High)
Some("high")  // DeepSeek 映射值
    │
    ▼ compat.thinking_format
    ├── DeepSeek  →  { thinking: { type: "enabled" }, reasoning_effort: "high" }
    ├── Anthropic →  { thinking: { type: "enabled", budget_tokens: 10000 } }
    ├── OpenRouter → { reasoning: { effort: "high" } }
    └── 通用       → { reasoning_effort: "high" }
```

### 6.2 思考内容提取

| 供应商 | 提取方式 |
|--------|----------|
| DeepSeek | `delta.reasoning_content` → `ThinkingDelta` |
| Anthropic | 独立的 thinking content block |
| 其他 | 不支持，不提取 |

## 7. 会话亲和与缓存

OpenAI 和 Anthropic 支持 prompt cache，通过 `session_id` 实现缓存亲和：

**OpenAI**：
- Header: `session_id` + `x-client-request-id`
- Body: `prompt_cache_key` + `prompt_cache_retention: "24h"`（当 `cache_retention == Long`）

**Anthropic**：
- `send_session_affinity_headers: true` + `supports_long_cache_retention: true`
- API 级别的 cache control

## 8. 定价

`ModelPricingPerMillion` 记录每百万 token 的 USD 价格：

```rust
pub struct ModelPricingPerMillion {
    pub input: f64,       // 输入价格
    pub output: f64,      // 输出价格
    pub cache_read: f64,  // 缓存读取价格
    pub cache_write: f64, // 缓存写入价格
}
```

当前定价数据（USD/百万 token）：

| 模型 | Input | Output | Cache Read | Cache Write |
|------|-------|--------|------------|-------------|
| DeepSeek V3 | 0.27 | 1.10 | 0.07 | 0.27 |
| DeepSeek R1 | 0.55 | 2.19 | 0.14 | 0.55 |
| GPT-4o Mini | 0.15 | 0.60 | — | — |
| GPT-4o | 2.50 | 10.00 | — | — |
| Claude Sonnet 4.6 | 3.00 | 15.00 | 0.30 | 3.75 |

## 9. 内置模型

`builtin_models()` 定义 13 个模型，覆盖 9 个供应商：

| ID | 供应商 | API 协议 | 推理 |
|----|--------|----------|------|
| `deepseek-chat` | DeepSeek | openai-completions | ✓ |
| `deepseek-reasoner` | DeepSeek | openai-completions | ✓ |
| `glm-4-flash` | GLM (智谱) | openai-completions | |
| `gpt-4o-mini` | OpenAI | openai-completions | |
| `gpt-4o` | OpenAI | openai-completions | |
| `claude-sonnet-4-6` | Anthropic | anthropic-messages | ✓ |
| `gemini-2.0-flash` | Gemini | google-generative-ai | |
| `openrouter-auto` | OpenRouter | openai-completions | |
| `ollama` | Ollama | ollama-native | |
| `llama-3.3-70b-versatile` | Groq | openai-completions | |
| `llama-3.3-70b` | Cerebras | openai-completions | |
| `mistral-large-latest` | Mistral | openai-completions | |
| `grok-3-mini` | xAI | openai-completions | ✓ |

## 10. 错误处理

所有 HTTP 错误统一映射为 `UncodeError` 枚举：

| HTTP 状态码 | → UncodeError |
|-------------|---------------|
| 401 / 403 | `LlmAuth(body)` |
| 429 | `LlmRateLimit(body)` |
| 其他 | `Llm("HTTP {status}: {body}")` |

流内错误通过 `StreamEvent::Error { reason, message }` 传递，`reason` 为 `StopReason::Error`。

## 11. 配置集成

用户可通过 `~/.uncode/config.toml` 自定义模型：

```toml
[[user_models]]
id = "my-model"
api = "openai-completions"
provider = "custom"
base_url = "https://api.example.com/v1"

[user_models.compat]
supports_developer_role = false
thinking_format = "deepseek"
max_tokens_field = "max_tokens"
```

`Model::from_user_config()` 将 `UserModelConfig` 转换为 `Model`，合并 `UserCompatConfig` 中的所有可选字段。

## 12. 扩展指南

### 新增 OpenAI 兼容供应商

1. 在 `builtin_models()` 中添加 `Model` 条目，`api` 设为 `"openai-completions"`
2. 按需配置 `CompatConfig`（如 `done_breaks_stream`、`max_tokens_field`）
3. 无需编写任何驱动代码

### 新增独立 API 协议

1. 在 `uncode-ai/src/providers/` 下实现 `Api` trait
2. 在 `uncode-cli` 或 `uncode-tui` 初始化时将实现注册到 `ApiRegistry`
3. 在 `builtin_models()` 中添加使用该协议的 `Model` 条目

### 新增 Thinking 格式

1. 在 `ThinkingFormat` 枚举中添加变体
2. 在对应 provider 的请求构建函数中添加格式分支
3. 在 `Model::from_user_config()` 的 thinking_format 映射中添加新格式

## 13. 顶层入口

`uncode-ai` 提供两个顶层函数，简化上层调用：

```rust
// 流式补全
pub async fn stream(model, context, options, api_registry)
    -> Result<BoxStream<StreamEvent>, UncodeError>

// 非流式补全
pub async fn complete(model, context, options, api_registry)
    -> Result<Message, UncodeError>
```

两者均通过 `ApiRegistry.get(model.api)` 路由到对应实现。
