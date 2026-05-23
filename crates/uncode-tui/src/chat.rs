use crate::message_renderer::MessageRendererRegistry;
use crate::theme::Theme;
use crate::tool_renderer::ToolRendererRegistry;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use uncode_core::event::{
    AgentEvent, DeltaType, ErrorCategory, TaskStatus, ToolCallEndEventData, ToolCallStatus,
};

/// 对话消息类型
#[derive(Debug, Clone)]
pub enum ChatMessage {
    User {
        text: String,
        file_refs: Vec<String>,
    },
    Assistant {
        text: String,
    },
    Thinking {
        text: String,
        expanded: bool,
        active: bool,
        started_at: Option<std::time::Instant>,
        duration_ms: Option<u64>,
    },
    ToolCall {
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
        status: ToolCallRenderStatus,
        duration_ms: Option<u64>,
        result: Option<String>,
        expanded: bool,
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
    Error {
        message: String,
        category: ErrorCategory,
    },
    Summary {
        completed: Vec<String>,
        next_steps: Vec<String>,
    },
    CompactionSummary {
        messages_replaced: usize,
        tokens_before: u64,
        tokens_after: u64,
        summary_text: String,
    },
    QueuedMessage {
        text: String,
    },
    /// Extension custom message with type-specific rendering.
    Custom {
        message_type: String,
        content: String,
        expanded: bool,
    },
    /// Turn 边界（微观规划多轮决策）
    TurnDivider {
        turn: u64,
    },
    /// 同 Turn 内多个工具调用（P2：可折叠分组）
    ToolTurnGroup {
        turn: u64,
        expanded: bool,
        entries: Vec<ToolGroupEntry>,
    },
    /// 步骤 / 待办（TaskUpdate、PhaseSummary 或助手 Markdown 清单）
    TodoList {
        id: String,
        title: String,
        items: Vec<TodoItem>,
        expanded: bool,
    },
}

/// 单条待办
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub text: String,
    pub done: bool,
}

/// 单条工具项（`ToolTurnGroup` 内）
#[derive(Debug, Clone)]
pub enum ToolGroupEntry {
    ToolCall {
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
        status: ToolCallRenderStatus,
        duration_ms: Option<u64>,
        result: Option<String>,
        expanded: bool,
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

impl ToolGroupEntry {
    fn tool_id(&self) -> &str {
        match self {
            Self::ToolCall { tool_id, .. } | Self::BashExecution { tool_id, .. } => tool_id,
        }
    }

    fn tool_name_label(&self) -> &str {
        match self {
            Self::ToolCall { tool_name, .. } => tool_name,
            Self::BashExecution { .. } => "bash",
        }
    }
}

/// 工具调用渲染状态（TUI 侧独立于 AgentEvent::ToolCallStatus）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallRenderStatus {
    Pending,
    Running,
    Success,
    Failed,
    AwaitConfirm,
}

impl From<ToolCallStatus> for ToolCallRenderStatus {
    fn from(status: ToolCallStatus) -> Self {
        match status {
            ToolCallStatus::Success => Self::Success,
            ToolCallStatus::Failed => Self::Failed,
            ToolCallStatus::Cancelled => Self::Failed,
            _ => Self::Failed,
        }
    }
}

/// 思考级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

impl ThinkingLevel {
    pub fn cycle_next(self) -> Self {
        match self {
            Self::Off => Self::Minimal,
            Self::Minimal => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::XHigh,
            Self::XHigh => Self::Off,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Off => "○",
            Self::Minimal => "◔",
            Self::Low => "◑",
            Self::Medium => "◕",
            Self::High => "●",
            Self::XHigh => "⬤",
        }
    }

    pub fn border_color(self) -> Color {
        match self {
            Self::Off => Color::White,
            Self::Minimal => Color::DarkGray,
            Self::Low => Color::Blue,
            Self::Medium => Color::Cyan,
            Self::High => Color::Magenta,
            Self::XHigh => Color::Red,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for ThinkingLevel {
    fn default() -> Self {
        Self::Medium
    }
}

/// Per-message cached line count + rendered output
struct LineCountEntry {
    line_count: usize,
    width: u16,
    cached_lines: Option<Vec<Line<'static>>>,
    cached_text_len: usize,
}

/// 对话区渲染状态（消息列表、滚动、工具卡片焦点与虚拟滚动缓存）。
///
/// **Pi:** 无同名类型；消费 `AgentEvent` 并映射为可渲染的 [`ChatMessage`]。
/// **OpenCode:** scrollback 信息密度作 UX benchmark。
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub tool_output_visible: bool,
    pub thinking_visible: bool,
    pub thinking_level: ThinkingLevel,
    pub focused_card: Option<usize>,
    pub workdir: String,

    // --- Virtual scrolling cache ---
    line_counts: Vec<LineCountEntry>,
    prefix_sum: Vec<usize>,
    prefix_dirty: bool,
    cached_width: u16,

    /// Inner-loop turn from last `TurnStart` (0 before first turn).
    current_turn: u64,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            tool_output_visible: true,
            thinking_visible: true,
            thinking_level: ThinkingLevel::default(),
            focused_card: None,
            workdir: String::new(),
            line_counts: Vec::new(),
            prefix_sum: vec![0],
            prefix_dirty: false,
            cached_width: 0,
            current_turn: 0,
        }
    }

    /// Deactivate the last Thinking block (stop spinner)
    pub fn deactivate_thinking(&mut self) {
        if let Some(ChatMessage::Thinking {
            active,
            expanded,
            text,
            started_at,
            duration_ms,
        }) = self.messages.last_mut()
        {
            *active = false;
            if let Some(start) = started_at.take() {
                *duration_ms = Some(start.elapsed().as_millis() as u64);
            }
            if !text.is_empty() {
                *expanded = true;
            }
        }
    }

    /// Invalidate cache entry for message at idx
    fn invalidate(&mut self, idx: usize) {
        if let Some(entry) = self.line_counts.get_mut(idx) {
            entry.width = 0; // force recompute
            entry.cached_lines = None;
        }
        self.prefix_dirty = true;
    }

    /// Invalidate all cached entries
    pub fn invalidate_all(&mut self) {
        for entry in &mut self.line_counts {
            entry.width = 0;
            entry.cached_lines = None;
        }
        self.prefix_dirty = true;
    }

    // --- Card focus navigation ---

    pub fn tool_card_indices(&self) -> Vec<usize> {
        self.messages
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                matches!(
                    m,
                    ChatMessage::ToolCall { .. }
                        | ChatMessage::BashExecution { .. }
                        | ChatMessage::ToolTurnGroup { .. }
                        | ChatMessage::Thinking { .. }
                        | ChatMessage::TodoList { .. }
                        | ChatMessage::Custom { .. }
                )
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn focus_next_card(&mut self) -> bool {
        let cards = self.tool_card_indices();
        if cards.is_empty() {
            return false;
        }
        let next = match self.focused_card {
            Some(cur) => {
                let pos = cards.iter().position(|&c| c == cur).unwrap_or(0);
                cards[(pos + 1) % cards.len()]
            }
            None => cards[0],
        };
        self.focused_card = Some(next);
        true
    }

    pub fn focus_prev_card(&mut self) -> bool {
        let cards = self.tool_card_indices();
        if cards.is_empty() {
            return false;
        }
        let prev = match self.focused_card {
            Some(cur) => {
                let pos = cards.iter().position(|&c| c == cur).unwrap_or(0);
                cards[(pos + cards.len() - 1) % cards.len()]
            }
            None => cards[cards.len() - 1],
        };
        self.focused_card = Some(prev);
        true
    }

    pub fn toggle_focused_card(&mut self) -> bool {
        let idx = match self.focused_card {
            Some(i) => i,
            None => return false,
        };
        match &mut self.messages[idx] {
            ChatMessage::ToolCall { expanded, .. }
            | ChatMessage::BashExecution { expanded, .. }
            | ChatMessage::ToolTurnGroup { expanded, .. }
            | ChatMessage::Thinking { expanded, .. }
            | ChatMessage::TodoList { expanded, .. }
            | ChatMessage::Custom { expanded, .. } => {
                *expanded = !*expanded;
            }
            _ => return false,
        }
        self.invalidate(idx);
        true
    }

    pub fn clear_focus(&mut self) {
        if let Some(idx) = self.focused_card.take() {
            self.invalidate(idx);
        }
    }

    pub fn set_all_expanded(&mut self, expanded: bool) {
        let mut changed: Vec<usize> = Vec::new();
        for (idx, msg) in self.messages.iter_mut().enumerate() {
            match msg {
                ChatMessage::ToolCall { expanded: e, .. }
                | ChatMessage::BashExecution { expanded: e, .. }
                | ChatMessage::ToolTurnGroup { expanded: e, .. }
                | ChatMessage::Thinking { expanded: e, .. }
                | ChatMessage::TodoList { expanded: e, .. }
                | ChatMessage::Custom { expanded: e, .. }
                    if *e != expanded =>
                {
                    *e = expanded;
                    changed.push(idx);
                }
                _ => {}
            }
        }
        for idx in changed {
            self.invalidate(idx);
        }
    }

    pub fn set_thinking_expanded(&mut self, expanded: bool) {
        let mut changed: Vec<usize> = Vec::new();
        for (idx, msg) in self.messages.iter_mut().enumerate() {
            if let ChatMessage::Thinking { expanded: e, .. } = msg
                && *e != expanded
            {
                *e = expanded;
                changed.push(idx);
            }
        }
        for idx in changed {
            self.invalidate(idx);
        }
    }

    pub fn message_start_line(&self, idx: usize) -> usize {
        self.prefix_sum.get(idx).copied().unwrap_or(0)
    }

    /// Push a new message, keeping cache vectors in sync
    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.line_counts.push(LineCountEntry {
            line_count: 0,
            width: 0,
            cached_lines: None,
            cached_text_len: 0,
        });
        self.prefix_dirty = true;
    }

    /// Rebuild prefix sum from line_counts
    fn recompute_prefix_sum(&mut self) {
        self.prefix_sum.clear();
        self.prefix_sum.push(0);
        let mut acc = 0usize;
        // Each message adds a separator blank line (except the first)
        for (i, entry) in self.line_counts.iter().enumerate() {
            let sep = if i > 0 { 1 } else { 0 };
            acc += sep + entry.line_count;
            self.prefix_sum.push(acc);
        }
        self.prefix_dirty = false;
    }

    /// Total rendered lines across all messages
    pub fn total_lines(&self) -> usize {
        *self.prefix_sum.last().unwrap_or(&0)
    }

    /// Ensure all line counts are up to date for the given width.
    /// Re-renders only stale messages and caches the results.
    /// Uses incremental rendering for streaming messages (tail-only re-render).
    #[allow(clippy::too_many_arguments)]
    pub fn ensure_line_counts(
        &mut self,
        width: u16,
        renderers: &ToolRendererRegistry,
        theme: &Theme,
        tick: usize,
        agent_busy: bool,
        tool_output_visible: bool,
        workdir: &str,
        message_renderers: &MessageRendererRegistry,
    ) {
        let width_changed = width != self.cached_width;
        if width_changed {
            for entry in &mut self.line_counts {
                entry.width = 0;
                entry.cached_lines = None;
            }
            self.cached_width = width;
            self.prefix_dirty = true;
        }

        for idx in 0..self.messages.len() {
            let needs_recompute = self
                .line_counts
                .get(idx)
                .is_none_or(|e| e.width != width || e.cached_lines.is_none());

            if !needs_recompute {
                continue;
            }

            let is_last = idx == self.messages.len() - 1;
            let focused = self.focused_card == Some(idx);

            // Incremental path: streaming append, width unchanged, text grew
            let can_incremental = is_last
                && self.line_counts.get(idx).is_some_and(|e| {
                    e.cached_lines.is_some()
                        && e.cached_text_len > 0
                        && message_text_len(&self.messages[idx]) > e.cached_text_len
                });

            let mut msg_lines = if can_incremental {
                self.render_incremental(
                    idx,
                    width,
                    renderers,
                    theme,
                    tick,
                    focused,
                    tool_output_visible,
                    message_renderers,
                )
            } else {
                render_message(
                    &self.messages[idx],
                    width,
                    renderers,
                    theme,
                    tick,
                    focused,
                    tool_output_visible,
                    workdir,
                    message_renderers,
                )
            };

            // Streaming cursor for active assistant
            if is_last
                && agent_busy
                && let ChatMessage::Assistant { text } = &self.messages[idx]
                && !text.is_empty()
                && !msg_lines.is_empty()
            {
                let show_cursor = tick % 4 < 2;
                if let Some(last) = msg_lines.pop() {
                    let mut spans = last.spans;
                    if show_cursor {
                        spans.push(Span::styled(
                            "█",
                            Style::default().fg(theme.tool_status.running),
                        ));
                    }
                    msg_lines.push(Line::from(spans));
                }
            }

            if let Some(entry) = self.line_counts.get_mut(idx) {
                entry.line_count = msg_lines.len();
                entry.width = width;
                entry.cached_text_len = message_text_len(&self.messages[idx]);
                entry.cached_lines = Some(msg_lines);
            }
            self.prefix_dirty = true;
        }

        if self.prefix_dirty {
            self.recompute_prefix_sum();
        }
    }

    /// Incremental render: keep cached prefix lines, only re-render the tail.
    #[allow(clippy::too_many_arguments)]
    fn render_incremental(
        &mut self,
        idx: usize,
        width: u16,
        renderers: &ToolRendererRegistry,
        theme: &Theme,
        tick: usize,
        focused: bool,
        tool_output_visible: bool,
        message_renderers: &MessageRendererRegistry,
    ) -> Vec<Line<'static>> {
        // Take ownership of old cached lines instead of cloning
        let old_lines = self.line_counts[idx]
            .cached_lines
            .take()
            .unwrap_or_default();
        let old_count = old_lines.len();

        // Full render of new content
        let new_lines = render_message(
            &self.messages[idx],
            width,
            renderers,
            theme,
            tick,
            focused,
            tool_output_visible,
            &self.workdir,
            message_renderers,
        );

        if old_count > 2 && new_lines.len() >= old_count {
            // Keep prefix (all but last line), append new tail
            let mut result = old_lines;
            result.pop(); // Last line may have changed
            result.extend(new_lines.into_iter().skip(old_count - 1));
            result
        } else {
            new_lines
        }
    }

    /// Find the range of visible message indices [first, last] for the given viewport.
    pub fn visible_range(&self, scroll_offset: usize, visible_height: usize) -> (usize, usize) {
        if self.messages.is_empty() || self.prefix_sum.len() < 2 {
            return (0, 0);
        }
        let start_line = scroll_offset;
        let end_line = scroll_offset + visible_height;

        // Binary search: find first message whose prefix_sum > start_line
        let first = self
            .prefix_sum
            .partition_point(|&sum| sum <= start_line)
            .saturating_sub(1)
            .min(self.messages.len() - 1);

        // Find last message whose prefix_sum <= end_line
        let last = self
            .prefix_sum
            .partition_point(|&sum| sum < end_line)
            .saturating_sub(1)
            .min(self.messages.len() - 1);

        (first, last)
    }

    /// Build viewport lines from cached renders for messages [first..=last].
    pub fn render_viewport(
        &mut self,
        first: usize,
        last: usize,
        scroll_offset: usize,
        visible_height: usize,
    ) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(visible_height + 4);
        let skip_in_first = scroll_offset.saturating_sub(self.prefix_sum[first]);

        for idx in first..=last {
            // Add separator blank line (matching original render_lines behavior)
            if idx > first {
                lines.push(Line::from(""));
                if lines.len() >= visible_height {
                    break;
                }
            }

            let cached = self
                .line_counts
                .get(idx)
                .and_then(|e| e.cached_lines.clone());

            let msg_lines = cached.unwrap_or_default();

            let iter: Box<dyn Iterator<Item = Line<'static>>> = if idx == first && skip_in_first > 0
            {
                Box::new(msg_lines.into_iter().skip(skip_in_first))
            } else {
                Box::new(msg_lines.into_iter())
            };

            for line in iter {
                lines.push(line);
                if lines.len() >= visible_height {
                    break;
                }
            }

            if lines.len() >= visible_height {
                break;
            }
        }

        lines
    }

    /// 处理 AgentEvent，更新对话消息列表
    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::ContentDelta {
                delta_type,
                content,
                content_index: _,
            } => match delta_type {
                DeltaType::Text => {
                    self.deactivate_thinking();
                    self.append_assistant_text(&content);
                }
                DeltaType::Thinking => self.append_thinking_text(&content),
                _ => {}
            },
            AgentEvent::ToolCallStart {
                tool_id,
                tool_name,
                arguments_summary,
            } => {
                self.deactivate_thinking();
                self.finalize_assistant();
                let entry = if tool_name == "bash" {
                    ToolGroupEntry::BashExecution {
                        tool_id,
                        command: extract_bash_command(&arguments_summary),
                        description: extract_json_str(&arguments_summary, "description"),
                        wd: extract_json_str(&arguments_summary, "workdir"),
                        exit_code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        duration_ms: None,
                        with_agent: true,
                        expanded: false,
                    }
                } else {
                    ToolGroupEntry::ToolCall {
                        tool_id,
                        tool_name,
                        arguments_summary,
                        status: ToolCallRenderStatus::Running,
                        duration_ms: None,
                        result: None,
                        expanded: false,
                    }
                };
                self.push_tool_group_entry(entry);
            }
            AgentEvent::ToolCallProgress {
                tool_id, detail, ..
            } => {
                let Some((idx, entry_idx)) = self.locate_tool(&tool_id) else {
                    return;
                };
                match entry_idx {
                    Some(ei) => {
                        if let ChatMessage::ToolTurnGroup { entries, .. } = &mut self.messages[idx]
                            && let Some(entry) = entries.get_mut(ei)
                        {
                            apply_tool_progress(entry, &detail);
                        }
                    }
                    None => match &mut self.messages[idx] {
                        ChatMessage::ToolCall {
                            arguments_summary,
                            result,
                            ..
                        } => {
                            if arguments_summary.is_empty() && !detail.is_empty() {
                                *arguments_summary = detail.clone();
                            } else {
                                let r = result.get_or_insert_with(String::new);
                                r.push_str(&detail);
                                r.push('\n');
                            }
                        }
                        ChatMessage::BashExecution {
                            command, stdout, ..
                        } => {
                            if command.is_empty() {
                                let cmd = extract_bash_command(&detail);
                                if cmd != detail {
                                    *command = cmd;
                                }
                            }
                            stdout.push_str(&detail);
                            stdout.push('\n');
                        }
                        _ => {}
                    },
                }
                self.invalidate(idx);
            }
            AgentEvent::ToolCallAwaitingApproval { tool_id, .. } => {
                self.set_tool_await_confirm(&tool_id);
            }
            AgentEvent::ToolCallEnd { data } => {
                let tool_id = &data.tool_id;
                let arguments = &data.arguments;
                let duration_ms = data.duration_ms;
                let render_status = ToolCallRenderStatus::from(data.status);
                let Some((idx, entry_idx)) = self.locate_tool(tool_id) else {
                    return;
                };
                match entry_idx {
                    Some(ei) => {
                        if let ChatMessage::ToolTurnGroup { entries, .. } = &mut self.messages[idx]
                            && let Some(entry) = entries.get_mut(ei)
                        {
                            apply_tool_end(entry, arguments, render_status, duration_ms, &data);
                        }
                    }
                    None => match &mut self.messages[idx] {
                        ChatMessage::ToolCall {
                            status: s,
                            duration_ms: d,
                            arguments_summary: args,
                            result,
                            expanded,
                            tool_name,
                            ..
                        } => {
                            *s = render_status;
                            *d = Some(duration_ms);
                            if args.is_empty() {
                                *args = arguments.clone();
                            }
                            if let Some(ref summary) = data.result_summary {
                                *result = Some(summary.clone());
                                if tool_name != "read" {
                                    *expanded = true;
                                }
                            }
                        }
                        ChatMessage::BashExecution {
                            duration_ms: d,
                            command,
                            stdout,
                            exit_code,
                            ..
                        } => {
                            *d = Some(duration_ms);
                            if command.is_empty() && !arguments.is_empty() {
                                *command = extract_bash_command(arguments);
                            }
                            if let Some(exit_pos) = stdout.rfind("exit code:") {
                                let after = &stdout[exit_pos + 10..];
                                if let Ok(code) = after.trim().parse::<i32>() {
                                    *exit_code = Some(code);
                                }
                            }
                        }
                        _ => {}
                    },
                }
                self.invalidate(idx);
            }
            AgentEvent::Error {
                message, category, ..
            } => {
                self.push_message(ChatMessage::Error { message, category });
            }
            AgentEvent::PhaseSummary { data } => {
                self.apply_phase_summary(&data);
            }
            AgentEvent::TaskUpdate { data } => {
                self.apply_task_update(&data);
            }
            AgentEvent::TurnStart { turn } => {
                self.current_turn = turn;
                if turn > 1 {
                    self.push_message(ChatMessage::TurnDivider { turn });
                }
            }
            AgentEvent::CompactionComplete {
                messages_replaced,
                tokens_before,
                tokens_after,
                summary_text,
                reason: _,
            } => {
                self.push_message(ChatMessage::CompactionSummary {
                    messages_replaced,
                    tokens_before,
                    tokens_after,
                    summary_text,
                });
            }
            AgentEvent::MessageQueued { text } => {
                self.push_message(ChatMessage::QueuedMessage { text });
            }
            AgentEvent::MessageDelivered { text } => {
                let before = self.messages.len();
                self.messages
                    .retain(|m| !matches!(m, ChatMessage::QueuedMessage { text: t } if t == &text));
                if self.messages.len() != before {
                    // Rebuild line_counts to match new messages indices
                    self.line_counts = self
                        .messages
                        .iter()
                        .map(|_| LineCountEntry {
                            line_count: 0,
                            width: 0,
                            cached_lines: None,
                            cached_text_len: 0,
                        })
                        .collect();
                    self.prefix_dirty = true;
                }
            }
            AgentEvent::TurnEnd { .. }
            | AgentEvent::SessionEnd { .. }
            | AgentEvent::AgentInterrupted { .. } => {
                self.deactivate_thinking();
                self.sync_todos_from_last_assistant();
            }
            _ => {}
        }
    }

    fn apply_phase_summary(&mut self, data: &uncode_core::event::PhaseSummaryData) {
        let mut items: Vec<TodoItem> = data
            .completed
            .iter()
            .map(|s| TodoItem {
                text: s.clone(),
                done: true,
            })
            .collect();
        items.extend(data.next_steps.iter().map(|s| TodoItem {
            text: s.clone(),
            done: false,
        }));
        if !items.is_empty() {
            let title = if data.phase > 0 {
                format!("Phase {}", data.phase)
            } else {
                "Progress".to_string()
            };
            self.upsert_todo_list(format!("phase:{}", data.phase), title, items);
        } else if !data.completed.is_empty() || !data.next_steps.is_empty() {
            self.push_message(ChatMessage::Summary {
                completed: data.completed.clone(),
                next_steps: data.next_steps.clone(),
            });
        }
        if !data.issues.is_empty() {
            self.push_message(ChatMessage::Summary {
                completed: Vec::new(),
                next_steps: data.issues.iter().map(|i| format!("⚠ {i}")).collect(),
            });
        }
    }

    fn apply_task_update(&mut self, data: &uncode_core::event::TaskUpdateData) {
        let parent_done = matches!(data.status, TaskStatus::Done);
        let items: Vec<TodoItem> = if data.subtasks.is_empty() {
            vec![TodoItem {
                text: data.title.clone(),
                done: parent_done,
            }]
        } else {
            data.subtasks
                .iter()
                .map(|s| TodoItem {
                    text: s.clone(),
                    done: parent_done,
                })
                .collect()
        };
        if items.is_empty() {
            return;
        }
        let title = if data.title.is_empty() {
            "Tasks".to_string()
        } else {
            data.title.clone()
        };
        self.upsert_todo_list(data.task_id.clone(), title, items);
    }

    fn upsert_todo_list(&mut self, id: String, title: String, items: Vec<TodoItem>) {
        if let Some(idx) = self
            .messages
            .iter()
            .rposition(|m| matches!(m, ChatMessage::TodoList { id: tid, .. } if tid == &id))
        {
            if let ChatMessage::TodoList {
                title: t,
                items: its,
                ..
            } = &mut self.messages[idx]
            {
                *t = title;
                *its = items;
            }
            self.invalidate(idx);
        } else {
            self.push_message(ChatMessage::TodoList {
                id,
                title,
                items,
                expanded: true,
            });
        }
    }

    fn sync_todos_from_last_assistant(&mut self) {
        let text = self.messages.iter().rev().find_map(|m| match m {
            ChatMessage::Assistant { text } => Some(text.as_str()),
            _ => None,
        });
        let Some(text) = text else {
            return;
        };
        let items = parse_markdown_todos(text);
        if items.is_empty() {
            return;
        }
        self.upsert_todo_list("assistant".to_string(), "Todos".to_string(), items);
    }

    /// 添加用户消息，解析 @file 引用
    pub fn push_user_message(&mut self, text: String) {
        let file_refs = extract_file_refs(&text);
        self.push_message(ChatMessage::User { text, file_refs });
    }

    fn push_tool_group_entry(&mut self, entry: ToolGroupEntry) {
        let turn = self.current_turn.max(1);
        if let Some(ChatMessage::ToolTurnGroup {
            turn: t, entries, ..
        }) = self.messages.last_mut()
            && *t == turn
        {
            entries.push(entry);
            let last = self.messages.len() - 1;
            self.invalidate(last);
            return;
        }
        self.push_message(ChatMessage::ToolTurnGroup {
            turn,
            expanded: false,
            entries: vec![entry],
        });
    }

    /// Mark a running tool card as waiting for user confirmation.
    fn set_tool_await_confirm(&mut self, tool_id: &str) {
        let Some((idx, entry_idx)) = self.locate_tool(tool_id) else {
            return;
        };
        match entry_idx {
            Some(ei) => {
                if let ChatMessage::ToolTurnGroup { entries, .. } = &mut self.messages[idx]
                    && let Some(entry) = entries.get_mut(ei)
                {
                    match entry {
                        ToolGroupEntry::ToolCall { status, .. } => {
                            *status = ToolCallRenderStatus::AwaitConfirm;
                        }
                        ToolGroupEntry::BashExecution { .. } => {}
                    }
                }
            }
            None => {
                if let ChatMessage::ToolCall { status, .. } = &mut self.messages[idx] {
                    *status = ToolCallRenderStatus::AwaitConfirm;
                }
            }
        }
        self.invalidate(idx);
    }

    /// `(message_index, entry_index)` — `entry_index` is `None` for legacy standalone tool cards.
    fn locate_tool(&self, tool_id: &str) -> Option<(usize, Option<usize>)> {
        for (mi, msg) in self.messages.iter().enumerate().rev() {
            match msg {
                ChatMessage::ToolCall { tool_id: tid, .. }
                | ChatMessage::BashExecution { tool_id: tid, .. }
                    if tid == tool_id =>
                {
                    return Some((mi, None));
                }
                ChatMessage::ToolTurnGroup { entries, .. }
                    if let Some(ei) = entries.iter().position(|e| e.tool_id() == tool_id) =>
                {
                    return Some((mi, Some(ei)));
                }
                _ => {}
            }
        }
        None
    }

    /// 追加 Assistant 文本
    fn append_assistant_text(&mut self, content: &str) {
        if let Some(ChatMessage::Assistant { text, .. }) = self.messages.last_mut() {
            text.push_str(content);
            self.invalidate(self.messages.len() - 1);
        } else {
            self.push_message(ChatMessage::Assistant {
                text: content.to_string(),
            });
        }
    }

    /// 追加思考文本
    fn append_thinking_text(&mut self, content: &str) {
        if let Some(ChatMessage::Thinking { text, active, .. }) = self.messages.last_mut() {
            text.push_str(content);
            *active = true;
            let last = self.messages.len() - 1;
            self.invalidate(last);
        } else {
            // Deactivate any prior Thinking blocks
            let mut to_invalidate: Vec<usize> = Vec::new();
            for (i, msg) in self.messages.iter_mut().enumerate().rev() {
                if let ChatMessage::Thinking { active, .. } = msg {
                    *active = false;
                    to_invalidate.push(i);
                } else {
                    break;
                }
            }
            self.push_message(ChatMessage::Thinking {
                text: content.to_string(),
                expanded: self.thinking_visible,
                active: true,
                started_at: Some(std::time::Instant::now()),
                duration_ms: None,
            });
            for idx in to_invalidate {
                self.invalidate(idx);
            }
        }
    }

    /// 最终化 Assistant 消息
    fn finalize_assistant(&mut self) {
        // No-op: render on demand with current width for responsive resizing
    }

    /// 渲染对话区可见行
    pub fn render_lines(
        &self,
        area: Rect,
        renderers: &ToolRendererRegistry,
        theme: &Theme,
        tick: usize,
        agent_busy: bool,
        tool_output_visible: bool,
        message_renderers: &MessageRendererRegistry,
    ) -> Vec<Line<'static>> {
        let mut all_lines: Vec<Line<'static>> = Vec::with_capacity(self.messages.len() * 3);

        for (idx, msg) in self.messages.iter().enumerate() {
            let is_last = idx == self.messages.len() - 1;
            let focused = self.focused_card == Some(idx);

            // Add blank line between messages
            if !all_lines.is_empty() {
                all_lines.push(Line::from(""));
            }

            let mut msg_lines = render_message(
                msg,
                area.width,
                renderers,
                theme,
                tick,
                focused,
                tool_output_visible,
                &self.workdir,
                message_renderers,
            );

            // Streaming cursor: if last message is an active Assistant text, blink cursor
            if is_last
                && agent_busy
                && let ChatMessage::Assistant { text } = msg
                && !text.is_empty()
                && !msg_lines.is_empty()
            {
                // Blink every 2 ticks (~100ms at 50ms poll)
                let show_cursor = tick % 4 < 2;
                if let Some(last) = msg_lines.pop() {
                    let mut spans = last.spans;
                    if show_cursor {
                        spans.push(Span::styled(
                            "█",
                            Style::default().fg(theme.tool_status.running),
                        ));
                    }
                    msg_lines.push(Line::from(spans));
                }
            }

            all_lines.extend(msg_lines);
        }

        all_lines
    }
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
    }
}

/// 提取消息主文本长度（用于增量渲染检测）
fn message_text_len(msg: &ChatMessage) -> usize {
    match msg {
        ChatMessage::User { text, .. } => text.len(),
        ChatMessage::Assistant { text } => text.len(),
        ChatMessage::Thinking { text, .. } => text.len(),
        ChatMessage::ToolCall {
            result,
            arguments_summary,
            ..
        } => arguments_summary.len() + result.as_ref().map_or(0, |r| r.len()),
        ChatMessage::BashExecution { stdout, stderr, .. } => stdout.len() + stderr.len(),
        ChatMessage::Error { message, .. } => message.len(),
        ChatMessage::CompactionSummary { summary_text, .. } => summary_text.len(),
        ChatMessage::QueuedMessage { text } => text.len(),
        ChatMessage::Custom { content, .. } => content.len(),
        ChatMessage::Summary {
            completed,
            next_steps,
        } => {
            completed.iter().map(String::len).sum::<usize>()
                + next_steps.iter().map(String::len).sum::<usize>()
        }
        ChatMessage::TurnDivider { .. } => 1,
        ChatMessage::ToolTurnGroup {
            entries, expanded, ..
        } => entries.len() * if *expanded { 4 } else { 1 },
        ChatMessage::TodoList { title, items, .. } => {
            title.len() + items.iter().map(|i| i.text.len()).sum::<usize>()
        }
    }
}

fn truncate_preview(s: &str, max_chars: usize) -> String {
    let t = s.trim().replace('\n', " ");
    uncode_core::text::truncate_chars(&t, max_chars)
}

/// 从助手 Markdown 提取 `- [ ]` / `- [x]` 待办（必要时展示 Todos）
fn parse_markdown_todos(text: &str) -> Vec<TodoItem> {
    let mut items = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("- [ ]")
            .or_else(|| trimmed.strip_prefix("* [ ]"))
        {
            let t = rest.trim();
            if !t.is_empty() {
                items.push(TodoItem {
                    text: t.to_string(),
                    done: false,
                });
            }
        } else if let Some(rest) = trimmed
            .strip_prefix("- [x]")
            .or_else(|| trimmed.strip_prefix("- [X]"))
            .or_else(|| trimmed.strip_prefix("* [x]"))
            .or_else(|| trimmed.strip_prefix("* [X]"))
        {
            let t = rest.trim();
            if !t.is_empty() {
                items.push(TodoItem {
                    text: t.to_string(),
                    done: true,
                });
            }
        }
    }
    items
}

fn render_todo_list(
    title: &str,
    items: &[TodoItem],
    expanded: bool,
    focused: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let (prefix, prefix_color) = if focused {
        if expanded {
            ("▾ ", theme.tool_status.running)
        } else {
            ("▸ ", theme.tool_status.running)
        }
    } else {
        ("  ", theme.ui.summary_card)
    };
    let done_count = items.iter().filter(|i| i.done).count();
    let mut lines = vec![Line::from(vec![
        Span::styled(prefix, Style::default().fg(prefix_color)),
        Span::styled(
            format!("Todos · {title} ({done_count}/{})", items.len()),
            Style::default()
                .fg(theme.ui.summary_card)
                .add_modifier(Modifier::BOLD),
        ),
    ])];
    if expanded {
        for item in items {
            let (box_ch, color) = if item.done {
                ("☑", theme.tool_status.success)
            } else {
                ("☐", theme.markdown.code_text)
            };
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(format!("{box_ch} "), Style::default().fg(color)),
                Span::styled(
                    item.text.clone(),
                    Style::default().fg(if item.done {
                        theme.ui.footer_text
                    } else {
                        theme.markdown.code_text
                    }),
                ),
            ]));
        }
    }
    lines
}

/// Extract (message_type, content) for custom renderer lookup.
fn message_type_and_content(msg: &ChatMessage) -> Option<(&str, &str)> {
    match msg {
        ChatMessage::User { text, .. } => Some(("user", text.as_str())),
        ChatMessage::Assistant { text } => Some(("assistant", text.as_str())),
        ChatMessage::Thinking { text, .. } => Some(("thinking", text.as_str())),
        ChatMessage::Error { message, .. } => Some(("error", message.as_str())),
        ChatMessage::Custom {
            message_type,
            content,
            ..
        } => Some((message_type.as_str(), content.as_str())),
        _ => None,
    }
}

/// 渲染单条消息
fn apply_tool_progress(entry: &mut ToolGroupEntry, detail: &str) {
    match entry {
        ToolGroupEntry::ToolCall {
            arguments_summary,
            result,
            ..
        } => {
            if arguments_summary.is_empty() && !detail.is_empty() {
                *arguments_summary = detail.to_string();
            } else {
                let r = result.get_or_insert_with(String::new);
                r.push_str(detail);
                r.push('\n');
            }
        }
        ToolGroupEntry::BashExecution {
            command, stdout, ..
        } => {
            if command.is_empty() {
                let cmd = extract_bash_command(detail);
                if cmd != detail {
                    *command = cmd;
                }
            }
            stdout.push_str(detail);
            stdout.push('\n');
        }
    }
}

fn apply_tool_end(
    entry: &mut ToolGroupEntry,
    arguments: &str,
    render_status: ToolCallRenderStatus,
    duration_ms: u64,
    data: &ToolCallEndEventData,
) {
    match entry {
        ToolGroupEntry::ToolCall {
            status,
            duration_ms: d,
            arguments_summary,
            result,
            expanded,
            tool_name,
            ..
        } => {
            *status = render_status;
            *d = Some(duration_ms);
            if arguments_summary.is_empty() {
                *arguments_summary = arguments.to_string();
            }
            if let Some(ref summary) = data.result_summary {
                *result = Some(summary.clone());
                if tool_name != "read" {
                    *expanded = true;
                }
            }
        }
        ToolGroupEntry::BashExecution {
            duration_ms: d,
            command,
            stdout,
            exit_code,
            ..
        } => {
            *d = Some(duration_ms);
            if command.is_empty() && !arguments.is_empty() {
                *command = extract_bash_command(arguments);
            }
            if let Some(exit_pos) = stdout.rfind("exit code:") {
                let after = &stdout[exit_pos + 10..];
                if let Ok(code) = after.trim().parse::<i32>() {
                    *exit_code = Some(code);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_tool_turn_group(
    turn: u64,
    expanded: bool,
    entries: &[ToolGroupEntry],
    width: u16,
    renderers: &ToolRendererRegistry,
    theme: &Theme,
    tick: usize,
    focused: bool,
    tool_output_visible: bool,
    workdir: &str,
) -> Vec<Line<'static>> {
    let n = entries.len();
    let labels: Vec<String> = entries
        .iter()
        .map(|e| capitalize_tool(e.tool_name_label()))
        .collect();
    let summary = labels.join(", ");
    let (prefix, prefix_color) = if focused {
        if expanded {
            ("▾ ", theme.tool_status.running)
        } else {
            ("▸ ", theme.tool_status.running)
        }
    } else {
        ("  ", theme.ui.footer_text)
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(prefix, Style::default().fg(prefix_color)),
        Span::styled(
            format!("Turn {turn} · {n} tools"),
            Style::default()
                .fg(theme.ui.footer_text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({summary})"),
            Style::default().fg(theme.ui.footer_text),
        ),
    ])];
    if expanded {
        for entry in entries {
            let entry_lines = match entry {
                ToolGroupEntry::ToolCall {
                    tool_name,
                    arguments_summary,
                    status,
                    duration_ms,
                    result,
                    expanded: e_exp,
                    ..
                } => render_tool_call(
                    tool_name,
                    arguments_summary,
                    status,
                    duration_ms,
                    result,
                    *e_exp && tool_output_visible,
                    renderers,
                    theme,
                    width,
                    tick,
                    false,
                    workdir,
                ),
                ToolGroupEntry::BashExecution {
                    command,
                    description,
                    wd,
                    exit_code,
                    stdout,
                    duration_ms,
                    with_agent,
                    expanded: e_exp,
                    ..
                } => render_bash(
                    command,
                    description,
                    wd,
                    exit_code,
                    stdout,
                    duration_ms,
                    *with_agent,
                    *e_exp && tool_output_visible,
                    false,
                    theme,
                    tick,
                ),
            };
            for mut line in entry_lines {
                line.spans.insert(
                    0,
                    Span::styled("    ", Style::default().fg(theme.ui.footer_text)),
                );
                lines.push(line);
            }
            lines.push(Line::from(""));
        }
        if lines.last().is_some_and(|l| l.spans.is_empty()) {
            lines.pop();
        }
    }
    lines
}

#[allow(clippy::too_many_arguments)]
fn render_message(
    msg: &ChatMessage,
    width: u16,
    renderers: &ToolRendererRegistry,
    theme: &Theme,
    tick: usize,
    focused: bool,
    tool_output_visible: bool,
    workdir: &str,
    message_renderers: &MessageRendererRegistry,
) -> Vec<Line<'static>> {
    // Check for custom message renderer
    if let Some((msg_type, content)) = message_type_and_content(msg) {
        if let Some(renderer) = message_renderers.get(msg_type) {
            return renderer.render(content, width, theme);
        }
    }
    let w = width.saturating_sub(2) as usize; // account for padding
    match msg {
        ChatMessage::User { text, file_refs } => render_user_message(text, file_refs, w, theme),
        ChatMessage::Assistant { text, .. } => {
            let mut lines = if text.is_empty() {
                vec![]
            } else {
                crate::markdown::render_markdown_with_theme(text, theme, Some(w))
            };

            // Add "uncode" name prefix to first line
            if !lines.is_empty() {
                let first = lines.remove(0);
                let mut new_spans = vec![Span::styled(
                    "UnCode ",
                    Style::default()
                        .fg(theme.tool_status.success)
                        .add_modifier(Modifier::BOLD),
                )];
                new_spans.extend(first.spans);
                lines.insert(0, Line::from(new_spans));
            }

            lines
        }
        ChatMessage::Thinking {
            text,
            expanded,
            active,
            duration_ms,
            ..
        } => {
            let (icon, label): (&str, String) = if *active {
                let dot = if (tick / 4).is_multiple_of(2) {
                    "●"
                } else {
                    "○"
                };
                (dot, "Thinking".to_string())
            } else {
                let dur = match duration_ms {
                    Some(d) => format_duration(*d),
                    None => String::new(),
                };
                if dur.is_empty() {
                    ("●", "Thought".to_string())
                } else {
                    ("●", format!("Thought · {dur}"))
                }
            };
            let icon_color = if *active {
                theme.tool_status.running
            } else {
                theme.tool_status.success
            };
            let label_color = icon_color;

            let (prefix, prefix_color) = if focused {
                if *expanded {
                    ("▾ ", theme.tool_status.running)
                } else {
                    ("▸ ", theme.tool_status.running)
                }
            } else {
                ("  ", icon_color)
            };

            if text.is_empty() {
                vec![Line::from(vec![
                    Span::styled(prefix, Style::default().fg(prefix_color)),
                    Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                    Span::styled(label, Style::default().fg(theme.ui.footer_text)),
                ])]
            } else {
                let mut lines = vec![Line::from(vec![
                    Span::styled(prefix, Style::default().fg(prefix_color)),
                    Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                    Span::styled(
                        label,
                        Style::default()
                            .fg(label_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])];

                if *expanded && !text.is_empty() {
                    let mut content_lines =
                        crate::markdown::render_markdown_with_theme(text, theme, Some(w));
                    while content_lines.last().is_some_and(|l| l.spans.is_empty()) {
                        content_lines.pop();
                    }
                    lines.extend(
                        content_lines
                            .into_iter()
                            .map(|l| l.style(Style::default().fg(theme.ui.footer_text))),
                    );
                } else if !*expanded && !text.is_empty() && !*active && !focused {
                    let preview = truncate_preview(text, 96);
                    lines.push(Line::from(Span::styled(
                        format!("   {preview}"),
                        Style::default().fg(theme.ui.footer_text),
                    )));
                }
                lines
            }
        }
        ChatMessage::TurnDivider { turn } => {
            vec![Line::from(Span::styled(
                format!("── Turn {turn} ──"),
                Style::default()
                    .fg(theme.ui.footer_text)
                    .add_modifier(Modifier::ITALIC),
            ))]
        }
        ChatMessage::ToolTurnGroup {
            turn,
            expanded,
            entries,
        } => render_tool_turn_group(
            *turn,
            *expanded,
            entries,
            width,
            renderers,
            theme,
            tick,
            focused,
            tool_output_visible,
            workdir,
        ),
        ChatMessage::TodoList {
            title,
            items,
            expanded,
            ..
        } => render_todo_list(title, items, *expanded, focused, theme),
        ChatMessage::ToolCall {
            tool_name,
            arguments_summary,
            status,
            duration_ms,
            expanded,
            result,
            ..
        } => render_tool_call(
            tool_name,
            arguments_summary,
            status,
            duration_ms,
            result,
            *expanded && tool_output_visible,
            renderers,
            theme,
            width,
            tick,
            focused,
            workdir,
        ),
        ChatMessage::BashExecution {
            command,
            description,
            wd,
            exit_code,
            stdout,
            duration_ms,
            with_agent,
            expanded,
            ..
        } => render_bash(
            command,
            description,
            wd,
            exit_code,
            stdout,
            duration_ms,
            *with_agent,
            *expanded && tool_output_visible,
            focused,
            theme,
            tick,
        ),
        ChatMessage::Error { message, .. } => vec![Line::from(vec![
            Span::styled(" ! ", Style::default().fg(theme.ui.error_message)),
            Span::styled(message.clone(), Style::default().fg(theme.ui.error_message)),
        ])],
        ChatMessage::Summary {
            completed,
            next_steps,
        } => {
            let mut lines = vec![Line::from(Span::styled(
                " > Summary",
                Style::default()
                    .fg(theme.ui.summary_card)
                    .add_modifier(Modifier::BOLD),
            ))];
            if !completed.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   已完成：{}", completed.join("、")),
                    Style::default().fg(theme.tool_status.success),
                )));
            }
            if !next_steps.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   下一步：{}", next_steps.join("、")),
                    Style::default().fg(theme.markdown.code_text),
                )));
            }
            lines
        }
        ChatMessage::CompactionSummary {
            messages_replaced,
            tokens_before,
            tokens_after,
            summary_text,
        } => {
            let mut lines = vec![Line::from(Span::styled(
                " > Context compressed",
                Style::default()
                    .fg(theme.ui.summary_card)
                    .add_modifier(Modifier::BOLD),
            ))];
            lines.push(Line::from(Span::styled(
                format!(
                    "   {messages_replaced} 条消息被压缩 | {tokens_before} → {tokens_after} tokens"
                ),
                Style::default().fg(theme.ui.footer_text),
            )));
            if !summary_text.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   {summary_text}"),
                    Style::default().fg(theme.ui.footer_text),
                )));
            }
            lines
        }
        ChatMessage::QueuedMessage { text } => vec![Line::from(vec![
            Span::styled(" - ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(
                format!("排队中: {text}"),
                Style::default().fg(theme.ui.footer_text),
            ),
        ])],
        ChatMessage::Custom {
            message_type,
            content,
            expanded,
        } => {
            let (prefix, prefix_color) = if focused {
                if *expanded {
                    ("▾ ", theme.tool_status.running)
                } else {
                    ("▸ ", theme.tool_status.running)
                }
            } else {
                ("  ", theme.ui.footer_text)
            };
            let mut lines = vec![Line::from(vec![
                Span::styled(prefix, Style::default().fg(prefix_color)),
                Span::styled(
                    format!("[{message_type}]"),
                    Style::default()
                        .fg(theme.ui.summary_card)
                        .add_modifier(Modifier::BOLD),
                ),
            ])];
            if *expanded && !content.is_empty() {
                let content_lines =
                    crate::markdown::render_markdown_with_theme(content, theme, Some(w));
                lines.extend(content_lines);
            } else if !content.is_empty() {
                let preview = truncate_preview(content, 96);
                lines.push(Line::from(Span::styled(
                    format!("   {preview}"),
                    Style::default().fg(theme.ui.footer_text),
                )));
            }
            lines
        }
    }
}

fn render_user_message(
    text: &str,
    file_refs: &[String],
    _width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        "> ",
        Style::default()
            .fg(theme.tool_status.success)
            .add_modifier(Modifier::BOLD),
    )];

    if file_refs.is_empty() {
        spans.push(Span::raw(text.to_string()));
    } else {
        let mut remaining = text;
        for fref in file_refs {
            let pattern = format!("@{fref}");
            if let Some(pos) = remaining.find(&pattern) {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                spans.push(Span::styled(
                    pattern.clone(),
                    Style::default().fg(theme.markdown.code_text),
                ));
                remaining = &remaining[pos + pattern.len()..];
            }
        }
        if !remaining.is_empty() {
            spans.push(Span::raw(remaining.to_string()));
        }
    }

    vec![Line::from(spans)]
}

#[allow(clippy::too_many_arguments)]
fn render_tool_call(
    tool_name: &str,
    args: &str,
    status: &ToolCallRenderStatus,
    duration_ms: &Option<u64>,
    result: &Option<String>,
    expanded: bool,
    renderers: &ToolRendererRegistry,
    theme: &Theme,
    width: u16,
    tick: usize,
    focused: bool,
    workdir: &str,
) -> Vec<Line<'static>> {
    let (icon, color) = match status {
        ToolCallRenderStatus::Running => {
            let dot = if (tick / 4).is_multiple_of(2) {
                "●"
            } else {
                "○"
            };
            (dot.to_string(), theme.tool_status.success)
        }
        ToolCallRenderStatus::Success => ("●".to_string(), theme.tool_status.success),
        ToolCallRenderStatus::Failed => ("✗".to_string(), theme.tool_status.failed),
        ToolCallRenderStatus::AwaitConfirm => ("…".to_string(), theme.tool_status.await_confirm),
        ToolCallRenderStatus::Pending => ("○".to_string(), theme.tool_status.pending),
    };

    // Focus/expand indicator prefix
    let (prefix, prefix_color) = if focused {
        if expanded {
            ("▾ ", theme.tool_status.running)
        } else {
            ("▸ ", theme.tool_status.running)
        }
    } else {
        ("  ", color)
    };

    // Get inline display from renderer
    let renderer = renderers.get(tool_name);
    let inline = renderer.render_call(args, workdir);

    let label = capitalize_tool(tool_name);
    let mut lines = Vec::new();

    // Header line: ▸ ● ToolName first_line
    let mut inline_lines = inline.lines();
    let first_inline = inline_lines.next().unwrap_or("").to_string();
    let header = Line::from(vec![
        Span::styled(prefix, Style::default().fg(prefix_color)),
        Span::styled(format!("{icon} "), Style::default().fg(color)),
        Span::styled(
            format!("{label} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(first_inline, Style::default().fg(theme.tool_status.running)),
    ]);
    lines.push(header);

    // Continuation lines (e.g. Bash $ command after # description)
    for cont in inline_lines {
        lines.push(Line::from(vec![
            Span::styled("      ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(
                cont.to_string(),
                Style::default().fg(theme.tool_status.running),
            ),
        ]));
    }

    // Result lines with ⎿ prefix when expanded
    if expanded && let Some(res) = result {
        let renderer = renderers.get(tool_name);
        let result_lines = renderer.render_result(args, res, width, theme);
        let prefix_span = Span::styled("  \u{23bf}  ", Style::default().fg(theme.ui.footer_text));
        for rl in result_lines {
            // Prepend ⎿ prefix to each result line
            let mut spans = vec![prefix_span.clone()];
            spans.extend(rl.spans);
            lines.push(Line::from(spans));
        }
    }

    // Footer: ⎿ (duration) — skip for read since it's trivial
    if tool_name != "read"
        && let Some(d) = duration_ms
    {
        let dur = if *d < 1000 {
            format!("{d}ms")
        } else {
            format!("{:.1}s", *d as f64 / 1000.0)
        };
        lines.push(Line::from(vec![
            Span::styled("  \u{23bf} ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(
                format!("({dur})"),
                Style::default().fg(theme.ui.footer_text),
            ),
        ]));
    }

    lines
}

#[allow(clippy::too_many_arguments)]
fn render_bash(
    command: &str,
    description: &str,
    workdir: &str,
    exit_code: &Option<i32>,
    stdout: &str,
    duration_ms: &Option<u64>,
    with_agent: bool,
    expanded: bool,
    focused: bool,
    theme: &Theme,
    tick: usize,
) -> Vec<Line<'static>> {
    let status_icon = match exit_code {
        None => {
            if (tick / 4).is_multiple_of(2) {
                "●"
            } else {
                "○"
            }
        }
        Some(0) => "●",
        Some(_) => "✗",
    };
    let color = match exit_code {
        None | Some(0) => theme.tool_status.success,
        Some(_) => theme.tool_status.failed,
    };
    let label = if with_agent { "Bash" } else { "Shell" };

    let (fprefix, prefix_color) = if focused {
        if expanded {
            ("▾ ", theme.tool_status.running)
        } else {
            ("▸ ", theme.tool_status.running)
        }
    } else {
        ("  ", color)
    };

    let title = if description.is_empty() {
        "Shell".to_string()
    } else {
        description.to_string()
    };
    let dir = if workdir.is_empty() || workdir == "." {
        String::new()
    } else {
        workdir.to_string()
    };
    let title = if dir.is_empty() || title.contains(&dir) {
        title
    } else {
        format!("{title} in {dir}")
    };

    let mut lines = Vec::new();

    // Header: ▸ ● Bash # description
    lines.push(Line::from(vec![
        Span::styled(fprefix, Style::default().fg(prefix_color)),
        Span::styled(format!("{status_icon} "), Style::default().fg(color)),
        Span::styled(
            format!("{label} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("# {title}"),
            Style::default().fg(theme.bash.command),
        ),
    ]));

    // Command line:   $ command
    lines.push(Line::from(vec![
        Span::styled("      ", Style::default().fg(theme.ui.footer_text)),
        Span::styled(
            format!("$ {command}"),
            Style::default().fg(theme.bash.command),
        ),
    ]));

    // Output lines: ⎿ prefix
    if expanded && !stdout.is_empty() {
        let all_lines: Vec<&str> = stdout.lines().collect();
        let max_show = 20;
        let prefix_span = Span::styled("  \u{23bf}  ", Style::default().fg(theme.ui.footer_text));

        for line in all_lines.iter().take(max_show) {
            lines.push(Line::from(vec![
                prefix_span.clone(),
                Span::styled(line.to_string(), Style::default().fg(theme.bash.stdout)),
            ]));
        }
        if all_lines.len() > max_show {
            lines.push(Line::from(vec![
                Span::styled("     \u{2026} ", Style::default().fg(theme.ui.footer_text)),
                Span::styled(
                    format!("+{} lines (ctrl+o to expand)", all_lines.len() - max_show),
                    Style::default().fg(theme.ui.footer_text),
                ),
            ]));
        }
    }

    // Footer: ⎿ (duration)
    if let Some(d) = duration_ms {
        let dur = if *d < 1000 {
            format!("{d}ms")
        } else {
            format!("{:.1}s", *d as f64 / 1000.0)
        };
        lines.push(Line::from(vec![
            Span::styled("  \u{23bf} ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(
                format!("({dur})"),
                Style::default().fg(theme.ui.footer_text),
            ),
        ]));
    }

    lines
}

/// 从 bash 工具参数中提取命令
fn extract_bash_command(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args)
        && let Some(cmd) = val.get("command").and_then(|v| v.as_str())
    {
        return cmd.to_string();
    }
    if let Some(val) = extract_quoted_value(args, &["\"command\""]) {
        return val;
    }
    String::new()
}

fn extract_json_str(args: &str, key: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args)
        && let Some(v) = val.get(key).and_then(|v| v.as_str())
    {
        return v.to_string();
    }
    String::new()
}

/// Extract the first non-empty quoted value following any of the given keys.
fn extract_quoted_value(s: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(pos) = s.find(key) {
            let rest = s.get(pos + key.len()..)?;
            let colon = rest.find(':')?;
            let after = rest.get(colon + 1..)?.trim_start();
            if after.starts_with('"') {
                let inner = after.get(1..)?;
                let end = inner.find('"')?;
                let val = &inner[..end];
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Capitalize tool name: "read" → "Read", "web_fetch" → "WebFetch"
fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

fn capitalize_tool(name: &str) -> String {
    match name {
        "write" => "Write".to_string(),
        "grep" => "Grep".to_string(),
        "find" => "Find".to_string(),
        "ls" => "Ls".to_string(),
        "web_fetch" => "WebFetch".to_string(),
        "web_search" => "WebSearch".to_string(),
        _ => {
            let mut chars = name.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// 从用户输入中提取 @file 引用
fn extract_file_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '@' {
            let mut path = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_whitespace() || next == ')' || next == ']' || next == ',' {
                    break;
                }
                path.push(chars.next().unwrap_or(next));
            }
            if !path.is_empty() {
                refs.push(path);
            }
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_tool_entry(msgs: &[ChatMessage], idx: usize) -> &ToolGroupEntry {
        match &msgs[idx] {
            ChatMessage::ToolTurnGroup { entries, .. } => {
                entries.first().expect("tool group should have an entry")
            }
            _ => panic!("expected ToolTurnGroup at index {idx}"),
        }
    }
    use uncode_core::event::ErrorCategory;
    use uncode_core::event::{PhaseSummaryData, ToolCallEndEventData};
    use uncode_core::message::UsageInfo;

    fn make_text_delta(content: &str) -> AgentEvent {
        AgentEvent::ContentDelta {
            delta_type: DeltaType::Text,
            content: content.to_string(),
            content_index: None,
        }
    }

    fn make_thinking_delta(content: &str) -> AgentEvent {
        AgentEvent::ContentDelta {
            delta_type: DeltaType::Thinking,
            content: content.to_string(),
            content_index: None,
        }
    }

    #[test]
    fn test_push_user_message() {
        let mut state = ChatState::new();
        state.push_user_message("分析 @src/main.rs 的结构".to_string());
        assert_eq!(state.messages.len(), 1);
        if let ChatMessage::User { text, file_refs } = &state.messages[0] {
            assert_eq!(text, "分析 @src/main.rs 的结构");
            assert_eq!(file_refs, &["src/main.rs"]);
        } else {
            panic!("Expected User message");
        }
    }

    #[test]
    fn test_assistant_text_accumulation() {
        let mut state = ChatState::new();
        state.handle_event(make_text_delta("Hello "));
        state.handle_event(make_text_delta("World"));
        assert_eq!(state.messages.len(), 1);
        if let ChatMessage::Assistant { text, .. } = &state.messages[0] {
            assert_eq!(text, "Hello World");
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_thinking_text_accumulation() {
        let mut state = ChatState::new();
        state.handle_event(make_thinking_delta("分析中..."));
        state.handle_event(make_thinking_delta("完成"));
        assert_eq!(state.messages.len(), 1);
        if let ChatMessage::Thinking { text, .. } = &state.messages[0] {
            assert_eq!(text, "分析中...完成");
        } else {
            panic!("Expected Thinking message");
        }
    }

    #[test]
    fn test_tool_call_lifecycle() {
        let mut state = ChatState::new();
        state.handle_event(make_text_delta("正在分析"));

        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "src/main.rs".into(),
        });
        assert_eq!(state.messages.len(), 2);

        state.handle_event(AgentEvent::ToolCallEnd {
            data: Box::new(ToolCallEndEventData {
                tool_id: "t1".into(),
                tool_name: "read".into(),
                arguments: r#"{"path":"src/main.rs"}"#.into(),
                status: ToolCallStatus::Success,
                duration_ms: 42,
                output_size: Some(1024),
                result_summary: Some("file contents...".into()),
                is_error: false,
            }),
        });

        if let ToolGroupEntry::ToolCall {
            status,
            duration_ms,
            expanded,
            ..
        } = first_tool_entry(&state.messages, 1)
        {
            assert_eq!(*status, ToolCallRenderStatus::Success);
            assert_eq!(*duration_ms, Some(42));
            assert!(
                !*expanded,
                "read should not auto-expand — it's agent info gathering"
            );
        } else {
            panic!("Expected ToolCall entry");
        }
    }

    #[test]
    fn test_tool_call_auto_expand_for_non_read() {
        let mut state = ChatState::new();
        state.handle_event(make_text_delta("editing"));
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t1".into(),
            tool_name: "edit".into(),
            arguments_summary: r#"{"path":"src/main.rs"}"#.into(),
        });
        state.handle_event(AgentEvent::ToolCallEnd {
            data: Box::new(ToolCallEndEventData {
                tool_id: "t1".into(),
                tool_name: "edit".into(),
                arguments: r#"{"path":"src/main.rs"}"#.into(),
                status: ToolCallStatus::Success,
                duration_ms: 100,
                output_size: Some(512),
                result_summary: Some("diff...".into()),
                is_error: false,
            }),
        });
        if let ToolGroupEntry::ToolCall { expanded, .. } = first_tool_entry(&state.messages, 1) {
            assert!(*expanded, "edit tools should auto-expand");
        } else {
            panic!("Expected ToolCall entry");
        }
    }

    #[test]
    fn test_tool_call_progress_fills_empty_summary() {
        let mut state = ChatState::new();
        state.handle_event(make_text_delta("正在分析"));

        // Real flow: ToolCallStart arrives with empty summary
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: String::new(),
        });

        if let ToolGroupEntry::ToolCall {
            arguments_summary, ..
        } = first_tool_entry(&state.messages, 1)
        {
            assert!(
                arguments_summary.is_empty(),
                "summary should be empty at start"
            );
        } else {
            panic!("Expected ToolCall entry");
        }

        // LLM finishes streaming arguments → progress pushes them immediately
        state.handle_event(AgentEvent::ToolCallProgress {
            tool_id: "t1".into(),
            progress_type: uncode_core::event::ProgressType::Spinner,
            detail: r#"{"path":"crates/uncode-tui/src/chat.rs"}"#.into(),
        });

        if let ToolGroupEntry::ToolCall {
            arguments_summary,
            status,
            result,
            ..
        } = first_tool_entry(&state.messages, 1)
        {
            assert!(
                arguments_summary.contains("chat.rs"),
                "summary should now contain the file path, got: {arguments_summary}"
            );
            assert_eq!(*status, ToolCallRenderStatus::Running);
            // First progress sets summary but does NOT pollute result
            assert!(
                result.is_none() || result.as_ref().is_some_and(|r| r.is_empty()),
                "result should not contain args JSON after first progress"
            );
        } else {
            panic!("Expected ToolCall entry");
        }

        // Tool finishes → final state
        state.handle_event(AgentEvent::ToolCallEnd {
            data: Box::new(ToolCallEndEventData {
                tool_id: "t1".into(),
                tool_name: "read".into(),
                arguments: r#"{"path":"crates/uncode-tui/src/chat.rs"}"#.into(),
                status: ToolCallStatus::Success,
                duration_ms: 120,
                output_size: Some(2048),
                result_summary: Some("file contents...".into()),
                is_error: false,
            }),
        });

        if let ToolGroupEntry::ToolCall {
            status,
            duration_ms,
            ..
        } = first_tool_entry(&state.messages, 1)
        {
            assert_eq!(*status, ToolCallRenderStatus::Success);
            assert_eq!(*duration_ms, Some(120));
        } else {
            panic!("Expected ToolCall entry");
        }
    }

    #[test]
    fn test_tool_call_progress_does_not_overwrite_existing_summary() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t2".into(),
            tool_name: "grep".into(),
            arguments_summary: r#"{"pattern":"TODO"}"#.into(),
        });

        state.handle_event(AgentEvent::ToolCallProgress {
            tool_id: "t2".into(),
            progress_type: uncode_core::event::ProgressType::Spinner,
            detail: "some output".into(),
        });

        if let ToolGroupEntry::ToolCall {
            arguments_summary, ..
        } = first_tool_entry(&state.messages, 0)
        {
            assert_eq!(
                arguments_summary, r#"{"pattern":"TODO"}"#,
                "existing summary should not be overwritten"
            );
        }
    }

    #[test]
    fn test_bash_execution() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "b1".into(),
            tool_name: "bash".into(),
            arguments_summary: r#"{"command":"cargo test"}"#.into(),
        });
        assert!(matches!(
            first_tool_entry(&state.messages, 0),
            ToolGroupEntry::BashExecution { command, .. } if command == "cargo test"
        ));
    }

    #[test]
    fn test_error_message() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::Error {
            category: ErrorCategory::Llm,
            message: "timeout".into(),
            recoverable: true,
        });
        assert!(matches!(
            &state.messages[0],
            ChatMessage::Error { message, .. } if message == "timeout"
        ));
    }

    #[test]
    fn test_phase_summary_renders_todo_list() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::PhaseSummary {
            data: Box::new(PhaseSummaryData {
                phase: 1,
                completed: vec!["分析代码".into()],
                issues: vec![],
                next_steps: vec!["实现功能".into()],
                token_usage: UsageInfo::default(),
            }),
        });
        assert!(matches!(
            &state.messages[0],
            ChatMessage::TodoList { items, .. } if items.len() == 2
                && items[0].done && !items[1].done
        ));
    }

    #[test]
    fn test_parse_markdown_todos() {
        let text = "Plan:\n- [x] done step\n- [ ] next step\n";
        let items = parse_markdown_todos(text);
        assert_eq!(items.len(), 2);
        assert!(items[0].done);
        assert!(!items[1].done);
    }

    #[test]
    fn test_sync_todos_from_assistant_on_turn_end() {
        let mut state = ChatState::new();
        state.handle_event(make_text_delta("- [ ] fix tests\n- [x] add docs\n"));
        state.handle_event(AgentEvent::TurnEnd {
            turn: 1,
            usage: UsageInfo::default(),
        });
        assert!(state.messages.iter().any(|m| {
            matches!(m, ChatMessage::TodoList { id, items, .. }
                if id == "assistant" && items.len() == 2)
        }));
    }

    #[test]
    fn test_task_update_upserts_todos() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::TaskUpdate {
            data: Box::new(uncode_core::event::TaskUpdateData {
                task_id: "t1".into(),
                status: TaskStatus::Running,
                title: "Build feature".into(),
                subtasks: vec!["design".into(), "implement".into()],
                depends_on: vec![],
            }),
        });
        assert!(matches!(
            &state.messages[0],
            ChatMessage::TodoList { title, items, .. }
            if title == "Build feature" && items.len() == 2
        ));
    }

    #[test]
    fn test_turn_divider_from_turn_start() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::TurnStart { turn: 1 });
        state.handle_event(AgentEvent::TurnStart { turn: 2 });
        assert_eq!(state.messages.len(), 1);
        assert!(matches!(
            &state.messages[0],
            ChatMessage::TurnDivider { turn: 2 }
        ));
    }

    #[test]
    fn test_tool_calls_same_turn_grouped() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::TurnStart { turn: 1 });
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "a.rs".into(),
        });
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t2".into(),
            tool_name: "grep".into(),
            arguments_summary: "foo".into(),
        });
        assert_eq!(state.messages.len(), 1);
        assert!(matches!(
            &state.messages[0],
            ChatMessage::ToolTurnGroup { turn: 1, entries, .. } if entries.len() == 2
        ));
    }

    #[test]
    fn test_tool_calls_new_turn_new_group() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::TurnStart { turn: 1 });
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "a.rs".into(),
        });
        state.handle_event(AgentEvent::TurnStart { turn: 2 });
        state.handle_event(AgentEvent::ToolCallStart {
            tool_id: "t2".into(),
            tool_name: "write".into(),
            arguments_summary: "b.rs".into(),
        });
        assert_eq!(state.messages.len(), 3);
        assert!(matches!(
            &state.messages[0],
            ChatMessage::ToolTurnGroup { turn: 1, .. }
        ));
        assert!(matches!(
            &state.messages[1],
            ChatMessage::TurnDivider { turn: 2 }
        ));
        assert!(matches!(
            &state.messages[2],
            ChatMessage::ToolTurnGroup { turn: 2, .. }
        ));
    }

    #[test]
    fn test_thinking_level_cycle() {
        assert_eq!(ThinkingLevel::Off.cycle_next(), ThinkingLevel::Minimal);
        assert_eq!(ThinkingLevel::XHigh.cycle_next(), ThinkingLevel::Off);
    }

    #[test]
    fn test_extract_file_refs() {
        let refs = extract_file_refs("分析 @src/main.rs 和 @Cargo.toml");
        assert_eq!(refs, ["src/main.rs", "Cargo.toml"]);
    }

    #[test]
    fn test_extract_bash_command() {
        let cmd = extract_bash_command(r#"{"command":"cargo test --all"}"#);
        assert_eq!(cmd, "cargo test --all");
    }

    #[test]
    fn test_render_lines_default_theme() {
        let state = ChatState::new();
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_render_lines_light_theme() {
        let state = ChatState::new();
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::light();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_render_tool_call_with_renderer() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: r#"{"path":"src/main.rs"}"#.into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: Some(150),
            result: Some("line1\nline2".into()),
            expanded: true,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        // 验证自定义渲染器输出包含路径信息
        assert!(combined.contains("src/main.rs"));
        // 验证状态图标（完成后为空格，不显示●）
        assert!(combined.contains("●"));
        // read 工具不显示耗时
        assert!(!combined.contains("150ms"));
    }

    #[test]
    fn test_render_error_with_theme_color() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::Error {
            message: "编译失败".into(),
            category: ErrorCategory::Tool,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("编译失败"));
        assert!(combined.contains("!"));
    }

    #[test]
    fn test_render_summary_with_theme() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::Summary {
            completed: vec!["重构完成".into()],
            next_steps: vec!["运行测试".into()],
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("Summary"));
        assert!(combined.contains("重构完成"));
        assert!(combined.contains("运行测试"));
    }

    #[test]
    fn test_render_bash_with_theme() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::BashExecution {
            tool_id: "b1".into(),
            command: "cargo test".into(),
            description: "Run tests".into(),
            wd: String::new(),
            exit_code: Some(0),
            stdout: "running 5 tests\nall passed".into(),
            stderr: String::new(),
            duration_ms: Some(3200),
            with_agent: true,
            expanded: true,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            combined.contains("# Run tests"),
            "header should show Bash # description: {}",
            combined
        );
        assert!(
            combined.contains("$ cargo test"),
            "header should show $ command"
        );
        assert!(combined.contains("●"));
        assert!(combined.contains("3.2s"), "footer should show duration");
        assert!(combined.contains("running 5 tests"));
    }

    #[test]
    fn test_render_queued_message_with_theme() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::QueuedMessage {
            text: "帮我修复那个 bug".into(),
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("排队中"));
        assert!(combined.contains("帮我修复那个 bug"));
    }

    #[test]
    fn test_render_user_message_with_theme() {
        let mut state = ChatState::new();
        state.push_user_message("分析 @Cargo.toml".into());
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("分析 @Cargo.toml"));
    }

    #[test]
    fn test_streaming_cursor_when_busy() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::ContentDelta {
            delta_type: DeltaType::Text,
            content: "Hello world".to_string(),
            content_index: None,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);

        // tick 0 — cursor visible (tick % 4 < 2)
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            true,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("█"), "cursor should be visible at tick 0");

        // tick 2 — cursor hidden (tick % 4 >= 2)
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            2,
            true,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(!combined.contains("█"), "cursor should be hidden at tick 2");

        // Not busy — no cursor
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(!combined.contains("█"), "no cursor when agent not busy");
    }

    #[test]
    fn test_tool_card_indices() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::User {
            text: "hi".into(),
            file_refs: vec![],
        });
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "{}".into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: Some(100),
            result: None,
            expanded: false,
        });
        state.messages.push(ChatMessage::Assistant {
            text: "done".into(),
        });
        state.messages.push(ChatMessage::BashExecution {
            tool_id: "b1".into(),
            command: "ls".into(),
            description: String::new(),
            wd: String::new(),
            exit_code: Some(0),
            stdout: "file.txt".into(),
            stderr: String::new(),
            duration_ms: Some(50),
            with_agent: true,
            expanded: true,
        });
        let indices = state.tool_card_indices();
        assert_eq!(indices, vec![1, 3]);
    }

    #[test]
    fn test_focus_next_prev_cycle() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "{}".into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: None,
            result: None,
            expanded: false,
        });
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t2".into(),
            tool_name: "grep".into(),
            arguments_summary: "{}".into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: None,
            result: None,
            expanded: false,
        });

        assert!(state.focus_next_card());
        assert_eq!(state.focused_card, Some(0));

        assert!(state.focus_next_card());
        assert_eq!(state.focused_card, Some(1));

        // Wrap around
        assert!(state.focus_next_card());
        assert_eq!(state.focused_card, Some(0));

        // Prev wraps
        assert!(state.focus_prev_card());
        assert_eq!(state.focused_card, Some(1));
    }

    #[test]
    fn test_toggle_focused_card() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "{}".into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: None,
            result: Some("file content".into()),
            expanded: false,
        });
        state.focused_card = Some(0);

        assert!(state.toggle_focused_card());
        match &state.messages[0] {
            ChatMessage::ToolCall { expanded, .. } => assert!(expanded),
            _ => panic!("expected ToolCall"),
        }

        assert!(state.toggle_focused_card());
        match &state.messages[0] {
            ChatMessage::ToolCall { expanded, .. } => assert!(!expanded),
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn test_set_all_expanded() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "{}".into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: None,
            result: None,
            expanded: false,
        });
        state.messages.push(ChatMessage::BashExecution {
            tool_id: "b1".into(),
            command: "ls".into(),
            description: String::new(),
            wd: String::new(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: None,
            with_agent: true,
            expanded: false,
        });

        state.set_all_expanded(true);
        match &state.messages[0] {
            ChatMessage::ToolCall { expanded, .. } => assert!(expanded),
            _ => panic!("expected ToolCall"),
        }
        match &state.messages[1] {
            ChatMessage::BashExecution { expanded, .. } => assert!(expanded),
            _ => panic!("expected BashExecution"),
        }

        state.set_all_expanded(false);
        match &state.messages[0] {
            ChatMessage::ToolCall { expanded, .. } => assert!(!expanded),
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn test_render_tool_call_collapsed_no_result() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: r#"{"path":"src/main.rs"}"#.into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: Some(100),
            result: Some("fn main() {}".into()),
            expanded: false,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            combined.contains("Read → src/main.rs"),
            "should show tool name and args: {}",
            combined
        );
        assert!(
            !combined.contains("fn main()"),
            "collapsed should not show result content"
        );
    }

    #[test]
    fn test_render_tool_call_expanded_shows_result() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: r#"{"path":"src/main.rs"}"#.into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: Some(100),
            result: Some("fn main() {}".into()),
            expanded: true,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            combined.contains("fn main()"),
            "expanded should show result content"
        );
    }

    #[test]
    fn test_render_focused_card_has_indicator() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: r#"{"path":"src/main.rs"}"#.into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: Some(100),
            result: None,
            expanded: false,
        });
        state.focused_card = Some(0);
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains('▸'), "focused collapsed should show ▸");
    }

    #[test]
    fn test_render_bash_collapsed_no_stdout() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::BashExecution {
            tool_id: "b1".into(),
            command: "ls".into(),
            description: String::new(),
            wd: String::new(),
            exit_code: Some(0),
            stdout: "src\ntarget\nCargo.toml".into(),
            stderr: String::new(),
            duration_ms: Some(50),
            with_agent: true,
            expanded: false,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("ls"), "should show command");
        assert!(
            !combined.contains("Cargo.toml"),
            "collapsed should not show stdout"
        );
    }

    #[test]
    fn test_tool_output_visible_overrides_expanded() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: "{}".into(),
            status: ToolCallRenderStatus::Success,
            duration_ms: Some(100),
            result: Some("secret content".into()),
            expanded: true,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        // tool_output_visible = false should hide even expanded cards
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            false,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            !combined.contains("secret content"),
            "tool_output_visible=false should hide results"
        );
    }

    #[test]
    fn test_thinking_renders_duration_when_done() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::Thinking {
            text: "分析中...".into(),
            expanded: true,
            active: false,
            started_at: None,
            duration_ms: Some(1500),
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            combined.contains("Thought · 1.5s"),
            "thought should show duration when done: {}",
            combined
        );
    }

    #[test]
    fn test_thinking_renders_focus_indicator() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::Thinking {
            text: "done".into(),
            expanded: true,
            active: false,
            started_at: None,
            duration_ms: None,
        });
        state.focused_card = Some(0);
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            combined.contains("▾"),
            "focused expanded thinking should show ▾: {}",
            combined
        );
    }

    #[test]
    fn test_thinking_collapsed_focus_indicator() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::Thinking {
            text: "hidden".into(),
            expanded: false,
            active: false,
            started_at: None,
            duration_ms: None,
        });
        state.focused_card = Some(0);
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            combined.contains("▸"),
            "focused collapsed thinking should show ▸: {}",
            combined
        );
        assert!(
            !combined.contains("hidden"),
            "collapsed thinking should not show content"
        );
    }

    #[test]
    fn test_bash_renders_workdir_in_title() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::BashExecution {
            tool_id: "b1".into(),
            command: "ls".into(),
            description: "List files".into(),
            wd: "src".into(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: None,
            with_agent: true,
            expanded: false,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(
            area,
            &renderers,
            &theme,
            0,
            false,
            true,
            &MessageRendererRegistry::new(),
        );
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            combined.contains("# List files in src"),
            "should show workdir in title: {}",
            combined
        );
    }

    #[test]
    fn test_thinking_toggle_via_focus() {
        let mut state = ChatState::new();
        state.messages.push(ChatMessage::Thinking {
            text: "some thoughts".into(),
            expanded: false,
            active: false,
            started_at: None,
            duration_ms: None,
        });
        state.focused_card = Some(0);
        state.toggle_focused_card();
        if let ChatMessage::Thinking { expanded, .. } = &state.messages[0] {
            assert!(*expanded, "toggling focused thinking should expand");
        } else {
            panic!("Expected Thinking");
        }
    }
}
