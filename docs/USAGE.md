# uncode 使用教程

## 一、启动原理

uncode 启动分三步：

```
配置文件 → LLM供应商注册 → 模式选择
```

**配置文件**（`~/.config/uncode/config.json`）告诉 uncode 用哪个模型、API key 是什么。启动时 uncode 读取它，注册对应的 LLM 驱动。

**两种运行模式：**

```
uncode "你的问题"          # print模式：一问一答，输出到终端
uncode -i                  # 交互模式：启动对话驱动TUI，持续对话
```

---

## 二、配置

```json
{
  "model": "deepseek-v4-pro",
  "providers": {
    "deepseek": { "api_key": "你的DeepSeek密钥" },
    "glm": { "api_key": "你的GLM密钥" }
  }
}
```

放在 `~/.config/uncode/config.json`。

---

## 三、Print 模式（命令行）

适合快速问答和脚本集成。

```bash
cd /home/arligle/EA/uncodenow

# 一问一答
cargo run -p uncode-cli -- --model deepseek-v4-pro "分析 src/main.rs 的功能"

# 从 GitHub Issue 开始
export GITHUB_TOKEN=ghp_xxx
cargo run -p uncode-cli -- --issue 42

# 全参数
cargo run -p uncode-cli -- --model glm-5.1 --session my-session "你好"
```

**原理：** CLI 读取参数 → 注册 LLM → 创建 Agent → 发送用户消息 → 流式 LLM 回答 + 工具执行 → 打印结果

---

## 四、交互模式（TUI）

```bash
cd /home/arligle/EA/uncodenow
cargo run -p uncode-cli -- -i
```

**启动后看到：**

```
┌──────────────────────────────────────────────────────────┐
│ uncode v0.2 │ DeepSeek-V3 │ session:abc3 │ 就绪          │  ← 状态栏
├──────────────────────────────────────────────────────────┤
│                                                          │
│  欢迎使用 uncode。描述你的需求，我会帮你完成。              │  ← 对话区
│                                                          │
├──────────────────────────────────────────────────────────┤
│ > _                                                      │  ← 输入栏
├──────────────────────────────────────────────────────────┤
│ Ctrl+X 命令 │ ↑↓ 历史 │ Tab 补全 │ Enter 发送            │  ← 快捷键提示
└──────────────────────────────────────────────────────────┘
```

**操作：**

| 操作 | 说明 |
|------|------|
| 输入文字 + Enter | 发送消息给 Agent |
| `@file` | 引用文件，Agent 自动读取 |
| `!command` | 直接执行 shell 命令 |
| `/help` + Enter | 显示帮助 |
| `/model` | 查看/切换模型 |
| `/session` | 查看/切换会话 |
| `/thinking` | 切换思考过程显示 |
| `/undo` | 撤销 Agent 最近一次文件修改 |
| ↑↓ | 浏览历史命令 |
| `Ctrl+A/E/K/U` | Emacs 行编辑 |
| Tab | 补全（路径/命令） |
| `Ctrl+X` | Leader Key 快捷操作 |
| Esc | 取消当前操作 |

**原理：**

```
用户输入 → AgentLoop.run()
              ├── 构建系统提示 + 工具定义
              ├── 调用 LLM（流式）
              ├── 解析响应 → 广播 AgentEvent
              ├── TuiEngine 订阅事件 → 实时刷新面板
              └── 工具执行 → 结果追加 → 继续循环
```

---

## 五、会话管理

```bash
# 新建会话
cargo run -p uncode-cli -- --session my-session "开始工作"

# 从 Issue 创建
cargo run -p uncode-cli -- --issue 42 --session fix-42
```

会话数据存储在 `~/.uncode/sessions/{id}.jsonl`。每条消息一行 JSON，支持回放和分支。

---

## 六、构建优化

```bash
# 发布构建（更快）
cargo build --release -p uncode-cli

# 运行发布版
./target/release/uncode -i
```

---

## 七、故障排查

| 问题 | 原因 | 解决 |
|------|------|------|
| `no LLM driver available` | config.json 不存在或格式错误 | 检查 `~/.config/uncode/config.json` |
| `LLM auth failed` | API key 错误 | 检查 `api_key` 是否正确 |
| `LLM rate limited` | 请求过快 | 等待几秒后重试 |
| TUI 显示异常 | 终端不支持 | 用 WezTerm/Alacritty/Kitty |
| TUI 无 Agent 响应 | LLM 调用失败 | 查看终端中的错误信息 |
