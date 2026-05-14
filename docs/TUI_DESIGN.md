# uncode TUI 交互设计详案（v2）

> **参照项目：** [earendil-works/pi](https://github.com/earendil-works/pi)
> 本文档基于 Pi 的对话式 TUI 设计理念，结合 uncode 的 Rust 原生架构和多供应商策略进行适配和增强。

## 一、设计目标

uncode TUI 是面向**开发人员**的终端 AI Agent 对话式编程界面。参照 Pi 的设计理念，以**对话驱动**为核心，将 Agent 的思考、工具调用、代码变更自然地嵌入对话流中。**TUI 仅为开发人员设计**——非程序员友好的可视化体验由 Platform（Web UI）承担。

| 原则 | 实现 | Pi 参考 |
|------|------|---------|
| 对话驱动 | 主区域是可滚动的对话历史，用户消息和 Agent 回复交替排列 | Pi ChatContainer |
| 内联工具 | 工具调用和结果折叠在对话流中，每个工具有独立的渲染策略 | Pi ToolExecutionComponent 自定义 renderCall/renderResult |
| 即时反馈 | Agent 的流式输出实时渲染，Markdown + 代码高亮 | Pi 差分渲染 16ms 节流 |
| 权限控制 | 危险操作（写入、执行命令）需用户确认后执行 | — |
| 键盘优先 | 所有操作可通过键盘完成，Leader Key + 直接快捷键体系 | Pi Ctrl+O/T/L + Shift+Tab |
| 渐进披露 | 工具输出和思考过程按需展开，不干扰主对话流 | Pi Ctrl+O toggle tool output |
| 消息队列 | Agent 工作时用户可排队发送后续指令，不阻塞交互 | Pi 消息队列（steering + follow-up） |
| 思考级别 | 6 级思考深度控制，动态编辑器边框颜色反馈 | Pi thinking levels + border color |

### 与 v1 的核心差异

| 维度 | v1（四面板监控） | v2（对话驱动） |
|------|-----------------|---------------|
| 布局 | 四面板：任务/工具/思考/总结 | 单流：状态栏 + 对话区 + 输入栏 |
| 交互模式 | 监控面板，用户观察 Agent 工作 | 对话式，用户与 Agent 交替行动 |
| 工具调用 | 独立面板列表展示 | 内联折叠在对话流中 |
| 代码变更 | 代码细节视图（按需展开） | 内联 diff，展开/折叠切换 |
| 输入方式 | 底部单行输入 | 底部输入栏 + 多行 + @ 引用 + / 命令 |
| 目标用户 | 非程序员优先 | **开发人员专属** |

---

## 二、整体布局

### 2.1 主布局

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│  > 帮我实现用户登录功能，参考 @src/auth.rs                  │  ← 用户消息
│                                                          │
│  uncode                                                  │
│  我来分析现有代码结构，然后实现登录功能。                    │  ← Agent 回复
│                                                          │
│  ┌─ 🛠 read src/auth.rs ─────────────────── ✅ 23ms ─┐  │  ← 内联工具调用（折叠）
│  │  读取了 142 行代码                                  │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  现有代码使用 JWT + Actix-web middleware 模式。           │  ← Agent 继续回复
│  我将实现以下内容：                                       │
│  1. POST /auth/login 端点                                │
│  2. 密码哈希验证                                         │
│  3. JWT token 生成                                       │
│                                                          │
│  ┌─ 🛠 edit src/auth.rs ──── ⚠️ 等待确认 ────────────┐  │  ← 内联工具调用（待确认）
│  │  - fn login_handler() { ... }                       │  │  ← 内联 diff
│  │  + fn login_handler(req: HttpRequest) -> HttpResponse {│
│  │  +     let creds: LoginRequest = req.json()?;        │  │
│  │  +     // 验证凭证...                                │  │
│  │  [Y] 确认  [N] 拒绝  [E] 编辑                        │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌─ $ bash cargo test ── ✅ 3.2s ────────────────────┐  │  ← Bash 执行（独立样式）
│  │  running 4 tests                                    │  │
│  │  test auth::tests::test_login ... ok                │  │
│  │  test result: ok. 4 passed; 0 failed; 0 ignored     │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
├──────────────────────────────────────────────────────────┤
│ > 帮我写一个单元测试 _                                    │  ← 输入栏（边框色反映思考级别）
├──────────────────────────────────────────────────────────┤
│ ~/EA/uncodenow feat/73-docs session:abc3                  │  ← 页脚第 1 行：路径+分支+会话
│ in:12k out:3k cached:8k/2k $0.04 ctx:34% deepseek-v3 🧠  │  ← 页脚第 2 行：Token+费用+模型+思考级
└──────────────────────────────────────────────────────────┘
```

### 2.2 布局分区

| 区域 | 高度 | 内容 | Pi 参考 |
|------|------|------|---------|
| 对话区 | 弹性填充 | 用户消息 + Agent 回复 + 内联工具调用，可滚动 | Pi ChatContainer |
| 输入栏 | 3 行（可展开） | 多行编辑器，边框颜色反映思考级别 | Pi EditorContainer |
| 页脚第 1 行 | 1 行 | 工作目录 + Git 分支 + 会话名称 | Pi Footer line 1 |
| 页脚第 2 行 | 1 行 | Token 统计（输入/输出/缓存读/缓存写）+ 费用 + 上下文使用率 + 模型 + 供应商 + 思考级别指示 | Pi Footer line 2 |

最小终端尺寸：80x24。

### 2.3 页脚详细设计

页脚是 Pi 的标志性设计，提供持续可见的关键信息：

**第 1 行：位置上下文**

```
~/EA/uncodenow feat/73-docs session:abc3
```

- 工作目录（`~` 缩写 home）
- 当前 Git 分支名
- 当前会话 ID（前 8 位）

**第 2 行：模型与资源信息**

```
in:12k out:3k cached:8k/2k $0.04 ctx:34% deepseek-v3 🧠
```

| 字段 | 含义 | 示例 |
|------|------|------|
| `in` | 输入 Token 数 | `12k` |
| `out` | 输出 Token 数 | `3k` |
| `cached` | 缓存读取/缓存写入 Token | `8k/2k` |
| `$` | 本次会话累计费用 | `$0.04` |
| `ctx` | 上下文窗口使用率 | `34%`（超过 80% 时变红预警） |
| 模型名 | 当前使用的 LLM 模型 | `deepseek-v3` |
| 🧠 | 思考级别指示（off/minimal/low/medium/high/xhigh） | 图标随级别变化 |

---

## 三、对话区详细设计

### 3.1 消息类型

对话区由以下消息类型交替构成：

| 类型 | 样式 | 示例 | Pi 参考 |
|------|------|------|---------|
| 用户消息 | `>` 前缀，白色 | `> 帮我实现登录功能` | UserMessageComponent |
| Agent 文本 | `uncode:` 前缀，Markdown 渲染 | 包含代码块、列表、加粗等 | AssistantMessageComponent |
| Agent 思考 | 灰色斜体，默认折叠 | `[💭 思考过程] 点击展开` | — |
| 工具调用 | 方框包裹，每个工具有独立渲染 | `[🛠 read] ✅ 23ms` | ToolExecutionComponent |
| Bash 执行 | 方框包裹，独立样式 `$` 前缀 | `[$ bash cargo test] ✅ 3.2s` | BashExecutionComponent |
| 内联 diff | 红绿高亮，在工具调用方框内 | `- old line` / `+ new line` | — |
| 错误信息 | 红色背景 | `[错误] bash 命令退出码 1` | — |
| 权限请求 | 黄色边框，交互按钮 | `[确认?] edit src/auth.rs` | — |
| 压缩摘要 | 蓝色信息卡片，替代被压缩的历史消息 | `[压缩] 8 条消息被替换为摘要` | CompactionSummaryMessageComponent |
| 分支摘要 | 紫色信息卡片，标记对话分支点 | `[分支] 从 abc3 分支到 def7` | BranchSummaryMessageComponent |
| 排队消息 | 暗色预览，显示在对话区底部 | `[排队中] 接下来写单元测试` | pendingMessagesContainer |

### 3.2 用户消息渲染

```
 > 帮我实现用户登录功能，参考 @src/auth.rs 中的现有结构
```

- `>` 前缀标识用户消息
- `@file` 引用高亮显示为青色
- `!command` 前缀高亮显示为黄色

### 3.3 Agent 文本消息渲染

Agent 的流式输出使用 Markdown 渲染：

- **代码块**：语法高亮（基于 tree-sitter），圆角边框，文件路径标签
- **粗体/斜体**：标准样式
- **列表**：缩进 + 项目符号
- **链接**：下划线
- **行内代码**：反色背景

流式输出时有光标闪烁效果，完成后停止。

### 3.4 Agent 思考过程

默认折叠，显示为一行摘要：

```
 [💭 思考过程 — 分析代码结构，规划实现方案] 点击展开
```

展开后显示完整的思考文本，灰色文字。通过 `/thinking` 命令切换默认行为。

### 3.5 工具调用渲染

工具调用以折叠方框的形式嵌入对话流。参照 Pi 的 ToolExecutionComponent 设计，每个工具有独立的 `renderCall()` 和 `renderResult()` 渲染策略：

**折叠状态（默认）：**

```
 ┌─ 🛠 read src/auth.rs ─────────────── ✅ 23ms ─┐
 │  读取了 142 行代码                                │
 └────────────────────────────────────────────────┘
```

**展开状态：**

```
 ┌─ 🛠 read src/auth.rs ─────────────── ✅ 23ms ─┐
 │  1  use actix_web::{web, HttpRequest, HttpResponse};│
 │  2  use jsonwebtoken::{encode, decode, Header, Validation};│
 │  3                                                   │
 │  4  pub struct Claims {                              │
 │  ...                                                 │
 │  [显示更多]                                           │
 └────────────────────────────────────────────────────┘
```

**工具调用状态颜色（参照 Pi）：**

| 状态 | 背景色 | 说明 |
|------|--------|------|
| pending | 淡灰色 | 等待执行 |
| running | 淡青色 | 执行中 |
| success | 淡绿色 | 执行成功 |
| error | 淡红色 | 执行失败 |
| awaiting_confirm | 淡黄色 | 等待用户确认 |

**每个工具的自定义渲染器：**

| 工具 | 折叠摘要 (renderCall) | 展开内容 (renderResult) | Pi 对应 |
|------|---------|---------|---------|
| read | `读取了 N 行代码` | 完整文件内容（带行号和语法高亮） | — |
| write | `写入 N 字节到 file` | 写入内容预览 | — |
| edit | 内联 diff 摘要 | 完整 diff + 上下文 | — |
| grep | `找到 N 处匹配` | 匹配行列表（高亮匹配部分） | — |
| bash | `退出码 N，耗时 Ns` | 完整 stdout/stderr | BashExecutionComponent |
| find | `找到 N 个文件` | 文件路径列表 | — |
| ls | `N 个条目` | 目录列表 | — |
| github | `Issue #42 已更新` | Issue 详情和操作记录 | — |

**Bash 执行独立样式：**

参照 Pi 的 BashExecutionComponent，Bash 命令使用独立的视觉样式（`$` 前缀而非 🛠），以区分普通工具调用和终端命令执行：

```
 ┌─ $ bash cargo test ── ✅ 3.2s ──────────────────┐
 │  running 4 tests                                  │
 │  test auth::tests::test_login ... ok              │
 │  test result: ok. 4 passed; 0 failed; 0 ignored   │
 └──────────────────────────────────────────────────┘
```

**Ctrl+O 切换工具输出可见性：**

参照 Pi，`Ctrl+O` 快速切换所有工具调用的展开/折叠状态。这是一个全局开关，方便用户在"只看对话"和"查看所有细节"之间快速切换。

### 3.6 内联 Diff

编辑操作（write/edit）的工具调用方框内，默认以 diff 形式展示：

```
 ┌─ 🛠 edit src/auth.rs ──── ⚠️ 等待确认 ────────────┐
 │  @@ -12,3 +12,8 @@                                   │
 │   fn login_handler() {                               │
 │  -    todo!()                                        │
 │  +    let creds: LoginRequest = req.json()?;         │
 │  +    let user = verify_credentials(&creds)?;        │
 │  +    let token = generate_jwt(&user)?;              │
 │  +    HttpResponse::Ok().json(token)                 │
 │   }                                                  │
 │                                                      │
 │  [Y] 确认  [N] 拒绝  [E] 在编辑器中打开              │
 └────────────────────────────────────────────────────┘
```

- 删除行：红色背景
- 新增行：绿色背景
- 上下文行：默认颜色

### 3.7 权限请求

涉及文件写入或命令执行的工具调用需要用户确认：

| 操作类型 | 默认权限 | 确认选项 |
|---------|---------|---------|
| read / grep / find / ls | 自动允许 | 无需确认 |
| edit / write | 需确认 | [Y] 允许 [N] 拒绝 [E] 在编辑器中编辑 |
| bash | 需确认 | [Y] 允许 [N] 拒绝 |
| bash（只读命令如 ls、cat） | 自动允许 | 无需确认 |

只读命令白名单：`ls`, `cat`, `head`, `tail`, `find`, `grep`, `git status`, `git log`, `git diff`, `cargo check`, `cargo test`, `cargo build`。

权限系统的粒度控制通过 `/permissions` 命令管理。

### 3.8 滚动行为

- Agent 正在输出时：自动滚动到底部
- 用户手动上滚时：暂停自动滚动，显示"有新消息 ↓"提示
- 用户滚到底部时：恢复自动滚动
- `Ctrl+X` → `g`：跳转到底部
- `Ctrl+X` → `G`：跳转到顶部
- `PageUp/PageDown`：翻页
- `↑/↓`（对话区焦点时）：逐行滚动
- 鼠标滚轮：支持（终端兼容时）

### 3.9 消息队列系统

参照 Pi 的核心设计，Agent 工作时用户可以继续输入，消息被排队等待处理。这是保持交互流畅性的关键机制：

**两种排队策略：**

| 类型 | 投递时机 | 图标 | 说明 |
|------|---------|------|------|
| steering（引导消息） | 当前工具调用完成后立即投递 | 🔄 | 在工具间隙插入，用于修正 Agent 方向 |
| follow-up（后续消息） | Agent 完成全部工作后投递 | 📋 | 在 Agent 闲下来后处理，用于追加需求 |

**交互方式：**

1. Agent 工作时，输入栏仍可输入
2. 用户输入消息后按 `Enter` 发送到队列
3. 对话区底部显示排队消息预览（暗色样式）
4. `Alt+↑` 可查看和检索已排队的消息

```
 ┌─────────────────────────────────────────────────┐
 │  [🔄 排队中] 下一个函数用错误处理模式              │  ← steering 消息
 │  [📋 排队中] 完成后写单元测试                      │  ← follow-up 消息
 └─────────────────────────────────────────────────┘
```

**实现要点：**

- AgentLoop 维护一个消息队列，区分 steering 和 follow-up 类型
- steering 消息在 `ToolCallEnd` 事件后、下一个 `ToolCallStart` 之前投递
- follow-up 消息在 `TurnEnd` 事件后投递
- TUI 通过 `AgentEvent::MessageQueued` 和 `AgentEvent::MessageDelivered` 事件追踪队列状态

### 3.10 压缩摘要消息

参照 Pi 的 CompactionSummaryMessageComponent，当上下文压缩发生时，对话区插入一条特殊消息替代被压缩的历史：

```
 ┌─────────────────────────────────────────────────┐
 │  📝 上下文压缩                                    │
 │  8 条消息被替换为摘要：                            │
 │  用户请求实现登录功能，Agent 读取了 auth.rs，       │
 │  实现了 JWT 认证和密码验证，测试全部通过。          │
 │  压缩前 Token: 12k → 压缩后: 2k                   │
 └─────────────────────────────────────────────────┘
```

### 3.11 分支摘要消息

参照 Pi 的 BranchSummaryMessageComponent，当用户通过 `/branch` 或会话导航创建分支时：

```
 ┌─────────────────────────────────────────────────┐
 │  🔀 对话分支                                      │
 │  从 abc3f01 分支到 def7a02                       │
 │  分支点：用户请求改为 OAuth2 方案                  │
 └─────────────────────────────────────────────────┘
```

---

## 四、输入栏详细设计

### 4.1 基本编辑

| 操作 | 按键 |
|------|------|
| 发送消息 | `Enter` |
| 换行（多行输入） | `Alt+Enter` 或 `Ctrl+Enter` |
| 删除 | `Backspace` / `Delete` |
| 移动光标 | `←/→/Home/End` |
| 跳转到行首/行尾 | `Ctrl+A` / `Ctrl+E` |
| 删除到行尾 | `Ctrl+K` |
| 删除到行首 | `Ctrl+U` |
| 删除前一个词 | `Ctrl+W` |
| 历史浏览 | `↑/↓` |
| Tab 补全 | `Tab` |
| 取消输入 | `Esc` |

### 4.2 特殊输入语法

| 语法 | 含义 | 示例 | Pi 参考 |
|------|------|------|---------|
| `@file` | 引用文件，Agent 自动读取 | `分析 @src/auth.rs 的安全漏洞` | — |
| `@dir/` | 引用目录 | `重构 @src/auth/ 下的模块结构` | — |
| `!command` | 执行 shell 命令，Agent 观察输出 | `!cargo test` | `!` bash with LLM |
| `!!command` | 直接执行 shell 命令，不经过 Agent | `!!git status` | `!!` bash without LLM |
| `/command` | 斜杠命令 | `/model deepseek-v3` | — |

### 4.3 @ 文件引用

输入 `@` 后触发文件路径补全：

```
 > 分析 @src/█
   src/auth.rs
   src/main.rs
   src/config.rs
   src/models/
```

选择文件后，文件路径高亮显示，Agent 在处理时会自动读取引用的文件内容。

### 4.4 ! Bash 快捷执行

参照 Pi 的 bash 执行设计，提供两种 bash 模式：

**`!command` — Bash with Agent（默认）**

命令执行结果会被 Agent 看到和分析，适合需要 Agent 根据输出继续工作的场景：

```
 > !cargo test

 ┌─ $ bash cargo test ──── ✅ 3.2s ────────────┐
 │  test result: ok. 4 passed; 0 failed           │
 └────────────────────────────────────────────────┘
```

**`!!command` — Bash without Agent**

命令直接执行，输出只显示在对话中，Agent 不参与。适合快速查看状态：

```
 > !!git status

 ┌─ $ git status ──── ✅ 0.1s ─────────────────┐
 │  On branch feat/73-docs                         │
 │  nothing to commit, working tree clean           │
 └────────────────────────────────────────────────┘
```

### 4.5 Tab 补全

Tab 键根据当前输入上下文提供不同补全：

| 上下文 | 补全内容 |
|--------|---------|
| `/` 开头 | 斜杠命令名 |
| `@` 后 | 文件/目录路径 |
| 普通文本 | 无补全 |

### 4.6 输入历史

- 保存最近 100 条输入历史
- `↑/↓` 浏览历史
- 历史按会话独立存储
- 空输入时 `↑` 显示上一条历史

---

## 五、斜杠命令

### 5.1 命令列表

| 命令 | 功能 | 说明 | Pi 参考 |
|------|------|------|---------|
| `/help` | 显示帮助 | 列出所有可用命令和快捷键 | — |
| `/quit` `/exit` | 退出 TUI | 保存会话后退出 | — |
| `/model [name]` | 查看/切换模型 | 无参数显示当前模型和可用列表，有参数切换 | Ctrl+L |
| `/session [id]` | 会话管理 | 无参数显示会话列表，有参数切换到指定会话 | — |
| `/tree` | 会话树导航 | 展示当前会话的分支树结构，支持导航 | /tree |
| `/new` | 新建会话 | 开始新的对话会话 | — |
| `/compact` | 压缩上下文 | 手动触发上下文压缩 | — |
| `/undo` | 撤销上次变更 | 通过 git 还原 Agent 最近一次文件修改 | — |
| `/redo` | 重做撤销的变更 | 恢复被 /undo 还原的修改 | — |
| `/thinking [level]` | 思考级别 | 无参数显示当前级别，有参数设置级别（off/minimal/low/medium/high/xhigh） | Shift+Tab |
| `/details` | 切换工具详情 | 开关工具调用的默认展开状态 | Ctrl+O |
| `/permissions` | 权限管理 | 查看和修改工具执行权限设置 | — |
| `/editor` | 外部编辑器 | 用 $EDITOR 打开多行编辑器 | — |
| `/export` | 导出对话 | 将当前会话导出为 Markdown 文件 | — |
| `/theme [name]` | 主题切换 | 切换颜色主题（默认/暗色/亮色） | — |
| `/issues [pull]` | Issue 管理 | 拉取 GitHub Issues 列表 | — |
| `/config` | 配置管理 | 查看当前配置 | — |

### 5.2 命令交互

斜杠命令输入后，部分命令（如 `/model`、`/session`）会弹出选择列表：

```
 > /model
 ┌────────────────────────────┐
 │  ○ glm-5.1                 │
 │  ● deepseek-v3  ← 当前     │
 │  ○ deepseek-v4-pro         │
 │  ○ ollama (codellama)      │
 └────────────────────────────┘
```

用 `↑/↓` 选择，`Enter` 确认，`Esc` 取消。

---

## 六、快捷键体系

### 6.1 Leader Key 模式

采用 `Ctrl+X` 作为 Leader Key，先按 `Ctrl+X`，再按对应键：

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+X` → `g` | 跳转到对话底部 |
| `Ctrl+X` → `G` | 跳转到对话顶部 |
| `Ctrl+X` → `c` | 切换焦点到输入栏 |
| `Ctrl+X` → `d` | 切换工具详情展开 |
| `Ctrl+X` → `t` | 切换思考过程显示 |
| `Ctrl+X` → `n` | 新建会话 |
| `Ctrl+X` → `s` | 切换会话 |
| `Ctrl+X` → `m` | 切换模型 |

### 6.2 直接快捷键

| 快捷键 | 功能 | Pi 参考 |
|--------|------|---------|
| `Enter` | 发送消息（输入栏）/ 确认权限请求（对话区） | — |
| `Esc` | 取消当前操作 / 退出选择列表 | — |
| `Esc` × 2 | 打开会话树导航（`/tree`） | Escape×2 opens /tree |
| `↑/↓` | 历史浏览（输入栏）/ 滚动（对话区焦点时） | — |
| `PageUp/PageDown` | 对话区翻页 | — |
| `Tab` | 补全 | — |
| `Shift+Tab` | 循环切换思考级别（off → minimal → low → medium → high → xhigh） | Shift+Tab cycles thinking level |
| `Ctrl+C` | 中断 Agent 执行，退出 TUI（连按两次） | Ctrl+C×2 |
| `Ctrl+O` | 切换工具输出展开/折叠（全局开关） | Ctrl+O toggle tool output |
| `Ctrl+T` | 切换思考过程显示 | Ctrl+T toggle thinking |
| `Ctrl+L` | 打开模型选择器 | Ctrl+L model selector |
| `Alt+Enter` | 输入栏换行 | — |
| `Alt+↑` | 检索已排队的消息 | Alt+Up retrieve queued messages |

### 6.3 权限确认快捷键

当对话区显示权限请求时：

| 快捷键 | 功能 |
|--------|------|
| `y` / `Enter` | 允许执行 |
| `n` / `Esc` | 拒绝执行 |
| `e` | 在外部编辑器中打开 |

---

## 七、事件流与对话区映射

Agent 事件流驱动对话区更新：

```
AgentEvent::SessionStart     → 状态栏更新
AgentEvent::ContentDelta     → Agent 文本流式追加（或思考过程追加）
AgentEvent::ToolCallStart    → 对话区插入新的工具调用方框
AgentEvent::ToolCallProgress → 工具调用方框内更新进度
AgentEvent::ToolCallEnd      → 工具调用方框更新状态图标和耗时
AgentEvent::TaskUpdate       → 状态栏显示当前任务（可选：对话区插入任务标签）
AgentEvent::PhaseSummary     → 对话区插入阶段总结卡片
AgentEvent::TurnEnd          → 页脚更新 Token 用量和费用
AgentEvent::Error            → 对话区插入错误消息卡片
AgentEvent::SessionEnd       → 状态栏更新为结束状态
MessageQueued                → 对话区底部显示排队消息预览（Pi 衍生事件）
MessageDelivered             → 排队消息从预览变为正式消息（Pi 衍生事件）
CompactionComplete           → 对话区插入压缩摘要消息（Pi 衍生事件）
BranchCreated                → 对话区插入分支摘要消息（Pi 衍生事件）
```

### 事件到对话消息的映射

| AgentEvent | 对话消息类型 | 渲染策略 |
|------------|-------------|---------|
| `ContentDelta { Text }` | Agent 文本 | Markdown 渲染，流式追加 |
| `ContentDelta { Thinking }` | Agent 思考 | 折叠块，灰色文字 |
| `ToolCallStart` | 工具调用开始 | 插入方框，spinner 图标 |
| `ToolCallProgress` | 工具进度更新 | 方框内显示进度 |
| `ToolCallEnd { Success }` | 工具调用完成 | 方框更新为 ✅ + 耗时 |
| `ToolCallEnd { Failed }` | 工具调用失败 | 方框更新为 ❌ + 错误摘要 |
| `Error` | 错误消息 | 红色卡片 |
| `PhaseSummary` | 阶段总结 | 蓝色信息卡片 |

---

## 八、会话管理

### 8.1 多会话支持

用户可以创建、切换、恢复多个会话：

```
 > /session
 ┌──────────────────────────────────────────┐
 │  ○ abc3f… │ 登录功能实现    │ 23 min 前   │
 │  ● def7a… │ Bug 修复         │ 2 h 前      │  ← 当前
 │  ○ 1234b… │ 重构认证模块     │ 昨天         │
 └──────────────────────────────────────────┘
```

### 8.2 会话持久化

- 每个会话对应一个 JSONL 文件（按 SESSION_SCHEMA.md 规范）
- 切换会话时保存当前对话状态
- 恢复会话时从 JSONL 文件重建对话历史
- 会话元数据包含：创建时间、最后活跃时间、消息数量、Token 用量

### 8.3 会话列表

`/session` 命令显示所有会话，按最后活跃时间排序。每条显示：
- 会话 ID（前 8 位）
- 用户第一条消息摘要（截断到 30 字符）
- 最后活跃时间

### 8.4 会话树导航（Pi 对齐）

参照 Pi 的会话树结构，JSONL 中的每条消息带有 `id` 和 `parentId`，支持原地分支：

**树结构：**

```
msg-1 (root)
├── msg-2 (用户：实现登录)
│   ├── msg-3 (Agent：读取文件)
│   │   └── msg-4 (Agent：编辑代码)
│   └── msg-5 (用户：改为 OAuth2) ← 分支点
│       └── msg-6 (Agent：重构为 OAuth2)
└── msg-7 (用户：先写测试) ← 另一个分支
    └── msg-8 (Agent：编写测试)
```

**`/tree` 命令：**

```
 > /tree
 ┌──────────────────────────────────────────┐
 │  ● msg-4 ← 当前位置                      │
 │  ├── msg-5 "改为 OAuth2" (分支)           │
 │  └── msg-7 "先写测试" (分支)              │
 │  ↑↓ 选择分支 │ Enter 跳转 │ Esc 返回     │
 └──────────────────────────────────────────┘
```

- `Esc` × 2 也可打开 `/tree` 视图
- 选择分支后，对话区从该分支点重新展示后续消息
- 分支操作不删除原有消息，只是切换活跃分支

---

## 九、模式切换

### 9.1 计划模式 vs 执行模式

| 模式 | 行为 | 适用场景 |
|------|------|---------|
| 计划模式（Plan） | Agent 只分析、规划，不执行写入/命令 | 复杂需求，先审查方案 |
| 执行模式（Build） | Agent 直接执行，包括文件修改和命令运行 | 简单需求，信任 Agent |

通过 `/plan` 和 `/build` 命令切换，状态栏显示当前模式。

计划模式下，Agent 的工具调用只生成预览，用户确认后切换到执行模式才会真正执行。

---

## 十、思考级别系统（Pi 对齐）

参照 Pi 的 thinking levels 设计，提供 6 级思考深度控制。思考级别影响 Agent 在回答前的思考深度，同时通过输入栏边框颜色提供视觉反馈。

### 10.1 级别定义

| 级别 | 图标 | 说明 | 输入栏边框色 |
|------|------|------|-------------|
| off | ○ | 不启用思考 | 默认色（白色） |
| minimal | ◔ | 最少思考，快速回答 | 淡灰色 |
| low | ◑ | 低度思考 | 淡蓝色 |
| medium | ◕ | 中度思考（默认） | 蓝色 |
| high | ● | 深度思考 | 紫色 |
| xhigh | ⬤ | 最深度思考 | 品红色 |

### 10.2 交互方式

- `Shift+Tab`：循环切换级别（off → minimal → low → medium → high → xhigh → off）
- `/thinking [level]`：直接设置级别
- `/thinking`（无参数）：显示当前级别和说明
- 页脚第 2 行的思考级别图标实时反映当前级别

### 10.3 对 LLM 请求的影响

思考级别映射到 LLM 请求参数：

| 级别 | 效果 |
|------|------|
| off | 不请求 thinking token，直接回答 |
| minimal | 分配少量 thinking token（~500） |
| low | 分配 thinking token（~1k） |
| medium | 分配 thinking token（~4k）（默认） |
| high | 分配大量 thinking token（~10k） |
| xhigh | 分配最大 thinking token（模型上限） |

---

## 十一、主题系统

参照 Pi 的 ~50 命名色主题系统，提供结构化的颜色配置。主题文件为 JSON 格式，支持热重载。

### 11.1 内置主题

| 主题 | 说明 |
|------|------|
| `default` | 暗色终端默认 |
| `light` | 亮色终端适配 |
| `monokai` | Monokai 配色 |
| `solarized` | Solarized Dark |

### 11.2 颜色分类

主题定义覆盖以下颜色分组（参照 Pi 的颜色体系）：

**核心 UI：**
- 用户消息、Agent 文本、输入栏、页脚背景、页脚文字

**对话元素：**
- 工具调用方框（按状态分类：pending/running/success/error/awaiting_confirm）
- 思考过程文字和折叠块
- 权限请求边框
- 错误消息背景
- 压缩摘要卡片、分支摘要卡片

**Diff 显示：**
- 新增行背景、新增行文字
- 删除行背景、删除行文字
- 上下文行、hunk 标题

**Markdown 渲染：**
- 标题、粗体、斜体、链接、行内代码背景
- 代码块边框、代码块背景

**语法高亮：**
- 关键字、字符串、注释、数字、类型、函数名

**思考级别边框：**
- 6 级别对应的输入栏边框色

**Bash 模式：**
- Bash 命令前缀色、stdout 色、stderr 色

### 11.3 主题文件格式

主题文件位于 `~/.config/uncode/themes/{name}.json`，支持用户自定义：

```json
{
  "name": "custom-dark",
  "ui": {
    "user_message": "#ffffff",
    "agent_text": "#e0e0e0",
    "input_border": "#4a9eff",
    "footer_bg": "#1a1a2e",
    "footer_text": "#888888"
  },
  "tool_status": {
    "pending": "#444444",
    "running": "#0088aa",
    "success": "#00aa44",
    "error": "#aa0000",
    "awaiting_confirm": "#aa8800"
  },
  "diff": {
    "added_bg": "#1a3a1a",
    "added_text": "#44dd44",
    "removed_bg": "#3a1a1a",
    "removed_text": "#dd4444"
  },
  "thinking_level_border": {
    "off": "#ffffff",
    "minimal": "#666666",
    "low": "#4488cc",
    "medium": "#4466dd",
    "high": "#8844dd",
    "xhigh": "#cc44aa"
  }
}
```

`/theme` 命令切换主题，修改主题文件后自动热重载。

---

## 十二、技术实现

### 12.1 框架选型

| 组件 | 技术 | 说明 | Pi 参考 |
|------|------|------|---------|
| 终端渲染 | ratatui + crossterm | 保持现有选型 | Pi 用手动 ANSI diff 渲染 |
| 语法高亮 | tree-sitter | 保持现有选型 | — |
| Markdown 渲染 | pulldown-cmark | 保持现有选型 | — |
| 异步事件 | tokio::sync::broadcast | 保持现有选型 | — |
| 输入处理 | crossterm event | 增加多行、@ 引用、Leader Key | — |

### 12.2 渲染策略

**ratatui 全量重绘 vs Pi 的差分渲染：**

Pi 使用手动 ANSI escape 序列实现差分渲染（只输出变化的部分），配合 `CSI 2026` 同步输出和 16ms 节流（约 60fps）。uncode 使用 ratatui 的全量重绘模式（每帧重绘整个界面），开发复杂度更低，性能在大多数终端下足够。

对于极端性能场景（超长对话历史、大量工具输出），采用以下优化策略：

- **虚拟滚动**：对话区只渲染可见区域的消息，支持大量历史消息
- **增量渲染**：流式输出时只追加新增内容
- **折叠渲染**：工具调用和思考过程默认折叠，减少渲染量
- **Markdown 预渲染**：每条消息完成后缓存渲染结果，滚动时直接使用缓存
- **渲染节流**：对话区更新频率控制在 ~60fps（16ms），避免快速事件导致的过度渲染

### 12.3 数据结构

对话区维护一个消息列表：

```rust
enum ChatMessage {
    User { text: String, file_refs: Vec<String> },
    Assistant { text: String, rendered_cache: Vec<Line> },
    Thinking { text: String, expanded: bool },
    ToolCall {
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
        status: ToolCallStatus,
        duration_ms: Option<u64>,
        result: Option<String>,
        expanded: bool,
    },
    BashExecution {
        command: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration_ms: Option<u64>,
        with_agent: bool,  // !command vs !!command
    },
    Error { message: String, category: ErrorCategory },
    Summary { completed: Vec<String>, next_steps: Vec<String> },
    CompactionSummary {
        messages_replaced: usize,
        tokens_before: u64,
        tokens_after: u64,
        summary_text: String,
    },
    BranchSummary {
        from_id: String,
        to_id: String,
        branch_reason: String,
    },
    QueuedMessage {
        text: String,
        queue_type: QueueType,  // Steering or FollowUp
    },
}

enum QueueType {
    Steering,  // 在当前工具调用完成后投递
    FollowUp,  // 在 Agent 完成全部工作后投递
}
```

### 12.4 思考级别实现

思考级别通过 `ThinkingLevel` 枚举表示，影响 LLM 请求的 thinking token 分配：

```rust
enum ThinkingLevel {
    Off,
    Minimal,   // ~500 tokens
    Low,       // ~1k tokens
    Medium,    // ~4k tokens (默认)
    High,      // ~10k tokens
    XHigh,     // 模型上限
}
```

输入栏的边框颜色通过主题文件中的 `thinking_level_border` 映射动态设置。
```

### 11.4 与 Agent 的交互

TUI 通过事件流与 AgentLoop 交互：

1. 用户输入 → `on_submit(text)` → 创建新的 `AgentLoop` 实例 → 发送消息
2. Agent 广播事件 → TUI 接收事件 → 更新对话区消息列表 → 重新渲染
3. 权限请求：TUI 暂停 Agent 的工具执行，等待用户确认后继续

---

## 十三、与 v1 的迁移

### 13.1 保留的模块

以下模块从 v1 迁移，无需重写：
- `input.rs` — InputEditor（需增加多行支持）
- `markdown.rs` — Markdown 渲染
- `highlight.rs` — 语法高亮
- `diff_viewer.rs` — Diff 渲染（改为内联使用）
- `complete.rs` — Tab 补全（增加 @ 路径补全）
- `slash.rs` — 斜杠命令（增加新命令）

### 13.2 重写的模块

- `lib.rs` — TuiEngine 从四面板改为对话驱动布局 + 页脚
- `code_detail.rs` — 不再作为独立覆盖层，改为工具调用的展开内容

### 13.3 新增的模块

- `chat.rs` — 对话消息数据结构（ChatMessage 枚举）和渲染
- `permission.rs` — 权限确认系统
- `theme.rs` — 主题系统（~50 命名色，JSON 配置，热重载）
- `session_ui.rs` — 会话列表选择界面 + 会话树导航
- `message_queue.rs` — 消息队列系统（steering + follow-up）
- `tool_renderer.rs` — 每个工具的自定义渲染器（renderCall/renderResult）
- `footer.rs` — 页脚组件（位置上下文 + Token/费用/模型信息）

---

## 十四、未定义项（留待实现时决策）

以下设计细节属于实现阶段微调，不在本文档中锁定：

- 精确的颜色值和主题配置格式（但结构和分类已确定）
- 对话消息的最大缓存数量
- @ 文件引用的精确补全算法
- 权限系统的持久化方式
- 导出 Markdown 的文件名规则
- 消息队列的最大排队数量
- 思考级别 token 分配的精确值（模型相关）
