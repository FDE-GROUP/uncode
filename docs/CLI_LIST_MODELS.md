# 模型列表命令

## 背景

uncode 支持 7 个 LLM 供应商，每个供应商有多个模型。用户需要查看当前配置下可用的模型列表，以便选择合适的模型。

## 目标

- CLI 支持 `uncode models` 列出所有可用模型
- TUI 模型选择器增强，显示模型详情
- 区分已配置（有 API key）和未配置的供应商

## 设计

### CLI 命令

```
uncode models                列出所有可用模型
uncode models --all          包含未配置的供应商
uncode models --json         JSON 格式输出
```

输出格式：
```
PROVIDER     MODEL              CONFIGURED
deepseek     deepseek-v3        ✅
deepseek     deepseek-v4-pro    ✅
glm          glm-5.1            ✅
ollama       llama3             ✅ (local)
openai       gpt-4o             ❌ (no API key)
anthropic    claude-sonnet-4-6  ❌ (no API key)
```

### 模型注册表扩展

在 `uncode-llm` 的 registry 中维护模型元数据：

```rust
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub context_window: u64,
    pub supports_tools: bool,
    pub supports_thinking: bool,
    pub supports_vision: bool,
}
```

### 模型列表来源

1. **内置列表**：编译时硬编码每个供应商的已知模型
2. **动态发现**（可选）：
   - Ollama: `GET /api/tags`
   - OpenAI: `GET /v1/models`
   - DeepSeek: `GET /v1/models`

### TUI 增强

模型选择器 overlay 显示额外信息：
- 上下文窗口大小
- 工具/思考/视觉能力标记
- 已配置/未配置状态

## 验收标准

- [ ] `uncode models` 列出可用模型
- [ ] 区分已配置和未配置的供应商
- [ ] `--json` 输出可供脚本消费
- [ ] TUI 模型选择器显示模型详情
