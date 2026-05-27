# uncode TUI 与 opencode "Agent 流程即 UI" 对齐实施技术方案

> **前置文档**：`UNCODE_OPENCODE_TUI_RENDERING_ANALYSIS.md`（opencode 渲染分析）、`UNCODE_OPENCODE_TUI_ALIGNMENT.md`（对齐可行性分析）  
> **相关修复文档**：[`UNCODE_TUI_STREAMING_RENDER_FIX.md`](UNCODE_TUI_STREAMING_RENDER_FIX.md)（Assistant 流式渲染 / 视口滚动 / broadcast 丢事件 — 已修复，2026-05-27）
> **编写日期**：2026-05-26
> **预估总工期**：Phase A (2-3d) + Phase B (1-2d) + Phase C (3-5d) = **6-10 天**

---

## 目录

1. [数据模型变更](#1-数据模型变更)
2. [事件管道变更](#2-事件管道变更)
3. [权限模态弹窗](#3-权限模态弹窗-phase-a1)
4. [工具输出折叠](#4-工具输出折叠-phase-a2)
5. [实时输出流式](#5-实时输出流式-phase-b1)
   - [5.6 Assistant 流式渲染修复（已完成）](#56-assistant-流式渲染修复已完成)
6. [格式统一](#6-格式统一-phase-b2)
7. [测试策略](#7-测试策略)
8. [风险与依赖](#8-风险与依赖)

---

## 1. 数据模型变更

### 1.1 `ToolCallEndEventData` 补充字段

**文件**：`crates/uncode-core/src/event.rs:21-30`

```rust
// 变更前
pub struct ToolCallEndEventData {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub duration_ms: u64,
    pub output_size: Option<usize>,
    pub result_summary: Option<String>,
    pub is_error: bool,
}

// 变更后
pub struct ToolCallEndEventData {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub duration_ms: u64,
    pub output_size: Option<usize>,
    pub result_summary: Option<String>,
    pub is_error: bool,
    // ── 新增字段 ──
    /// Human-readable tool description (e.g. "Installing dependencies", "Reading config file")
    pub title: Option<String>,
    /// Structured metadata from tool execution (e.g. {"bytes_written": 1024, "lines_modified": 3})
    pub metadata: Option<serde_json::Value>,
}
```

**影响范围**：
- `loop_engine.rs:2149-2164` — 构建 ToolCallEndEventData 处需填充新字段
- `chat.rs:738-760` — `apply_tool_end()` 需使用 title 替代硬编码名称
- `event.rs:860-920` — 所有测试用例中 `ToolCallEndEventData` 构造处

### 1.2 `ToolCallAwaitingApproval` 补充原始参数

**文件**：`crates/uncode-core/src/event.rs:106-113`

```rust
// 变更后
ToolCallAwaitingApproval {
    tool_id: String,
    tool_name: String,
    arguments_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_description: Option<String>,
    // ── 新增 ──
    /// Raw parsed tool arguments for permission dialog preview (diff, code, command)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_args: Option<serde_json::Value>,
}
```

**影响范围**：
- `loop_engine.rs:1706-1762` — 发送 `ToolCallAwaitingApproval` 处需携带 `tool_args`
- `lib.rs:2217-2223` — `handle_event` 传递 `tool_args` 给 permission manager
- `permission.rs` — `PendingConfirmation` 类型添加 `tool_args` 字段

### 1.3 `PendingConfirmation` 扩展

**文件**：`crates/uncode-tui/src/permission.rs:16-22`

```rust
// 变更后
pub struct PendingConfirmation {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments_summary: String,
    pub tool_description: Option<String>,
    pub options: Vec<ConfirmOption>,
    // ── 新增 ──
    pub tool_args: Option<serde_json::Value>,
}
```

### 1.4 `ToolGroupEntry::ToolCall` 补充 title 字段

**文件**：`crates/uncode-tui/src/chat.rs:102-108`

```rust
// 变更后
pub enum ToolGroupEntry {
    ToolCall {
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
        status: ToolCallRenderStatus,
        duration_ms: Option<u64>,
        result: Option<String>,
        expanded: bool,
        // ── 新增 ──
        title: Option<String>,
    },
    BashExecution {
        tool_id: String,
        command: String,
        description: String,
        wd: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration_ms: Option<u64>,
        with_agent: bool,
        expanded: bool,
    },
}
```

---

## 2. 事件管道变更

### 2.1 `loop_engine.rs` 填充新字段

**文件**：`crates/uncode-agent/src/loop_engine.rs:2149-2164`

```rust
// 变更后 — 在 execute_prepared_tool_shared 结尾
let content_text = tool_result.text_content();
self.emit(AgentEvent::ToolCallEnd {
    data: Box::new(ToolCallEndEventData {
        tool_id: id.clone(),
        tool_name: name.clone(),
        arguments: String::new(),  // 可选优化：prepared_args.to_string()
        status: if tool_result.is_error { ToolCallStatus::Failed } else { ToolCallStatus::Success },
        duration_ms,
        output_size: Some(content_text.len()),
        result_summary: Some(content_text),
        is_error: tool_result.is_error,
        // 新增
        title: tool_result.details
            .as_ref()
            .and_then(|d| d.get("title"))
            .and_then(|v| v.as_str())
            .map(String::from),
        metadata: if tool_result.details.as_ref().is_some_and(|d| d.as_object().is_some_and(|o| o.len() > 1)) {
            tool_result.details.clone()
        } else {
            None
        },
    }),
});
```

### 2.2 `ToolCallAwaitingApproval` 携带原始参数

**文件**：`crates/uncode-agent/src/loop_engine.rs:1706`（permission gate 之后）

在 `execute_prepared_tool_shared` 被调用前，permission gate 发送 `ToolCallAwaitingApproval` 事件时，携带原始 JSON 参数：

```rust
// 变更位置: loop_engine.rs（permission gate 处）
// 新增 tool_args 字段
AgentEvent::ToolCallAwaitingApproval {
    tool_id: id.clone(),
    tool_name: name.clone(),
    arguments_summary,
    tool_description,
    tool_args: Some(raw_args.clone()),  // 原始 serde_json::Value
}
```

### 2.3 `ToolCallStart` 从 `tool_args` 提取 title

**文件**：`crates/uncode-agent/src/loop_engine.rs:1708`

```rust
// 变更后 — 从原始 JSON 中提取 title 字段（如果存在）
let title = raw_args.get("description").or_else(|| raw_args.get("title"))
    .and_then(|v| v.as_str())
    .map(String::from);

self.emit(AgentEvent::ToolCallStart {
    tool_id: id.clone(),
    tool_name: name.clone(),
    arguments_summary: arguments_summary.clone(),
    // 新增: title 从 args 中提取
});
```

> **注意**：`ToolCallStart` 结构体本身不需要新增字段，`arguments_summary` 已包含足够信息供 `render_call()` 使用。title 信息在 `ToolCallEnd` 阶段才需要（区分成功/失败后的显示文本）。

---

## 3. 权限模态弹窗 {Phase A1}

### 3.1 设计目标

将当前的 status-line 权限确认升级为 **覆盖对话区的模态弹窗**，展示工具执行预览。

```
┌─ ═══════════════════════  Permission Required  ═══════════════════════ ─┐
│                                                                         │
│  ← Write src/main.rs                                                    │
│     File: src/main.rs                                                     │
│                                                                         │
│  ┌─ Preview ───────────────────────────────────────────────────────────┐│
│  │   1 + fn main() {                                                   ││
│  │   2 +     println!("Hello, world!");                                ││
│  │   3 + }                                                             ││
│  │                                                                     ││
│  │                                                                     ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                                                         │
│  [y] Allow    [n] Reject    [a] Always Allow    [e] Edit                │
└─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘
```

### 3.2 文件变更清单

| 文件 | 变更类型 | 行数 |
|:---|:---|:---|
| `permission.rs` | 新增 `render_permission_dialog()` | ~120 |
| `lib.rs` | 修改 `render()` 调用对话框渲染 | ~20 |
| `lib.rs` | 修改 `handle_key()` 键绑定 | ~15 |
| `lib.rs` | 修改 `handle_event()` 传递 tool_args | ~5 |
| `diff_viewer.rs` | 提取公共 diff 渲染函数 | ~30 |

### 3.3 `render_permission_dialog()` 实现

**新增函数**（位于 `permission.rs` 或 `lib.rs`）：

```rust
/// 渲染权限确认模态对话框
///
/// 根据工具类型：
/// - write/edit/apply_patch: 渲染文件内容或 diff 预览
/// - bash: 渲染命令 + 工作目录 + description
/// - 其他: 渲染工具名 + 参数摘要
fn render_permission_dialog(
    f: &mut Frame,
    area: Rect,
    pending: &PendingConfirmation,
    theme: &Theme,
) {
    let dialog_width = area.width.min(80).max(40);
    let dialog_height = area.height.min(24).max(8);
    let dialog_rect = centered_rect(area, dialog_width, dialog_height);

    // 1. 渲染对话框边框
    f.render_widget(
        Block::bordered()
            .border_style(Style::default().fg(theme.tool_status.await_confirm))
            .title(" Permission Required ")
            .title_alignment(Alignment::Center),
        dialog_rect,
    );

    let inner = dialog_rect.inner(&Margin::new(1, 1));
    let mut lines = Vec::new();

    // 2. 工具头部信息（图标 + 名称 + 描述）
    let (icon, label) = match pending.tool_name.as_str() {
        "write" => ("←", "Write"),
        "edit" => ("←", "Edit"),
        "bash" => ("$", "Bash"),
        "apply_patch" => ("%", "Patch"),
        "read" => ("→", "Read"),
        _ => ("⚙", capitalize_first(&pending.tool_name)),
    };

    // 3. 根据工具类型渲染预览内容
    match pending.tool_name.as_str() {
        "write" | "edit" => {
            render_permission_file_preview(&mut lines, pending, theme);
        }
        "bash" => {
            render_permission_bash_preview(&mut lines, pending, theme);
        }
        _ => {
            render_permission_generic(&mut lines, pending, theme);
        }
    }

    // 4. 按钮栏
    let button_line = render_permission_buttons(pending);
    lines.push(button_line);

    // 5. 渲染内容到对话框区域
    let content_height = (inner.height as usize).saturating_sub(1);
    let visible_lines: Vec<_> = lines.into_iter().take(content_height).collect();
    let text = Text::from(visible_lines);
    f.render_widget(Paragraph::new(text), inner);
}
```

### 3.4 各工具类型的预览渲染

#### Write/Edit: 文件内容预览

```rust
fn render_permission_file_preview(
    lines: &mut Vec<Line<'static>>,
    pending: &PendingConfirmation,
    theme: &Theme,
) {
    let file_path = pending.tool_args
        .as_ref()
        .and_then(|a| a.get("filePath").and_then(|v| v.as_str()))
        .unwrap_or("unknown");

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("File: {file_path}"), Style::default().fg(theme.ui.footer_text)),
    ]));
    lines.push(Line::raw(""));

    if let Some(content) = pending.tool_args
        .as_ref()
        .and_then(|a| a.get("content").and_then(|v| v.as_str()))
    {
        // 渲染前 15 行预览
        lines.push(Line::from(Span::styled(
            "  ┌─ Preview ────────────────────────────────",
            Style::default().fg(theme.ui.footer_text),
        )));
        for (i, line) in content.lines().take(15).enumerate() {
            lines.push(Line::from(vec![
                Span::styled(format!("  │{i:>3} "), Style::default().fg(theme.ui.footer_text)),
                Span::styled(format!("+ {line}"), Style::default().fg(theme.diff.added_line)),
            ]));
        }
        if content.lines().count() > 15 {
            lines.push(Line::from(Span::styled(
                format!("  │  ... {} more lines", content.lines().count() - 15),
                Style::default().fg(theme.ui.footer_text),
            )));
        }
        lines.push(Line::from(Span::styled(
            "  └──────────────────────────────────────────",
            Style::default().fg(theme.ui.footer_text),
        )));
    }
}
```

#### Bash: 命令预览

```rust
fn render_permission_bash_preview(
    lines: &mut Vec<Line<'static>>,
    pending: &PendingConfirmation,
    theme: &Theme,
) {
    let command = pending.tool_args
        .as_ref()
        .and_then(|a| a.get("command").and_then(|v| v.as_str()))
        .unwrap_or("");
    let workdir = pending.tool_args
        .as_ref()
        .and_then(|a| a.get("workdir").and_then(|v| v.as_str()))
        .unwrap_or(".");
    let description = pending.tool_args
        .as_ref()
        .and_then(|a| a.get("description").and_then(|v| v.as_str()))
        .unwrap_or("");

    if !description.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("# {description}"), Style::default().fg(theme.bash.command)),
        ]));
    }
    if workdir != "." {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("Working directory: {workdir}"), Style::default().fg(theme.ui.footer_text)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("$ {command}"), Style::default().fg(theme.bash.command).bold()),
    ]));
    lines.push(Line::raw(""));
}
```

#### 通用工具

```rust
fn render_permission_generic(
    lines: &mut Vec<Line<'static>>,
    pending: &PendingConfirmation,
    theme: &Theme,
) {
    if let Some(desc) = &pending.tool_description {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(desc, Style::default().fg(theme.ui.footer_text)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            &pending.arguments_summary,
            Style::default().fg(theme.markdown.text),
        ),
    ]));
}
```

#### 按钮栏

```rust
fn render_permission_buttons(pending: &PendingConfirmation) -> Line<'static> {
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("[y] Allow", Style::default().fg(Color::Green).bold()),
        Span::raw("    "),
        Span::styled("[n] Reject", Style::default().fg(Color::Red).bold()),
        Span::raw("    "),
        Span::styled("[a] Always Allow", Style::default().fg(Color::Yellow).bold()),
    ];
    if pending.options.iter().any(|o| *o == ConfirmOption::Edit) {
        spans.push(Span::raw("    "));
        spans.push(Span::styled("[e] Edit", Style::default().fg(Color::Cyan).bold()));
    }
    Line::from(spans)
}
```

### 3.5 布局集成（lib.rs）

**修改 `render()` 函数**：在渲染 status 后、chat 前检查是否需要权限模态：

```rust
// lib.rs render() 函数中（约 line 634）
fn render(&mut self, f: &mut Frame) {
    // ... 现有布局分割代码 ...

    // 权限模态弹窗覆盖在 chat 区域上方
    if self.permission.has_pending() {
        self.render_permission_dialog(f, chat_area, self.permission.pending().unwrap(), &self.theme);
        self.render_status(f, status_area); // 简化状态行
        self.render_input(f, input_area);
        self.render_footer(f, footer_area);
        return; // 不渲染普通 chat
    }

    // 正常 chat 渲染
    self.render_chat(...);
    // ...
}
```

**修改 `handle_key()` 函数**（已正确绑定 y/n/e，需添加 `a`）：

```rust
// lib.rs:1014-1038 中添加
KeyCode::Char('a') => {
    if let Some(p) = self.permission.confirm(ConfirmOption::Allow) {
        // 永久允许：记录到 policy
        self.permission.add_permanent_allow(&p.tool_name);
        self.resolve_permission(&p.tool_id, Approval::Allow);
    }
}
```

### 3.6 `PermissionManager` 添加持久化允许

```rust
// permission.rs 新增
impl PermissionManager {
    /// Register a permanent allow for a tool
    pub fn add_permanent_allow(&mut self, tool_name: &str) {
        if let Some(ref mut policy) = self.policy {
            // 实现持久化记录...
        }
    }
}
```

> 此功能可选 —— 可先实现会话级 "Always Allow"，持久化留到 Phase C。

---

## 4. 工具输出折叠 {Phase A2}

### 4.1 设计目标

当前行为：工具结果有硬编码 `max_lines` 上限（bash=20, read=80, grep=50），超出直接截断。

目标行为（对齐 opencode）：
- 默认显示前 N 行
- 超出时显示 `… +M lines (Space to expand)`
- 聚焦工具卡片时按 Space 切换展开/折叠

### 4.2 render_result 返回溢出标志

**文件**：`crates/uncode-tui/src/tool_renderer.rs:52-55`

修改 `render_result` 签名（或通过返回值隐式检测）：

```rust
// 方案 A：修改 trait 返回值
pub trait ToolRenderer: Send + Sync {
    fn render_call(&self, args: &str, workdir: &str) -> String;
    fn render_result(
        &self,
        args: &str,
        result: &str,
        width: u16,
        theme: &Theme,
        max_lines: usize,     // 新增：告知渲染器最大行数
    ) -> (Vec<Line<'static>>, usize);  // 新增：返回 (lines, total_count)
}
```

**方案 B（推荐：零 API 变更）**：在 `render_tool_call()` 渲染层截断，而非在各工具 renderer 中：

```rust
// chat.rs:1950 — render_tool_call 中
if expanded && let Some(res) = result {
    let result_lines = renderer.render_result(args, res, width, theme);
    let max_show = 20;  // 可配置
    let total = result_lines.len();

    if total > max_show && !expanded_full {
        // 只显示前 max_show 行
        for rl in result_lines.into_iter().take(max_show) {
            // ... append with ⎿ prefix
        }
        // 追加截断提示
        lines.push(Line::from(vec![
            Span::styled("  \u{23bf} ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(
                format!("+{} more lines (Space to expand)", total - max_show),
                Style::default().fg(theme.ui.footer_text),
            ),
        ]));
    } else {
        // 显示全部
        for rl in result_lines { /* ... */ }
    }
}
```

### 4.3 展开/折叠交互

现有代码 `chat.rs:1073-1074` 已支持 `ToolCall { expanded, .. }` 状态。需确保 Space 键映射正确：

```rust
// lib.rs 键处理（现有 Ctrl+J/K 导航，需添加 Space 切换）
// 在 handle_key 中
KeyCode::Char(' ') => {
    if let Some(idx) = self.chat.focused_card_index {
        self.chat.toggle_card_expand(idx);
    }
}
```

现有的 `toggle_card_expand()` 需确认同时在 `ToolTurnGroup` 和独立 `ToolCall` 上生效。

---

## 5. 实时输出流式 {Phase B1}

### 5.1 设计目标

Bash 工具执行时，stdout 行实时追加到 TUI 显示（类似终端输出），而非等待全部完成后一次性展示。

### 5.2 数据流

```
bash_exec.rs: 逐行读取 stdout
  → ToolProgress::LogLine(line)
    → on_progress 回调
      → AgentEvent::ToolCallProgress { detail: line }
        → loop_engine.rs emit
          → TUI event_rx 接收
            → chat.rs apply_tool_progress() 追加行
              → 下一帧 render 显示
```

### 5.3 bash_exec.rs 改造

**文件**：`crates/uncode-agent/src/tools/bash_exec.rs`

当前 `execute()` 函数：
```rust
// 现状：同步等待子进程完成，收集全部输出后返回
let output = child.wait_with_output()?;
let stdout = String::from_utf8_lossy(&output.stdout).to_string();
```

目标：添加流式进度报告：

```rust
/// 新增：带进度回调的 bash 执行
pub(crate) async fn execute_with_progress(
    args: &BashArgs,
    workdir: &Path,
    mut on_progress: impl FnMut(String),
) -> Result<BashOutput> {
    let mut child = Command::new(&args.command)
        .current_dir(workdir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_buf = BufReader::new(stdout).lines();
    let mut stderr_buf = BufReader::new(stderr).lines();

    let mut stdout_lines = Vec::new();
    let mut stderr_lines = Vec::new();

    // 逐行读取 stdout，实时回调
    loop {
        tokio::select! {
            Ok(Some(line)) = stdout_buf.next_line() => {
                on_progress(line.clone());
                stdout_lines.push(line);
            }
            Ok(Some(line)) = stderr_buf.next_line() => {
                stderr_lines.push(format!("[stderr] {line}"));
            }
            status = child.wait() => {
                let exit_code = status?.code();
                return Ok(BashOutput {
                    stdout: stdout_lines.join("\n"),
                    stderr: stderr_lines.join("\n"),
                    exit_code,
                });
            }
        }
    }
}
```

### 5.4 ToolContext 进度通道

现有 `ToolContext.on_progress` 已存在（`tool.rs:193`），`execute_prepared_tool_shared` 已正确将 `ToolProgress` 转换为 `AgentEvent::ToolCallProgress`（`loop_engine.rs:126-140`）。**无需修改此层。**

关键确认：bash_exec 的 `execute_with_context` 是否使用了 `ctx.on_progress`？

```rust
// bash.rs 当前结构（简化）
async fn execute_with_context(&self, args, ctx) -> Result<ToolResult> {
    let output = bash_exec::execute(&parsed_args, &workdir)?;
    Ok(ToolResult::ok(output.stdout).with_details(...))
}
```

需改为：

```rust
async fn execute_with_context(&self, args, ctx) -> Result<ToolResult> {
    let on_progress = ctx.on_progress.clone();
    let output = if let Some(cb) = on_progress {
        bash_exec::execute_with_progress(&parsed_args, &workdir, move |line| {
            cb(ToolProgress::LogLine(line));
        }).await?
    } else {
        bash_exec::execute(&parsed_args, &workdir)?
    };
    Ok(ToolResult::ok(output.stdout).with_details(...))
}
```

### 5.5 chat.rs 实时追加确认

现有 `apply_tool_progress()`（`chat.rs:1373`）已正确处理 `BashExecution` 的 stdout 追加：

```rust
ToolGroupEntry::BashExecution { command, stdout, .. } => {
    if command.is_empty() {
        let cmd = extract_bash_command(detail);
        if cmd != detail { *command = cmd; }
    }
    stdout.push_str(detail);
    stdout.push('\n');
}
```

且 `render_bash()`（`chat.rs:2060`）显示 `stdout` 内容。**流式渲染已就绪，只需 bash_exec 逐行发送。**

### 5.6 Assistant 流式渲染修复（已完成）

> **详细说明**：见 [`UNCODE_TUI_STREAMING_RENDER_FIX.md`](UNCODE_TUI_STREAMING_RENDER_FIX.md)

Phase B1 主要覆盖 **工具 stdout 流式**；2026-05 另发现并修复一类 **Assistant `ContentDelta` 流式** 问题，与 B1 共用同一事件管道，但对齐方案中未单独成章，故在此交叉引用。

| 症状 | 根因类别 | 修复要点 |
|:---|:---|:---|
| 只见末尾、黄色标题一闪 | 视口 `scroll_offset` + `raw_skip` 裁剪 | `scroll_offset = target.max(msg_start)`，`trailing_assistant_*` |
| 流式假停住、恢复后首尾无中间 | 视口钉在 `msg_start` + 结束时跳底 | 同上；流式期间跟随本条消息底部 |
| 中间文字永久缺失 | `broadcast` `Lagged` 丢弃 `ContentDelta` | 通道 4096、`process_agent_event_batch`、`on_agent_events_lagged` |
| Todo 出现后滚动失效 | `messages.last()` 非 Assistant | `trailing_assistant_index()` |

**已变更文件**（与附录速查表互补）：

- `crates/uncode-tui/src/lib.rs` — `render_chat` 滚动、`process_agent_event_batch`
- `crates/uncode-tui/src/chat.rs` — `trailing_assistant_*`、Assistant 禁用 incremental 渲染
- `crates/uncode-agent/src/loop_engine.rs` — `broadcast::channel(4096)`

**#591–#595 已全部修复**（见 [`UNCODE_TUI_STREAMING_RENDER_FIX.md`](UNCODE_TUI_STREAMING_RENDER_FIX.md) §7）。可选后续：Progress 节流、MessageEnd 全文快照。

---

## 6. 格式统一 {Phase B2}

### 6.1 工具图标统一

| 工具 | 当前 | 目标 (opencode) | 优先级 |
|:---|:---|:---|:---|
| read | `→` | `→` | ✅ 已对齐 |
| write | file path 无图标 | `←` | 需改 |
| edit | 无图标 | `←` | 需改 |
| bash | `# description\n$ cmd` | `$ command` | ✅ 已对齐 |
| grep | `"pattern" in dir` | `✱ pattern in dir` | 需改 |
| find | `"pattern" in path` | `✱ pattern in path` | 需改 |
| ls | `dir/` | `→ dir/` | 需改 |
| web_fetch | `GET url` | `% url` | 需改 |
| web_search | `"query"` | `◈ query` | 需改 |

**修改位置**：各 `ToolRenderer::render_call()` 实现（`tool_renderer.rs`）

### 6.2 Border/缩进风格

当前 `render_tool_call()`（`chat.rs:1930`）输出：

```
  ● Read → src/main.rs
  ⎿  1   fn main() {
  ⎿  2       println!("Hello");
  ⎿ (12ms)
```

目标（对齐 opencode `BlockTool`）—— 对已完成的工具调用添加左侧边框：

```
┃ ● Write ← src/main.rs
┃ ⎿  1 + fn main() {
┃ ⎿  2 +     println!("Hello");
┃ ⎿ (12ms)
```

**实现**：在 `render_tool_call()` 中，当 `status == Success` 且 `result.is_some` 时，为所有行添加左侧边框前缀与面板背景色。

### 6.3 推理摘要提取

当前 `ChatMessage::Thinking` 渲染时（`chat.rs:1670` 左右），标题显示为：
```
● Thinking
▾ Thought · 1.2s
```

可优化为提取首行作为摘要（对齐 opencode 的 `reasoningSummary()`）：

```rust
fn reasoning_summary(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("");
    if first_line.len() <= 60 {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..57])
    }
}
```

---

## 7. 测试策略

### 7.1 单元测试

| 模块 | 测试内容 |
|:---|:---|
| `event.rs` | `ToolCallEndEventData` 新增字段序列化/反序列化 |
| `permission.rs` | `PendingConfirmation` 新字段；`add_permanent_allow` |
| `tool_renderer.rs` | `render_result` 截断逻辑（返回 total_count vs shown） |
| `chat.rs` | `apply_tool_end` 使用 `title` 字段 |

### 7.2 集成测试

```rust
// tests/tui_alignment.rs
#[tokio::test]
async fn test_permission_dialog_renders_for_bash() {
    // 1. 构造 ToolCallAwaitingApproval 事件（带 tool_args）
    // 2. 发送到 TuiEngine
    // 3. 验证 render 输出包含分区边框 + $ command + [y] Allow
}

#[tokio::test]
async fn test_tool_output_collapse_expand() {
    // 1. 创建长输出的 ToolCallEnd
    // 2. 验证仅显示前 N 行 + "... more (Space)"
    // 3. 发送 Space → 验证全部展开
}

#[tokio::test]
async fn test_bash_streaming_progress() {
    // 1. 模拟 bash 逐行输出
    // 2. 验证每行通过 ToolCallProgress 追加到 stdout
}
```

### 7.3 回归测试

运行完整测试套件（`cargo test --workspace -- --test-threads=1`），确保：
- 所有现有 1755 测试通过
- 无 `-D warnings` 编译警告
- `cargo fmt --check` 通过
- `cargo clippy --all-targets --no-deps` 无新增 warning

---

## 8. 实施顺序与阶段计划

### Phase A：核心对齐（2-3 天）

```
A1. 数据模型变更                        Day 1 上午 (2h)
    ├── event.rs: ToolCallEndEventData + title/metadata
    ├── event.rs: ToolCallAwaitingApproval + tool_args
    ├── permission.rs: PendingConfirmation + tool_args
    └── chat.rs: ToolGroupEntry::ToolCall + title

A2. 事件管道适配                        Day 1 下午 (2h)
    ├── loop_engine.rs: ToolCallEnd 填充 title/metadata
    ├── loop_engine.rs: ToolCallAwaitingApproval 携带 tool_args
    └── chat.rs: apply_tool_end 使用 title

A3. 权限模态弹窗                        Day 2 (4h)
    ├── permission.rs: render_permission_dialog()
    ├── lib.rs: render() 集成
    ├── lib.rs: handle_key() 添加 'a' 键
    └── 各工具预览渲染器

A4. 工具输出折叠                        Day 3 上午 (2h)
    ├── chat.rs: render_tool_call 截断逻辑
    └── chat.rs: toggle_card_expand 确认

验证                                   Day 3 下午 (2h)
    └── cargo fmt + clippy + test
```

### Phase B：流式与格式（1-2 天）

```
B1. Bash 实时输出流式                   Day 4 (3h)
    ├── bash_exec.rs: execute_with_progress
    └── bash.rs: use on_progress callback

B2. 格式统一                           Day 4/5 (3h)
    ├── tool_renderer.rs: 图标更新
    ├── chat.rs: border/indentation
    └── chat.rs: reasoning_summary

验证                                   Day 5 下午 (1h)
```

### Phase C：新功能（3-5 天）

```
C1. 子agent session 导航                Day 6-7
    ├── session_channel.rs: 树操作
    └── lib.rs/chat.rs: 切换 UI

C2. 对话时间线                          Day 8-9
    └── 新模块 + 新 UI

C3. 缺失工具实现                        Day 9-10
    ├── tools/task.rs
    ├── tools/question.rs
    └── tools/skill.rs
```

---

## 9. 风险与依赖

| 风险 | 影响 | 缓解措施 |
|:---|:---|:---|
| `event.rs` 字段变更破坏序列化兼容 | 高 | 新字段使用 `#[serde(default, skip_serializing_if = "Option::is_none")]` |
| `chat.rs` 渲染性能退化（模态弹窗重绘） | 低 | 仅在权限激活时渲染模态，不增加常规渲染开销 |
| bash_exec 流式读取引入 async 复杂性 | 中 | 使用 `tokio::select!` 保证兼容现有同步路径 |
| `ToolCallProgress` 刷屏导致 TUI `broadcast::Lagged`、Assistant 中间丢字 | 高 | 已修复：见 [`UNCODE_TUI_STREAMING_RENDER_FIX.md`](UNCODE_TUI_STREAMING_RENDER_FIX.md)（4096 缓冲 + 批处理 + Lagged 提示）；可选 Progress 节流 |
| Phase B 需要 bash_exec 改造可能影响 CI | 中 | 先实现非侵入式 fallback（`on_progress.is_some()` 检测），不影响现有测试 |
| `diff_viewer.rs` 提取公共函数可能引入回归 | 低 | 先改 permission 处的使用，再提取公共函数 |

---

## 附录：关键文件速查

| 文件 | 涉及阶段 | 主要变更 |
|:---|:---|:---|
| `uncode-core/src/event.rs` | A1 | 新增 `title`, `metadata`, `tool_args` 字段 |
| `uncode-core/src/tool.rs` | A1 | 确认 `ToolResult.details` 格式 |
| `uncode-agent/src/loop_engine.rs` | A2, **流式修复** | 填充新字段；`broadcast::channel(4096)` |
| `uncode-agent/src/tools/bash.rs` | B1 | 使用 `on_progress` 回调 |
| `uncode-agent/src/tools/bash_exec.rs` | B1 | `execute_with_progress` |
| `uncode-tui/src/permission.rs` | A1, A3 | 扩展字段 + 新增渲染函数 |
| `uncode-tui/src/lib.rs` | A3, **流式修复** | render/handle_key/handle_event；`render_chat` 滚动、事件批处理 |
| `uncode-tui/src/chat.rs` | A2, A4, B2, **流式修复** | ToolGroupEntry + 截断 + border；`trailing_assistant_*` |
| `docs/technologies/UNCODE_TUI_STREAMING_RENDER_FIX.md` | **流式修复** | 问题根因与修复说明（交叉引用本文 §5.6） |
| `uncode-tui/src/tool_renderer.rs` | B2 | 图标更新 |
| `uncode-tui/src/diff_viewer.rs` | A3 | 提取公共渲染函数 |
