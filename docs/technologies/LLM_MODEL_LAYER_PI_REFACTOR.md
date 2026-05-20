# LLM 模型层 Pi 对齐重构方案

> 状态：Phase 1 已落地；Phase 2 已落地（本分支）  
> 关联：[PI_LLM_LAYER.md](../pi-technologies/PI_LLM_LAYER.md)、[MODEL_LAYER_DESIGN.md](../pi-technologies/MODEL_LAYER_DESIGN.md)、[UNCODE_PI_ALIGNMENT_AND_EVALUATION.md](UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) §3.4

## 1. 动机

uncode 当前采用 **API-first**（4 个 `Api` 协议实现 + `Model` 数据表），与 Pi 的 **供应商优先 + `OpenAICompletionsCompat` 预设** 在工程上等价，但带来两点差距：

1. **Compat 分散**：每个内置 `Model` 手写部分 `compat` 字段，厂商级默认值重复且易漏（如 `thinking_format` 只写在 `Model.thinking_format` 而未同步到 `compat`）。
2. **调用路径偏裸**：上层直接 `stream()`，缺少 Pi `streamSimple` 侧的 **thinking level 钳制**、选项归一化与后续可插拔的 **transform / proxy** 钩子。

用户目标：在保留 API-first 优势的前提下，获得 Pi 式 **按厂商/模型精细化配置** 与 **统一简易入口**。

## 2. 设计原则（不变与新增）

| 原则 | 说明 |
|------|------|
| **保留 API-first** | 不回到「每供应商一个驱动 crate」；仍以 `openai-completions` 等 4 协议为路由单位。 |
| **引入 Provider Preset** | 对齐 Pi 的 vendor 级 `OpenAICompletionsCompat`：一份预设 + 模型级 delta。 |
| **单一有效 Compat** | 运行时 `Model::effective_compat()` = preset ⊕ model 覆盖；Provider 只读有效值。 |
| **stream_simple 入口** | 对齐 Pi `streamSimple`：钳制 thinking、合并 compat，再调用 `stream()`。 |
| **渐进迁移** | 内置模型表可逐步「瘦身」为仅 id/定价/窗口 + 少量 delta。 |

## 3. 目标架构

```
uncode-agent (LoopEngine)
    │  StreamOptions.thinking_level
    ▼
uncode-ai::stream_simple()     ← 新增：clamp + effective_compat
    ▼
uncode-ai::stream()
    ▼
ApiRegistry → OpenAiCompletions / Anthropic / …
```

### 3.1 新增类型：`ProviderPreset`

```rust
// crates/uncode-ai/src/provider_preset.rs
pub struct ProviderPreset {
    pub id: &'static str,           // 与 Model.provider 对齐
    pub default_api: &'static str,
    pub default_base_url: &'static str,
    pub compat: CompatConfig,       // 厂商完整 Compat（对齐 Pi 表）
    pub thinking_level_map: HashMap<ThinkingLevel, Option<String>>,
}
```

- `builtin_provider_presets()`：deepseek、glm、openai、anthropic、gemini、groq、cerebras、openrouter、mistral、xai、ollama。
- `apply_provider_preset(model)`：在 `ModelRegistry::from_builtin()` / `from_user_config` 后应用。

### 3.2 `CompatConfig` 合并语义

- `CompatConfig::merge_with_overlay(base, overlay)`：以 **overlay 相对 `Default` 的非默认字段** 覆盖 base（bool/enum 与 default 比较；`Option` 用 `or`）。
- `Model::effective_compat()`：查 preset → merge → 缓存可选（Phase 2）。

### 3.3 `stream_simple`

```rust
pub async fn stream_simple(
    model: &Model,
    context: &Context,
    options: &StreamOptions,
    api_registry: &ApiRegistry,
) -> Result<BoxStream<'static, StreamEvent>, UncodeError>
```

行为（Phase 1）：

1. `thinking_level` ← `clamp_thinking_level(requested, model)`（使用合并后的 `thinking_level_map`）。
2. `model.compat` ← `effective_compat()` 的副本再传入 `stream()`。
3. Phase 2+：`on_payload` / harness `transform_context` 快照、Anthropic thinking SSE、proxy stream。

## 4. 分阶段路线图

| 阶段 | 内容 | 验收 |
|------|------|------|
| **P1** | `ProviderPreset`、`effective_compat`、`stream_simple`、LoopEngine/Compaction 改用 `stream_simple` | ✅ |
| **P2** | 内置模型表瘦身；`from_user_config` / `from_model_config` 套 preset；文档同步 `UNCODE_LLM_LAYER.md` | ✅ |
| **P3** | Anthropic thinking block SSE；`StreamOptions` 全链路（transport/retry 与 Pi 对齐） | Anthropic reasoning 流可见 |
| **P4** | Harness hooks：`transform_context` / proxy stream（可选） | 与 Pi 高级特性表一致 |

## 5. 与 Pi 的差异（刻意保留）

- **不**复制 Pi 的 per-vendor npm 包与 OAuth 表；OAuth 仍走配置 + 后续独立 Issue。
- **不**放弃 4 协议拆分；Gemini/Ollama 仍独立 `Api` 实现。
- **动态注册 Api**：Rust 侧仍静态注册；Pi 的 `registerApiProvider` 动态性列入 P4 评估。

## 6. 配置面

用户 `[[user_models]]` 行为不变：`provider` 决定 preset，`compat` 子表仍为模型级覆盖。文档补充「推荐只写 delta」。

## 7. Issue 追踪

-  umbrella：`feat: LLM 模型层 Pi 对齐重构`（待建）
-  closes 关系：P1 可独立 PR `feat/N-llm-provider-preset-stream-simple`
