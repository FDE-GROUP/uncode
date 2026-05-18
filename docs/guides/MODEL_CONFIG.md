# 模型配置指南

## 配置文件

```
~/.config/uncode/config.json
```

| 目录 | XDG 变量 | UnCode 用途 |
|------|----------|-------------|
| `~/.config/uncode/` | `$XDG_CONFIG_HOME` | 配置文件 |
| `~/.local/share/uncode/` | `$XDG_DATA_HOME` | 会话数据 |

> Windows 对应 `%APPDATA%\uncode\`，macOS 对应 `~/Library/Application Support/uncode/`。通过 `dirs` crate 自动解析。

配置结构分为 **4 个部分**：

| 部分 | 键 | 必填 | 说明 |
|------|-----|------|------|
| **运行参数** | `model`、`max_tokens`、`temperature` | 是 | 默认模型及全局生成参数 |
| **供应商认证** | `providers` | 按需 | API Key / 连接信息 |
| **模型注册** | `models` | 否 | 自定义模型列表。不写时使用 14 个内置模型；写了则**完全替代**内置列表 |
| **高级自定义** | `user_models` | 否 | 指定 API 协议、compat 等高级字段 |

## 完整配置示例

```json
{
  "model": "deepseek-v4-pro",
  "max_tokens": 8192,
  "temperature": 0.7,
  "providers": {
    "deepseek": {
      "api_key": "sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    },
    "glm": {
      "api_key": "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.xxxxxxxxxxxx"
    }
  }
}
```

只写 `providers` 和 `model` 就够了。UnCode 内置 13 个模型，自动匹配 provider。

## 第一部分：运行参数

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `model` | `"deepseek-v3"` | 默认模型 ID，必须是内置或 `models` 中定义的 |
| `max_tokens` | `8192` | 单次请求最大输出 token（全局生效） |
| `temperature` | `0.7` | 生成温度 0.0–2.0（全局生效） |

可通过 `--model <id>` 临时覆盖。

## 第二部分：供应商认证

只有配置了 API Key 的供应商，其模型才会出现在 TUI 切换列表中。

| 供应商 | 必填字段 | 默认 API 地址 |
|--------|----------|---------------|
| `deepseek` | `api_key` | `https://api.deepseek.com/v1` |
| `glm` | `api_key` | `https://open.bigmodel.cn/api/paas/v4` |
| `ollama` | — | `http://localhost:11434` |
| `openrouter` | `api_key` | `https://openrouter.ai/api/v1` |
| `openai` | `api_key` | `https://api.openai.com/v1` |
| `anthropic` | `api_key` | `https://api.anthropic.com/v1` |
| `gemini` | `api_key` | `https://generativelanguage.googleapis.com/v1beta` |

可选 `base_url` 覆盖默认 API 地址（用于代理或自建服务）：

```json
"deepseek": {
  "api_key": "your-key",
  "base_url": "https://your-proxy.example.com/v1"
}
```

## 第三部分：模型注册（models）

### 默认行为

不写 `models` 字段时，使用 13 个内置模型：

| ID | Provider | 名称 |
|----|----------|------|
| `deepseek-chat` | deepseek | DeepSeek V3 |
| `deepseek-v4-pro` | deepseek | DeepSeek V4 Pro |
| `deepseek-reasoner` | deepseek | DeepSeek R1 |
| `glm-4-flash` | glm | GLM-4 Flash |
| `gpt-4o-mini` | openai | GPT-4o Mini |
| `gpt-4o` | openai | GPT-4o |
| `claude-sonnet-4-6` | anthropic | Claude Sonnet 4.6 |
| `gemini-2.0-flash` | gemini | Gemini 2.0 Flash |
| `openrouter-auto` | openrouter | OpenRouter Auto |
| `llama-3.3-70b-versatile` | groq | Llama 3.3 70B (Groq) |
| `llama-3.3-70b` | cerebras | Llama 3.3 70B (Cerebras) |
| `mistral-large-latest` | mistral | Mistral Large |
| `grok-3-mini` | xai | Grok 3 Mini |

> **Ollama 无内置占位模型。** 使用 Ollama 需在 `models` 中指定实际的模型名（如 `gemma4:26b`），见下方示例。

### 自定义模型列表

写了 `models` 后**完全替代**内置列表，只显示你指定的模型：

```json
{
  "model": "deepseek-v4-pro",
  "providers": {
    "deepseek": { "api_key": "sk-xxx" },
    "glm": { "api_key": "xxx" },
    "ollama": { "host": "http://localhost:11434" }
  },
  "models": [
    { "id": "deepseek-v4-pro", "provider": "deepseek", "display_name": "DeepSeek V4 Pro" },
    { "id": "glm-5.1", "provider": "glm", "display_name": "GLM 5.1" },
    { "id": "gemma4:26b", "provider": "ollama", "display_name": "Gemma 4 26B" }
  ]
}
```

每个模型对象的字段：

| 字段 | 必填 | 说明 |
|------|------|------|
| `id` | 是 | 模型唯一标识，API 请求中使用的名称 |
| `provider` | 是 | 所属供应商，决定 API 协议和默认地址 |
| `display_name` | 是 | TUI 中显示的名称 |
| `max_tokens` | 否 | 上下文窗口大小，默认 128000 |
| `supports_vision` | 否 | 是否支持图片输入，默认 false |

### 模型过滤规则

TUI 切换列表只显示**同时满足**以下条件的模型：

1. 模型的 `provider` 在 `providers` 中已配置（有 API Key）
2. 模型在 `models` 列表中（或使用默认列表）

## 第四部分：高级自定义（user_models）

`user_models` 用于需要指定 API 协议、compat 参数等高级配置的场景。始终与 `models`（或内置列表）合并，同 id 覆盖。

```json
{
  "user_models": [
    {
      "id": "my-model",
      "api": "openai-completions",
      "provider": "my-provider",
      "base_url": "https://api.example.com/v1",
      "api_key": "sk-xxx",
      "context_window": 64000,
      "max_output_tokens": 4096
    }
  ]
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `id` | 是 | 模型 ID |
| `api` | 否 | API 协议，默认 `openai-completions` |
| `provider` | 是 | 供应商名 |
| `base_url` | 否 | API 地址 |
| `api_key` | 否 | 独立 API Key |
| `context_window` | 否 | 默认 128000 |
| `max_output_tokens` | 否 | 默认 8192 |
| `compat` | 否 | Provider 兼容性配置 |

## 模型切换方式

| 方式 | 操作 |
|------|------|
| TUI Ctrl+P | 循环切换可用模型 |
| TUI `/model` | 弹出模型选择器 |
| TUI `/model <id>` | 直接切换 |
| 命令行 | `uncode --model glm-5.1 "问题"` |

## 注意事项

1. **切换模型不影响** `max_tokens` 和 `temperature`（全局参数）
2. **配置文件权限**：`chmod 600 ~/.config/uncode/config.json`
3. **查看可用模型**：`uncode models`
4. **API Key 安全**：不要提交到 Git 仓库
