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
uncode -i                  # 交互模式：启动四面板TUI，持续对话
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
┌──────────────────────────────────────────┐
│ uncode v0.1 | 就绪                       │  ← 状态栏
├──────────────────────┬───────────────────┤
│ 📋 任务清单           │ 🛠️ 工具调用       │
│ 等待中...             │ 等待中...         │
├──────────────────────┼───────────────────┤
│ 💭 思考过程           │ 📝 阶段总结       │
│ 等待中...             │ 等待中...         │
├──────────────────────┴───────────────────┤
│ > _                                      │  ← 输入区
└──────────────────────────────────────────┘
```

**操作：**

| 操作 | 说明 |
|------|------|
| 输入文字 + Enter | 发送消息给 Agent |
| `/simple` + Enter | 切换到简化两面板视图 |
| `/full` + Enter | 恢复四面板视图 |
| `/help` + Enter | 显示帮助 |
| ↑↓ | 浏览历史命令 |
| `Ctrl+A/E/K/U` | Emacs 行编辑 |
| Tab | 补全（路径/命令） |
| Ctrl+D | 切换代码细节视图 |
| Ctrl+E | 全屏代码视图 |
| Ctrl+L | 锁定布局 |
| Esc | 退出 |

**四面板说明：**

| 面板 | 显示内容 |
|------|---------|
| 任务清单 | Agent 拆解的子任务和进度 |
| 工具调用 | 正在执行的工具（read/write/bash 等） |
| 思考过程 | LLM 的推理链，Markdown 渲染 |
| 阶段总结 | 每轮完成后的里程碑总结 |

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
