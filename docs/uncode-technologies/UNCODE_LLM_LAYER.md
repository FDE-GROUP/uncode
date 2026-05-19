# uncode LLM 抽象层

> Api trait + 4 种协议实现 + 流式协议 | 基于源码分析，2026-05 修订

uncode-ai 是 LLM 通信的核心抽象层。所有 Provider 实现统一的 `Api` trait，通过 `StreamEvent` 枚举向下游传递流式数据。内置 13 个模型，覆盖 4 种 API 协议跨 10+ LLM 服务商。

---

## Api trait

```rust
#[async_trait]
pub trait Api: Send + Sync {
    fn api_name(&self) -> &'static str;

    async fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError>;

    async fn complete(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<Message, UncodeError> {
        // 默认实现：消费整个 stream → collect_assistant_message()
        let s = self.stream(model, context, options).await?;
        collect_assistant_message(s).await
    }
}
```

`stream()` 是核心方法，返回 `BoxStream<'static, StreamEvent>`。`complete()` 有默认实现，将整个流消费为单条 `Message`。

---

## StreamEvent 流式协议

```rust
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments: String },
    ToolCallEnd(Box<ToolCallEndData>),
    Usage(UsageInfo),
    Error { reason: StopReason, message: String },
    Done { reason: StopReason },
}
```

### 工具调用三阶段协议

```
ToolCallStart { id, name }
    ↓
ToolCallDelta { id, arguments: "片段1" }
ToolCallDelta { id, arguments: "片段2" }
    ↓ ...（消费者必须拼接 arguments）
ToolCallEnd { id, name, arguments: 完整的 JSON Value }
```

每个流**必须**以 `StreamEvent::Done` 结束。

### 消费端：collect_assistant_message

```rust
pub async fn collect_assistant_message(
    stream: BoxStream<'static, StreamEvent>,
) -> Result<Message, UncodeError> {
    // 拼接 TextDelta → text
    // 拼接 ThinkingDelta → thinking
    // 累积 ToolCall → Vec<ToolCall>
    // 取最后一个 Usage → usage
    // 取最后一个 Done.reason → stop_reason
    // 组装 Message { role: Assistant, content, usage, stop_reason }
}
```

---

## 4 种 API 协议

### OpenAI Completions

| 属性 | 值 |
|------|-----|
| api_name | `"openai-completions"` |
| 端点 | `{base_url}/chat/completions` |
| 认证 | `Authorization: Bearer {key}` |
| 协议 | SSE（`data: {json}`，`data: [DONE]` 终止） |
| 工具定义 | `{"type": "function", "function": {...}}` |
| Thinking | `ThinkingFormat::DeepSeek`（`reasoning_content`）或 `OpenRouter` |

**覆盖服务商**：OpenAI、DeepSeek、GLM、OpenRouter、Groq、Cerebras、Mistral、xAI。

工具调用从 `choices[0].delta.tool_calls[]` 解析，arguments 增量拼接。`CompatConfig` 控制 15 种 Provider 差异（`supports_developer_role`、`max_tokens_field`、`done_breaks_stream` 等）。

### Anthropic Messages

| 属性 | 值 |
|------|-----|
| api_name | `"anthropic-messages"` |
| 端点 | `{base_url}/messages` |
| 认证 | `x-api-key` + `anthropic-version: 2023-06-01` |
| 协议 | SSE（Anthropic 特有事件类型） |
| 工具定义 | `{"name": "...", "input_schema": {...}}` |
| Thinking | `{"thinking": {"type": "enabled", "budget_tokens": N}}` |

特有事件类型：`message_start`、`content_block_start`、`content_block_delta`（含 `input_json_delta`）、`content_block_stop`、`message_delta`。

System prompt 作为顶层 `"system"` 字段发送，不在 messages 数组中。工具结果包裹在 `role: "user"` 消息中（`type: "tool_result"`）。

### Gemini Generative

| 属性 | 值 |
|------|-----|
| api_name | `"google-generative-ai"` |
| 端点 | `{base_url}/models/{model_id}:streamGenerateContent?alt=sse` |
| 认证 | `x-goog-api-key` |
| 协议 | SSE |
| 工具定义 | `{"functionDeclarations": [...]}` |

工具调用以 `functionCall` parts 原子性交付（无 arguments 流式）。Provider 从单个 chunk 合成完整的 `ToolCallStart → ToolCallDelta → ToolCallEnd` 序列。

System prompt 注入为伪 user/model 对话（`"System: {prompt}"` / `"Understood."`）。不支持 Thinking。

### Ollama Native

| 属性 | 值 |
|------|-----|
| api_name | `"ollama-native"` |
| 端点 | `{base_url}/api/chat` |
| 认证 | 无（本地） |
| 协议 | JSONL（每行一个完整 JSON 对象，非 SSE） |
| 工具定义 | 与 OpenAI 相同格式（共享 `build_tools_json()`） |

工具调用从 `message.tool_calls[]` 原子性交付，Provider 合成三阶段序列。完成信号为 `done: true` 字段。Temperature 放在 `options: {"temperature": ...}` 子对象中。

---

## 注册与路由

### ApiRegistry

```rust
pub struct ApiRegistry {
    apis: HashMap<String, Arc<dyn Api>>,
    lazy_loaders: HashMap<String, LazyLoader>,
}
```

支持三种注册模式：
- **Eager**：`register(Arc<dyn Api>)` — 立即可用
- **Lazy**：`register_lazy(name, loader)` — 首次 `get_or_init()` 时加载并缓存
- **Unregister**：`unregister(name)` 移除

### ModelRegistry

```rust
pub struct ModelRegistry {
    models: HashMap<String, Model>,
}
```

12 个内置模型 + 用户自定义模型（`merge_user_models`，同 ID 覆盖）。

### 路由流程

```
uncode_ai::stream(model_id, context, options, api_registry)
    │
    ├── api_registry.get(model.api) → Arc<dyn Api>
    ├── model_registry.get(model_id) → Model
    └── api.stream(model, context, options) → BoxStream<StreamEvent>
```

---

## Model 定义

```rust
pub struct Model {
    pub id: String,                    // "deepseek-chat"
    pub name: String,                  // "DeepSeek V3"
    pub api: String,                   // "openai-completions"
    pub provider: String,              // "deepseek"
    pub base_url: String,
    pub context_window: u32,           // 默认 128_000
    pub max_output_tokens: u32,        // 默认 8192
    pub reasoning: bool,
    pub thinking_format: Option<ThinkingFormat>,
    pub input_modalities: Vec<InputModality>,
    pub pricing: ModelPricingPerMillion,
    pub headers: HashMap<String, String>,
    pub compat: CompatConfig,          // 15 个 Provider 差异字段
    pub thinking_level_map: HashMap<ThinkingLevel, Option<String>>,
}
```

### CompatConfig — Provider 差异矩阵

| 字段 | 说明 | 典型差异 |
|------|------|----------|
| `supports_developer_role` | `"developer"` vs `"system"` 角色 | OpenAI ✅ / DeepSeek ❌ |
| `max_tokens_field` | `"max_tokens"` vs `"max_completion_tokens"` | 新版 OpenAI 用后者 |
| `done_breaks_stream` | `"data: [DONE]"` 是否终止流 | GLM 需要 |
| `thinking_format` | Thinking 输出格式 | DeepSeek vs OpenRouter vs Anthropic |
| `supports_usage_in_streaming` | 流中是否包含 Usage | 部分 Provider 不支持 |
| `supports_cache_control_on_tools` | 工具定义中 cache_control | Anthropic 独有 |
| `supports_eager_tool_input_streaming` | 工具输入即时流式 | OpenAI ✅ / 其他 ❌ |

---

## 错误映射

| HTTP 状态 | 映射为 |
|-----------|--------|
| 401 / 403 | `UncodeError::LlmAuth` |
| 429 | `UncodeError::LlmRateLimit` |
| 其他 | `UncodeError::Llm` |

流级别错误通过 `StreamEvent::Error { reason, message }` 传递。

---

*本文档基于 uncode 源码（`crates/uncode-ai/`）编写。*
