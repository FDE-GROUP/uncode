# TUI 事件流分析报告

> uncode-tui 的事件驱动架构全面分析，涵盖输入事件处理、Agent 事件订阅、渲染管线与组件交互。

---

## 目录

1. [架构总览](#1-架构总览)
2. [事件流全景图](#2-事件流全景图)
3. [主循环：`TuiEngine::run()`](#3-主循环-tuienginerun)
4. [UI 输入事件流](#4-ui-输入事件流)
5. [Agent 事件流](#5-agent-事件流)
6. [渲染管线](#6-渲染管线)
7. [关键交互流程](#7-关键交互流程)
8. [组件协作矩阵](#8-组件协作矩阵)
9. [设计要点与权衡](#9-设计要点与权衡)

---

## 1. 架构总览

TUI 层的核心架构围绕 `TuiEngine` 展开。`TuiEngine` 是 TUI crate 的唯一入口结构体，持有所有子组件，并在 `run()` 方法中驱动整个事件循环。

```
┌─────────────────────────────────────────────────────────┐
│                      TuiEngine                           │
│  ┌──────────┐  ┌───────────┐  ┌──────────────────┐     │
│  │  chat     │  │  editor   │  │  event_rx         │     │
│  │(ChatState)│  │(InputEdit)│  │(broadcast::Rx)   │     │
│  └────┬─────┘  └─────┬─────┘  └────────┬─────────┘     │
│       │              │                 │                │
│  ┌────┴──────────────┴─────────────────┴──────────┐    │
│  │              tokio::select! (biased)            │    │
│  │   UI events ← → Agent events ← → idle tick     │    │
│  └─────────────────────────────────────────────────┘    │
│       │                                                 │
│  ┌────┴──────────────────────────────────────────┐     │
│  │            terminal.draw(|f| render(f))        │     │
│  │   Chat  │ Status │ Input │ Footer L1 │ Footer L2│    │
│  └───────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────┘
```

### 依赖关系

```
uncode-core (AgentEvent, UsageInfo, event types)
     ↓
uncode-tui (TuiEngine, ChatState, InputEditor, ...)
     ↓
uncode-agent (provides on_submit callback, SessionStore)
```

TUI 通过 `broadcast::Receiver<AgentEvent>` 订阅 Agent 事件，通过 `on_submit: Fn(String, CancellationToken, String)` 回调向 Agent 提交用户输入。

---

## 2. 事件流全景图

```
                    ┌──────────────┐
                    │   crossterm   │
                    │   event::poll │
                    └──────┬───────┘
                           │ Event
                    ┌──────▼───────┐
                    │  TuiEngine   │
                    │  主循环       │
                    └──┬──────┬────┘
                       │      │
          ┌────────────▼─┐  ┌─▼──────────────┐
          │  UI 事件处理  │  │ Agent 事件处理  │
          │              │  │                │
          │ Key/Mouse/   │  │ SessionStart   │
          │ Resize/Paste │  │ TurnEnd        │
          │              │  │ ContentDelta   │
          │ → 快捷键/ESC │  │ ToolCallStart  │
          │ → 输入编辑   │  │ ToolCallEnd    │
          │ → 覆盖层交互 │  │ Error          │
          │ → 权限确认   │  │ PhaseSummary   │
          │ → 斜杠命令   │  │ Compaction     │
          │ → 提交消息   │  │ MessageQueue   │
          └──────┬───────┘  └───────┬────────┘
                 │                  │
                 ▼                  ▼
          ┌─────────────────────────────────┐
          │         状态变更                  │
          │  agent_busy, activity, footer   │
          │  chat.messages, line_counts     │
          │  selector, welcome, permission  │
          └──────────────┬──────────────────┘
                         │
                         ▼
          ┌─────────────────────────────────┐
          │         渲染 (render)            │
          │  chat → status → input → footer │
          │  selector (overlay)             │
          │  welcome (overlay)              │
          └─────────────────────────────────┘
```

---

## 3. 主循环：`TuiEngine::run()`

### 3.1 循环结构

`run()` 使用 `tokio::select!` 的 **biased** 模式，形成三路并发事件源：

```rust
tokio::select! {
    biased;

    // 源 1：UI 事件（最高优先级）
    ui_result = async { /* event::poll + event::read 循环 */ } => { ... }

    // 源 2：Agent 事件（来自 broadcast channel）
    Ok(event) = event_rx.recv() => { ... }

    // 源 3：空闲 tick（200ms 超时，驱动动画）
    _ = tokio::time::sleep(Duration::from_millis(200)) => { /* 无操作，触发重绘 */ }
}
```

**关键特性**：

| 特性 | 说明 |
|------|------|
| `biased` | UI 事件优先于 Agent 事件，确保用户输入即时响应 |
| 50ms 轮询 | UI 在内部循环以 50ms 间隔轮询 crossterm，平衡响应性与 CPU 占用 |
| 200ms idle tick | 当无事件时，每 200ms 触发一次重绘，驱动 spinner 动画和思考状态指示灯 |
| 每轮重绘 | 每次 select 返回后，循环顶部执行 `terminal.draw(|f| self.render(f))` |

### 3.2 生命周期

```
terminal.init()
     │
     ▼
┌─  enter main loop  ──────────────────────────────┐
│  ┌──────────────────┐                            │
│  │ terminal.draw()  │ ← 每轮开始重绘              │
│  └────────┬─────────┘                            │
│           │                                       │
│     tokio::select! {                              │
│       UI events  →  handle_input()               │
│       Agent events → handle_event()              │
│       idle tick  →  (no-op, just re-render)       │
│     }                                             │
│           │                                       │
│     if quit_requested { break }                   │
└───────────────────────────────────────────────────┘
     │
     ▼
DisableMouseCapture + ratatui::restore()
```

---

## 4. UI 输入事件流

### 4.1 事件分发决策树

UI 事件处理遵循严格的优先级链：

```
Event::Key(key_event)
  │
  ├─ [1] ESC 键 (最高优先级)
  │   ├─ agent_busy → cancel token, 插入 Interrupted 消息
  │   ├─ focused_card → clear_focus()
  │   ├─ welcome.visible → hide()
  │   └─ selector.visible → hide()
  │
  ├─ [2] leader_pending → handle_leader_key()
  │
  ├─ [3] welcome.visible → Enter 关闭, 其他忽略
  │
  ├─ [4] permission.has_pending() → y/n/e 处理
  │
  ├─ [5] 全局快捷键 (Ctrl+*)
  │   ├─ Ctrl+X → leader_pending = true
  │   ├─ Ctrl+O → 切换 tool_output_visible
  │   ├─ Ctrl+T → 切换 thinking_visible
  │   ├─ Ctrl+L → 弹出模型选择器
  │   ├─ Ctrl+P / Ctrl+Shift+P → 模型正向/反向循环
  │   ├─ Ctrl+R → 重试上一条消息
  │   ├─ Ctrl+N → 新建会话
  │   ├─ Ctrl+/ → 撤销最后一轮
  │   ├─ Ctrl+G → 外部编辑器
  │   └─ Ctrl+C → 中断/退出
  │
  ├─ [6] BackTab → thinking_level 循环
  │
  ├─ [7] PageUp/PageDown → 对话区滚动
  │
  ├─ [8] 条件快捷键（需要特定状态）
  │   ├─ Ctrl+J (selector可见) → selector.next()
  │   ├─ Ctrl+K (selector可见) → selector.prev()
  │   ├─ Ctrl+J (selector隐藏 + input为空) → focus_next_card()
  │   ├─ Ctrl+K (selector隐藏 + input为空) → focus_prev_card()
  │   ├─ Space (focused_card) → toggle_focused_card()
  │   ├─ ↑/↓ (selector可见) → selector导航
  │   ├─ Enter (selector可见) → 选择模型
  │   └─ Enter (focused_card + input为空) → toggle
  │
  ├─ [9] Enter → InputEditor.handle_key() → InputAction::Submit
  │       → handle_submit() → 斜杠命令 或 submit_text()
  │
  └─ [10] 其他 Key → InputEditor.handle_key()
          → 字符插入 / 编辑操作 / Tab补全

Event::Mouse(mouse)
  ├─ ScrollUp → scroll_offset -= 3, auto_scroll = false
  └─ ScrollDown → scroll_offset += 3

Event::Resize(_, _) → terminal.autoresize() + terminal.clear()

Event::Paste(text) → InputEditor.handle_paste()
```

### 4.2 ESC 键的特殊处理

ESC 键在整个系统中承担多重角色，按优先级：

1. **拒绝权限确认**：如果 permission 有待确认项，执行 `deny()`
2. **中断 Agent**：如果 agent 正在工作，取消 CancellationToken，插入 `[Interrupted] Agent stopped.` 消息
3. **清除焦点**：退出卡片聚焦模式
4. **隐藏覆盖层**：关闭 Welcome 和 Selector

### 4.3 消息提交流

```
用户消息提交
  │
  ├─ SlashCommands::execute() 匹配斜杠命令
  │   ├─ /thinking → 切换思考可见性
  │   ├─ /details → 切换工具输出可见性
  │   ├─ /clear → 清空对话
  │   ├─ /compact → 上下文使用报告
  │   ├─ /model [name] → 切换模型
  │   ├─ /new → 新建会话
  │   ├─ /fork [id] → 分支
  │   ├─ /export [fmt] → 导出
  │   ├─ /sessions → 列出会话
  │   ├─ /branch → 显示分支
  │   ├─ /name [title] → 设置标题
  │   ├─ /copy → 复制回复
  │   ├─ /usage → Token 用量
  │   ├─ /reload → 重载配置
  │   ├─ /diff → git 变更
  │   ├─ /theme [name] → 主题切换
  │   ├─ /template → 模板
  │   ├─ /tree → 会话树
  │   ├─ /skills → 技能列表
  │   └─ /quit → 退出
  │
  ├─ Skill 调用 (/<skill_name> [args])
  │
  └─ 普通文本 → submit_text()
      ├─ agent_busy? → 排队 (MessageQueue + QueuedMessage)
      └─ idle? → 直接提交
          ├─ agent_busy = true
          ├─ footer.start_turn()
          ├─ chat.push_user_message()
          ├─ 文件引用展开 (expand_file_refs)
          └─ on_submit(text, cancel_token, model)
```

---

## 5. Agent 事件流

### 5.1 事件接收与分发

Agent 事件通过 `broadcast::Receiver<AgentEvent>` 接收，在 `TuiEngine::handle_event()` 中分发：

```
event_rx.recv()
  │
  ▼
TuiEngine::handle_event(event)
  │
  ├─ 更新自身状态 (agent_busy, activity, footer)
  │
  └─ chat.handle_event(event)  ← 委托给 ChatState
```

### 5.2 TuiEngine 层状态处理

| AgentEvent | 状态变更 |
|------------|----------|
| `SessionStart` | `session_id = session_id` |
| `TurnStart` | `agent_busy = true`, `footer.current_turn = turn`, `activity = Idle` |
| `TurnEnd` | `footer.update_usage()` only；**不**改 `agent_busy`（多 Turn ReAct 链保持 busy） |
| `SessionEnd` | `finish_agent_run()` + `footer.update_usage()` |
| `AgentSettled` | `finish_agent_run()` |
| `AgentInterrupted` | `finish_agent_run()` |
| `Error`（`recoverable = false`） | `finish_agent_run()` |
| `ContentDelta(Thinking)` | `activity = Thinking` |
| `ContentDelta(Text)` | `activity = Writing` |
| `ToolCallStart` | `activity = RunningTool { name }` |

**Turn 结束检测**：使用 `matches!` 宏检测 `TurnEnd | SessionEnd | AgentInterrupted`，触发 `flush_queue()` 消费排队消息。

> **注意（微观规划 UX）**：内层 ReAct 每一 Turn 都会 `TurnEnd`，但 TUI 在每次 `TurnEnd` 将 `agent_busy = false`，多 Turn 链中间会出现「假空闲」。详见 [`UNCODE_TUI_MICRO_PLANNING_UX.md`](UNCODE_TUI_MICRO_PLANNING_UX.md) §5.2。

### 5.3 ChatState 层消息转换

`ChatState::handle_event()` 将 `AgentEvent` 转换为 `ChatMessage` 枚举：

```
ContentDelta(Text)        → append_assistant_text() → ChatMessage::Assistant { text }
ContentDelta(Thinking)    → append_thinking_text()  → ChatMessage::Thinking { text }
ToolCallStart             → ChatMessage::ToolCall | ChatMessage::BashExecution
ToolCallProgress          → 更新已有消息的 arguments_summary / stdout
ToolCallEnd               → 更新状态、耗时、结果摘要
Error                     → ChatMessage::Error
PhaseSummary              → ChatMessage::TodoList（completed/next_steps）；issues → Summary
TaskUpdate                → ChatMessage::TodoList（upsert by task_id）
TurnStart (turn≥2)        → ChatMessage::TurnDivider
CompactionComplete        → ChatMessage::CompactionSummary
MessageQueued             → ChatMessage::QueuedMessage
MessageDelivered          → 移除对应 QueuedMessage

TurnEnd / SessionEnd / AgentInterrupted → deactivate_thinking()
```

**流式文本累积**：
- `append_assistant_text()`：如果最后一条消息已是 `Assistant`，追加文本；否则新建
- `append_thinking_text()`：如果最后一条已是 `Thinking`，追加并标记 active；否则先停用旧 Thinking 块，再新建

**思考文本的 active 状态**：
- 新的 Thinking 块：`active = true`（显示动态 spinner）
- 旧 Thinking 块：`active = false`（显示静态图标）
- 当 Assistant 文本开始/工具调用开始时，调用 `deactivate_thinking()` 停用最后一个 Thinking 块

---

## 6. 渲染管线

### 6.1 布局结构

```
┌────────────────────────────────────┐
│         对话区 (Chat Area)          │  Constraint::Min(0)
│         scroll_offset 管理          │
│         虚拟滚动 + 可见范围         │
├────────────────────────────────────┤
│         状态行 (Status Bar)         │  Constraint::Length(1)
│    " * Thinking (5s | 1.2k tok)"   │  (仅 agent_busy 时显示)
├────────────────────────────────────┤
│         输入栏 (Input Editor)       │  Constraint::Length(3)
│    > 用户输入 [Block: TOP border]  │
│    [补全弹窗 - 上方弹出]            │
├────────────────────────────────────┤
│  页脚 L1: workdir git-branch sid   │  Constraint::Length(1)
├────────────────────────────────────┤
│  页脚 L2: in/out/cost/ctx/time     │  Constraint::Length(1)
│           model 标签 + level icon   │
└────────────────────────────────────┘

覆盖层 (需要时渲染):
  ├─ WelcomeScreen  (居中弹窗, 55%×45%)
  └─ OverlaySelector (居中弹窗, 60%×40%)
```

### 6.2 对话区渲染的两阶段管线

对话区渲染采用**缓存增量渲染**策略：

#### 阶段 1：`ensure_line_counts()` — 更新行数缓存

```
对每条消息:
  ├─ 是最后一条 + 文本增长 + 宽度不变？
  │   └─ YES → render_incremental()  [增量渲染：保留前缀行，只重新渲染尾部]
  │   └─ NO  → render_message()      [全量渲染]
  │
  ├─ 是最后一条 + agent_busy + Assistant 文本非空？
  │   └─ YES → 添加闪烁光标 "█" (tick % 4 < 2 时可见)
  │
  └─ 更新 LineCountEntry { line_count, width, cached_text_len, cached_lines }
```

#### 阶段 2：`visible_range()` + `render_viewport()` — 视口裁剪

```
scroll_offset + visible_height
  │
  ├─ visible_range():
  │   通过 prefix_sum 数组做二分查找，确定 [first, last] 消息索引
  │
  └─ render_viewport():
      遍历 [first..=last]，从缓存中取出 Line<'static>，拼接为视口输出
```

### 6.3 虚拟滚动机制

| 组件 | 说明 |
|------|------|
| `line_counts: Vec<LineCountEntry>` | 每条消息的行数缓存 |
| `prefix_sum: Vec<usize>` | 前缀和数组，`prefix_sum[i]` = 前 i 条消息的总行数 |
| `scroll_offset: usize` | 当前滚动偏移（行号） |
| `auto_scroll: bool` | 是否自动跟随底部（有新消息时自动滚到底部） |

**自动滚动规则**：
- `scroll_offset + visible_height >= total_lines` → `auto_scroll = true`
- 用户手动滚动（PageUp、鼠标滚轮上滚）→ `auto_scroll = false`

### 6.4 ChatMessage 渲染矩阵

| 消息类型 | 渲染方式 | 特殊处理 |
|----------|----------|----------|
| `User` | `> text` + `@file` 高亮 | 文件引用使用 code_text 颜色 |
| `Assistant` | Markdown 渲染 + `UnCode ` 前缀 | 最后一条 + busy 时附加闪烁光标 |
| `Thinking` | `●/○ Thinking...` + 内容 | active=true 时图标闪烁，内容用 footer_text 颜色 |
| `ToolCall` | `▸/▾ ● ToolName(args)` + 折叠结果 | focused+expanded 时显示 `▾`，展开用 `⎿` 前缀显示结果 |
| `BashExecution` | `▸/▾ ● Bash(cmd)` + stdout | exit_code=None 时图标闪烁 |
| `Error` | ` ! message` | error_message 颜色 |
| `Summary` | `> Summary` + 已完成/下一步 | summary_card / success 颜色 |
| `CompactionSummary` | `> Context compressed` + 统计 | footer_text 颜色 |
| `QueuedMessage` | `- 排队中: text` | footer_text 颜色 |

### 6.5 状态行动态显示

状态行仅在 `agent_busy = true` 时渲染：

```
 * Thinking (5s | 1.2k tok)   ← activity=Thinking
 * Running bash (5s | 1.2k tok)  ← activity=RunningTool { name: "bash" }
 * Writing (5s | 1.2k tok)   ← activity=Writing
 * Processing (5s | 1.2k tok) ← activity=Idle (fallback)
```

状态行背景颜色为 `theme.tool_status.running`，文字加粗反色。

---

## 7. 关键交互流程

### 7.1 用户输入 → Agent 响应 完整链路

```
用户按键
  → crossterm poll → Event::Key
  → TuiEngine 事件分发
  → InputEditor::handle_key() → InputAction::Submit(text)
  → handle_submit(text, &on_submit)
  → submit_text(text, &on_submit)
      ├─ last_user_input = Some(text)
      ├─ agent_busy = true
      ├─ footer.start_turn()
      ├─ chat.push_user_message()
      ├─ expand_file_refs() 展开 @file 引用
      ├─ new_cancel_token() 创建取消令牌
      └─ on_submit(expanded, token, model)  → Agent 层

Agent 层处理中...
  → AgentEvent 通过 broadcast channel 发送

TuiEngine 接收事件:
  ├─ ContentDelta(Thinking) → activity = Thinking, chat.append_thinking_text()
  ├─ ContentDelta(Text)     → activity = Writing, chat.append_assistant_text()
  ├─ ToolCallStart          → activity = RunningTool, chat.push_message(ToolCall/Bash)
  ├─ ToolCallProgress       → 更新工具消息详情
  ├─ ToolCallEnd            → 更新工具消息状态/结果
  ├─ PhaseSummary           → chat.push_message(Summary)
  └─ TurnEnd                → agent_busy = false, flush_queue()
```

### 7.2 排队消息流程

```
Agent 工作中，用户输入新消息:
  agent_busy = true
  → queue.enqueue(text, QueueType::FollowUp)
  → chat.push_message(QueuedMessage { text })

Agent TurnEnd / SessionEnd / AgentInterrupted:
  → flush_queue()
  → queue.drain_follow_up().into_iter().next()
  → 如果有排队消息，立即提交:
      agent_busy = true
      chat.push_user_message(text)
      on_submit(text, token, model)
```

**两种队列模式**：
- `FollowUp`：Agent 完成全部工作后投递（`OneAtATime`）
- `Steering`：当前工具调用完成后立即投递（`All`）

### 7.3 中断流程

```
用户按 ESC 或 Ctrl+C (agent_busy = true):
  → current_cancel.cancel()
  → agent_busy = false
  → current_cancel = None
  → footer.end_turn()
  → chat.deactivate_thinking()
  → chat.invalidate_all()
  → chat.push_message(Summary { "[Interrupted] Agent stopped." })

注意: ESC 返回 continue，Ctrl+C 在 agent_busy=false 时 break 主循环
```

### 7.4 卡片聚焦与展开流程

```
Ctrl+J / Ctrl+K (input 为空):
  → chat.focus_next_card() / chat.focus_prev_card()
  → scroll_to_focused_card()

Space / Enter (focused_card 存在 + input 为空):
  → chat.toggle_focused_card()
  → 被聚焦的 ToolCall 或 BashExecution 的 expanded 翻转
  → 缓存失效 → 重新渲染

Ctrl+O:
  → tool_output_visible 翻转 + set_all_expanded(tool_output_visible)
```

### 7.5 权限确认流程

```
工具调用需要确认时:
  → permission.request_confirmation(tool_id, tool_name, args, allow_edit)
  → 状态行切换显示确认提示

用户按键:
  → y / Enter → permission.confirm(Allow)  → 允许执行
  → n         → permission.deny()         → 拒绝执行
  → e         → permission.confirm(Edit)  → 允许编辑后执行
  → Esc       → permission.deny()         → 拒绝执行（在全局 ESC handler 中优先于中断/清除逻辑处理）
```

---

## 8. 组件协作矩阵

| 组件 | 职责 | 输入 | 输出 |
|------|------|------|------|
| **TuiEngine** | 主循环、事件路由、状态管理 | KeyEvent, Mouse, Resize, Paste, AgentEvent | render() 调用, on_submit |
| **ChatState** | 消息存储、缓存行数、虚拟滚动 | AgentEvent, push_message() | Line<'static> 视口 |
| **InputEditor** | 文本编辑、历史、补全 | KeyEvent, Paste | InputAction::Submit/Cancel/None |
| **OverlaySelector** | 模型/选项选择弹出层 | show(), hide(), next(), prev() | selected_item() |
| **WelcomeScreen** | 首次启动欢迎界面 | 默认可见 | Enter/Esc 关闭 |
| **PermissionManager** | 危险操作确认 | needs_confirmation(), request() | confirm()/deny() |
| **CompletionEngine** | 斜杠命令 + 文件路径补全 | complete(input) | Vec<String> |
| **SlashCommands** | 内置命令路由 | execute(input) | Option<String> |
| **ToolRendererRegistry** | 工具结果自定义渲染 | get(tool_name) | &dyn ToolRenderer |
| **MessageQueue** | 消息排队 | enqueue(), drain_follow_up() | Vec<String> |
| **FooterState** | Token/费用/耗时统计 | update_usage(), start_turn(), end_turn() | 格式化的行文本 |
| **Theme** | 颜色系统 (4 内置主题 + JSON 自定义) | - | 颜色查找 |

---

## 9. 设计要点与权衡

### 9.1 渲染性能优化

- **缓存失效策略**：仅当消息文本变化或宽度变化时重新渲染该消息
- **增量渲染**：对于流式输出的最后一条消息，保留前缀缓存行，只重新渲染增长的尾部
- **视口裁剪**：通过 `prefix_sum` 前缀和 + 二分查找确定可见范围，只渲染可见消息
- **零分配静态分发**：`ToolRendererRegistry` 使用静态实例避免 vtable 和 HashMap 开销

### 9.2 状态一致性

- `agent_busy` 在 `TuiEngine` 和渲染上下文中多处使用，驱动状态行、闪烁光标、排队逻辑
- `handler_event` 同时更新 TuiEngine 自身状态和 ChatState，确保 UI 状态与对话状态同步
- `tick` 计数器（每帧自增）驱散闪烁效果：spinner 动画、光标闪烁、状态灯

### 9.3 事件优先级

- `biased` select 确保 UI 输入即时响应
- ESC 键优先于所有其他按键处理，保障用户随时能中断/退出
- Permission 确认优先级高于输入编辑，防止误操作

### 9.4 架构特性

- **broadcast 通道解耦**：TUI 与 Agent 通过事件流松耦合，Agent 不知道 TUI 的存在
- **回调式提交**：`on_submit` 函数由外部注入，TUI 不直接依赖 Agent 实现
- **可扩展性**：工具渲染器通过 `ToolRenderer` trait 注册，新增工具只需添加静态实例和 match arm
- **主题系统**：JSON 配置文件 + 热重载（/reload），支持内置 4 套主题 + 用户自定义
