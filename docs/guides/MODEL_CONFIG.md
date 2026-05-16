# 模型配置指南

## 配置规范

UnCode 遵循 **JSON 配置文件规范**，所有模型相关配置集中存储在一个 JSON 文件中：

```
~/.config/uncode/config.json
```

此路径遵循 **XDG Base Directory 规范**（Linux 桌面应用配置存储标准）：

| 目录 | XDG 变量 | UnCode 用途 |
|------|----------|-------------|
| `~/.config/uncode/` | `$XDG_CONFIG_HOME` | 配置文件（config.json） |
| `~/.local/share/uncode/` | `$XDG_DATA_HOME` | 会话数据（JSONL） |

> Windows 对应 `%APPDATA%\uncode\`，macOS 对应 `~/Library/Application Support/uncode/`。UnCode 通过 `dirs` crate 自动解析，首次运行时自动生成，无需手动创建。

整个配置结构分为 **4 个部分**：

| 部分 | 键 | 必填 | 说明 |
|------|-----|------|------|
| **运行参数** | `model`、`max_tokens`、`temperature` | 是 | 默认模型及全局生成参数，切换模型时不受影响 |
| **供应商认证** | `providers` | 按需 | 各 LLM 供应商的 API Key / 连接信息，决定哪些供应商可用 |
| **模型注册** | `models` | 否 | 可用模型列表，不写时使用内置默认。用于限定可切换的模型范围 |
| **自定义端点** | 各 provider 内的 `base_url` | 否 | 覆盖供应商默认 API 地址，用于代理或自建服务 |

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
  },
  "models": [
    {
      "id": "deepseek-v4-pro",
      "provider": "deepseek",
      "display_name": "DeepSeek V4 Pro"
    }
  ]
}
```

## 第一部分：运行参数

控制 UnCode 启动时使用的默认模型和全局生成行为。

### model（必填）

默认模型 ID。必须是 `models` 列表中或UnCode内置的默认模型列表中的某个 `id`。

可通过命令行 `--model <id>` 临时覆盖，例如 `uncode --model glm-5.1 "用户提问内容"`。

### max_tokens

单次请求的最大输出 token 数。**全局生效**，切换模型不改变此值。默认 `8192`。

### temperature

生成温度，范围 0.0–2.0。越低越确定，越高越随机。**全局生效**，切换模型不改变此值。默认 `0.7`。

## 第二部分：供应商认证

### providers（按需配置）

每个供应商配置一个对象，包含 `api_key`。可以配置的供应商：

| 供应商 | 说明 |
|--------|------|
| `deepseek` | DeepSeek 系列模型 |
| `glm` | 智谱 GLM 系列模型 |
| `ollama` | 本地 Ollama，字段为 `host`（默认 `http://localhost:11434`） |
| `openrouter` | OpenRouter 聚合网关 |
| `openai` | OpenAI 系列模型 |
| `anthropic` | Anthropic Claude 系列模型 |
| `gemini` | Google Gemini 系列模型 |

只有配置了 `api_key`（或 Ollama 的 `host`）的供应商才会被视为"已配置"，其模型才会出现在 TUI 的模型切换列表中。

## 第三部分：模型注册

自定义可用模型列表。这个字段**不是必填**，有两种设置方式：

- **不写**（默认）：UnCode 使用内置的默认模型列表，定义在 `crates/uncode-core/src/config.rs` 的 `default_models()` 函数中。你的 `config.json` 里看不到这个列表，但它实际生效。当前内置列表为：
- **手动写**：只使用你指定的模型。如果你想缩小可用范围（例如只保留 `deepseek-v4-pro`），就显式写出。

```json
[
  { "id": "deepseek-v3",        "provider": "deepseek", "display_name": "DeepSeek V3" },
  { "id": "deepseek-v4-pro",   "provider": "deepseek", "display_name": "DeepSeek V4 Pro" },
  { "id": "glm-5.1",           "provider": "glm",      "display_name": "GLM 5.1" },
  { "id": "ollama",            "provider": "ollama",    "display_name": "Ollama (local)" }
]
```

自定义时，每个模型对象的字段：

| 字段 | 必填 | 说明 |
|------|------|------|
| `id` | 是 | 模型唯一标识，也是 API 请求中使用的模型名 |
| `provider` | 是 | 所属供应商，需与 `providers` 中的键一致 |
| `display_name` | 是 | TUI 中显示的名称 |
| `max_tokens` | 否 | 模型上下文窗口大小，默认 `128000` |
| `supports_vision` | 否 | 是否支持图片输入，默认 `false` |
| `supports_tools` | 否 | 是否支持工具调用，默认 `false` |

### 模型过滤规则

TUI 模型切换列表（Ctrl+P 或 `/model`）只显示**同时满足**以下条件的模型：

1. 模型的 `provider` 在 `providers` 中已配置（有 API Key）
2. 模型在 `models` 列表中（或使用默认列表）

**举例**：你的配置中只配了 `deepseek` 的 Key，但 `models` 默认包含 `glm-5.1`。由于 `glm` 供应商未配置，`glm-5.1` 不会出现在切换列表中。

**举例**：要只使用 `deepseek-v4-pro`，在配置中指定：

```json
"models": [
  {
    "id": "deepseek-v4-pro",
    "provider": "deepseek",
    "display_name": "DeepSeek V4 Pro"
  }
]
```

## 第四部分：自定义端点

当使用代理、镜像站或自建服务时，可在各供应商配置中添加 `base_url` 覆盖默认 API 地址：

```json
"deepseek": {
  "api_key": "your-key",
  "base_url": "https://your-proxy.example.com/v1"
}
```

此字段可选，不写时使用供应商官方默认地址。

## 模型切换方式

### 在 TUI 中

- **Ctrl+P** — 循环切换可用模型
- **/model** — 弹出模型选择器，↑/↓ 选择，Enter 确认，Esc 取消
- **/model <名称>** — 直接切换到指定模型，如 `/model glm-5.1`

### 在命令行

```bash
uncode --model glm-5.1 "你的问题"
```

## 注意事项

1. **不同模型的能力差异很大**。GLM 支持视觉输入，DeepSeek 不支持。请根据任务选择合适的模型。
2. **切换模型时思考等级会自动重置**。不同模型支持的思考等级不同。
3. **会话与模型绑定**。切换模型后新消息使用新模型，历史消息不受影响。
4. **配置文件权限**。`config.json` 包含 API Key，建议设置严格权限：
   ```bash
   chmod 600 ~/.config/uncode/config.json
   ```
5. **API Key 安全**。不要将包含 API Key 的配置文件提交到 Git 仓库。确保 `~/.config/uncode/` 在你的 `.gitignore` 全局规则中。

## 常见问题

### 怎么知道我有哪些可用模型？

在 TUI 中输入 `/models` 命令，或在命令行运行：

```bash
uncode models
```

### 切换模型后没有反应？

检查目标模型的供应商是否在 `providers` 中配置了有效的 API Key。

### 如何添加 OpenAI 模型？

```json
{
  "providers": {
    "openai": {
      "api_key": "sk-..."
    }
  },
  "models": [
    {
      "id": "gpt-4o",
      "provider": "openai",
      "display_name": "GPT-4o",
      "supports_vision": true,
      "supports_tools": true
    }
  ]
}
```
