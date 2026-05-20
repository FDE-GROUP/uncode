# uncode TUI 架构

> 虚拟滚动 + 增量渲染 + syntect 高亮 + Markdown + 9 工具渲染器 | 基于源码分析，2026-05 修订

uncode 的 TUI 基于 ratatui + crossterm 构建，是一个全屏终端 UI。核心挑战是实时渲染流式 LLM 输出（Thinking + 文本 + 工具调用），同时保持响应性。

**微观规划 UX**（用户能否感知多 Turn「想→做」链、Turn 边界与 `agent_busy` 语义）：见 [`UNCODE_TUI_MICRO_PLANNING_UX.md`](UNCODE_TUI_MICRO_PLANNING_UX.md)。机制定义见 [`UNCODE_MICRO_PLANNING.md`](UNCODE_MICRO_PLANNING.md)。

---

## 模块结构

| 模块 | 职责 |
|------|------|
| `lib.rs` | `TuiEngine` 主状态机：事件循环 + 渲染 + 快捷键 + ESC 处理 |
| `chat.rs` | `ChatState` 消息列表 + 虚拟滚动缓存 + Thinking 渲染 |
| `highlight.rs` | syntect 语法高亮引擎 |
| `markdown.rs` | GFM Markdown → ratatui Lines 渲染器 |
| `input.rs` | `InputEditor` 输入框（历史/撤销/补全/UTF-8 光标） |
| `theme.rs` | `Theme` 50+ 命名颜色 + 4 内置主题 + JSON 自定义 |
| `tool_renderer.rs` | `ToolRendererRegistry` 9 类工具自定义渲染器 + 语法高亮 |
| `welcome.rs` | `WelcomeScreen` 欢迎覆盖层 |
| `selector.rs` | `OverlaySelector` 模型选择弹窗 |
| `complete.rs` | `CompletionEngine` 斜杠命令 + 文件路径补全 |
| `slash.rs` | `SlashCommands` 可扩展斜杠命令注册表 |
| `message_queue.rs` | `MessageQueue` followUp/Steering 队列 |
| `permission.rs` | `PermissionManager` 工具权限管理 |
| `diff_viewer.rs` | `DiffViewer` 统一 diff 覆盖层 |

---

## 事件循环

```rust
// TuiEngine::run()
tokio::select! {
    biased;
    ui_result = /* crossterm 事件轮询 50ms */ => { /* 键盘/鼠标处理 */ }
    Ok(event) = event_rx.recv() => {
        self.handle_event(event);
        if matches!(event, TurnEnd | SessionEnd | AgentInterrupted) {
            self.flush_queue(&on_submit);
        }
    }
}
```

`biased` 确保 UI 事件优先处理。Agent 事件到达时更新 `ChatState`，turn 结束时 flush 消息队列。

---

## ChatState — 虚拟滚动

### 数据结构

```rust
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    line_counts: Vec<LineCountEntry>,    // 每条消息缓存行数 + 渲染输出
    prefix_sum: Vec<usize>,              // 前缀和（二分查找用）
    prefix_dirty: bool,
    cached_width: u16,
}
```

### 渲染流程

```
① ensure_line_counts()
    ├── 遍历所有消息
    ├── 检查缓存是否过期（宽度变化 或 文本增长）
    ├── 仅重新渲染过期消息
    └── 最后一条流式消息使用 render_incremental()

② render_incremental()
    ├── 保留已缓存的 prefix 行
    └── 仅重新渲染增长的 tail 部分

③ visible_range()
    ├── 使用 prefix_sum 二分查找（partition_point）
    └── O(log n) 找到 [first, last] 可见消息索引

④ render_viewport()
    ├── 仅渲染 visible 范围内的消息
    ├── 跳过 scroll_offset 以上的行
    └── 组装最终 Lines
```

### ChatMessage 枚举

```rust
pub enum ChatMessage {
    User { text, file_refs },
    Assistant { text },
    Thinking { text, expanded, active },
    ToolCall { tool_id, tool_name, arguments_summary, status, duration_ms, result, expanded },
    BashExecution { tool_id, command, exit_code, stdout, stderr, duration_ms, with_agent },
    Error { message, category },
    Summary { completed, next_steps },
    CompactionSummary { messages_replaced, tokens_before, tokens_after, summary_text },
    QueuedMessage { text },
}
```

---

## Markdown 渲染

`markdown.rs` 使用 `markdown` crate 的 `to_mdast()` 解析 GFM AST：

| 元素 | 渲染方式 |
|------|----------|
| 段落 | 自动换行（`push_wrapped`） |
| 标题 H1 | 粗体 + 闪烁 |
| 引用块 | `▎` 前缀 |
| 有序列表 | 数字计数器 + 缩进 |
| 任务列表 | `☑` / `☐` |
| 代码块 | 逐行语法高亮 |
| 表格 | `│` 边框 + `┼` 分隔 |
| 行内代码 | 反引号包裹 |
| 链接 | `[text](url)` 格式 |

**长输出截断**：保留前 50 行 + 后 50 行，中间 `... (N lines omitted) ...`。

---

## 语法高亮

使用 **syntect**（非 tree-sitter）。三个静态全局：

```rust
static SYNTAX_SET: LazyLock<SyntaxSet>;      // 语法定义
static THEME_SET: LazyLock<ThemeSet>;         // 颜色主题
static THEME_PREFERENCES: &[&str] = &[
    "base16-eighties.dark",    // 首选
    "Solarized (dark)",
    "base16-ocean.dark",
    "InspiredGitHub",
];
```

主题选择优先级：
1. 用户配置的 `theme.syntax_theme_name`
2. 偏好链依次尝试
3. 第一个可用主题

`highlight_line_with_theme()` 映射 syntect 样式到 ratatui `Style`（RGB 前景色 + BOLD/ITALIC/UNDERLINED）。失败时回退到单色 `theme.markdown.code_text`。

---

## 输入编辑器

`InputEditor` 是完整的 readline 实现：

| 功能 | 快捷键 |
|------|--------|
| 提交 | Enter |
| 多行 | Shift+Enter |
| 历史 | ↑ / ↓ |
| 撤销/重做 | Ctrl+Z / Ctrl+Y |
| 单词跳转 | Alt+← / Alt+→ |
| 删词 | Ctrl+W（前）/ Alt+D（后） |
| 行首/行尾 | Ctrl+A / Ctrl+E |
| 补全 | Tab（循环） |
| 外部编辑器 | Ctrl+G（$EDITOR） |

Unicode 宽度感知的光标定位。

---

## 工具渲染器

`ToolRendererRegistry` 使用静态分发（`ToolKind` 枚举），零分配：

```rust
pub enum ToolKind { Read, Write, Edit, Grep, Bash, Find, Ls }

impl ToolRendererRegistry {
    pub fn render_call(&self, kind: ToolKind, args: &Value) -> Vec<Line<'static>> { ... }
    pub fn render_result(&self, kind: ToolKind, result: &str) -> Vec<Line<'static>> { ... }
}
```

- `ReadRenderer`：显示文件路径 + 行范围
- `EditRenderer`：显示 diff 颜色（added=绿，removed=红，header=青）
- `BashRenderer`：显示命令 + exit code + 输出行

---

## 布局

```
┌──────────────────────────────────┐
│                                  │
│          Chat Area               │  Min(0) — 自适应
│        (virtual scroll)          │
│                                  │
├──────────────────────────────────┤
│  ╭─ input ───────────────────╮   │  Length(3)
│  ╰───────────────────────────╯   │
├──────────────────────────────────┤
│ status │ workdir │ session │ git │  Length(1)
├──────────────────────────────────┤
│ in/out tokens │ cost │ ctx% │ model │ thinking │  Length(1)
└──────────────────────────────────┘
```

覆盖层按优先级渲染：`welcome` → `selector` → `diff_viewer` → `code_detail`。

---

## 主题系统

4 个内置主题：`default`（暗色）、`light`、`monokai`、`solarized`。

自定义主题从 `~/.uncode/themes/<name>.json` 加载，覆盖到 `default_dark` 上。支持命名颜色、`#RRGGBB`、RGB 数组。

`Theme` 包含 7 个子结构（约 50 个命名颜色）：`UiColors`、`ToolStatusColors`、`DiffColors`、`BashColors`、`MarkdownColors`、`SyntaxColors`，外加 `thinking_level_border: [Color; 6]`。

通过 `/theme <name>` 斜杠命令热切换。

---

## 消息队列

`MessageQueue`（TUI 层）管理用户在 Agent 运行时的输入：

| 队列 | 模式 | 时机 |
|------|------|------|
| `FollowUp` | `OneAtATime` | Turn 结束后弹出一条 |
| `Steering` | `All` | 立即全部注入 |

Agent 忙碌时用户输入排队，显示为 `QueuedMessage`。Turn 结束时自动 flush。

---

## 权限管理

`PermissionManager` 分三级：

| 级别 | 工具 | 行为 |
|------|------|------|
| 自动允许 | read, grep, find, ls | 只读工具无需确认 |
| 需要确认 | edit, write | 每次修改文件需 y/n/e |
| 白名单 | bash | `ls`, `cat`, `git status`, `cargo check/test/build/clippy/fmt` 等安全命令自动通过，其余需确认 |

---

*本文档基于 uncode 源码（`crates/uncode-tui/`）编写。*
