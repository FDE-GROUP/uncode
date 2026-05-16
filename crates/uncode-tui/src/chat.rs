use crate::theme::Theme;
use crate::tool_renderer::ToolRendererRegistry;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use uncode_core::event::{AgentEvent, DeltaType, ErrorCategory, ToolCallStatus};

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
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration_ms: Option<u64>,
        with_agent: bool,
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
}

/// 工具调用渲染状态（TUI 侧独立于 AgentEvent::ToolCallStatus）
#[derive(Debug, Clone, PartialEq)]
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
}

/// 对话状态容器
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub tool_output_visible: bool,
    pub thinking_visible: bool,
    pub thinking_level: ThinkingLevel,

    // --- Virtual scrolling cache ---
    line_counts: Vec<LineCountEntry>,
    prefix_sum: Vec<usize>,
    prefix_dirty: bool,
    cached_width: u16,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            tool_output_visible: true,
            thinking_visible: false,
            thinking_level: ThinkingLevel::default(),
            line_counts: Vec::new(),
            prefix_sum: vec![0],
            prefix_dirty: false,
            cached_width: 0,
        }
    }

    /// Deactivate the last Thinking block (stop spinner)
    fn deactivate_thinking(&mut self) {
        if let Some(ChatMessage::Thinking { active, .. }) = self.messages.last_mut() {
            *active = false;
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

    /// Push a new message, keeping cache vectors in sync
    fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.line_counts.push(LineCountEntry {
            line_count: 0,
            width: 0,
            cached_lines: None,
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
    pub fn ensure_line_counts(
        &mut self,
        width: u16,
        renderers: &ToolRendererRegistry,
        theme: &Theme,
        tick: usize,
        agent_busy: bool,
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
                .map_or(true, |e| e.width != width || e.cached_lines.is_none());

            if needs_recompute {
                let is_last = idx == self.messages.len() - 1;
                let mut msg_lines =
                    render_message(&self.messages[idx], width, renderers, theme, tick);

                // Streaming cursor for active assistant
                if is_last && agent_busy {
                    if let ChatMessage::Assistant { text } = &self.messages[idx] {
                        if !text.is_empty() && !msg_lines.is_empty() {
                            let show_cursor = tick % 4 < 2;
                            let last = msg_lines.pop().unwrap();
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
                }

                if let Some(entry) = self.line_counts.get_mut(idx) {
                    entry.line_count = msg_lines.len();
                    entry.width = width;
                    entry.cached_lines = Some(msg_lines);
                }
                self.prefix_dirty = true;
            }
        }

        if self.prefix_dirty {
            self.recompute_prefix_sum();
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
            if idx > 0 && idx > first {
                lines.push(Line::from(""));
                if lines.len() >= visible_height {
                    break;
                }
            } else if idx > first {
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
                if tool_name == "bash" {
                    let command = extract_bash_command(&arguments_summary);
                    self.push_message(ChatMessage::BashExecution {
                        tool_id,
                        command,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        duration_ms: None,
                        with_agent: true,
                    });
                } else {
                    self.push_message(ChatMessage::ToolCall {
                        tool_id,
                        tool_name,
                        arguments_summary,
                        status: ToolCallRenderStatus::Running,
                        duration_ms: None,
                        result: None,
                        expanded: false,
                    });
                }
            }
            AgentEvent::ToolCallProgress {
                tool_id, detail, ..
            } => {
                let idx = self.messages.iter().rposition(|m| {
                    matches!(
                        m,
                        ChatMessage::ToolCall { tool_id: tid, .. }
                        | ChatMessage::BashExecution { tool_id: tid, .. }
                        if tid == &tool_id
                    )
                });
                if let Some(idx) = idx {
                    match &mut self.messages[idx] {
                        ChatMessage::ToolCall {
                            arguments_summary,
                            result,
                            ..
                        } => {
                            // Show path immediately when arguments arrive
                            if arguments_summary.is_empty() && !detail.is_empty() {
                                *arguments_summary = detail.clone();
                            } else {
                                let r = result.get_or_insert_with(String::new);
                                r.push_str(&detail);
                                r.push('\n');
                            }
                        }
                        ChatMessage::BashExecution { stdout, .. } => {
                            stdout.push_str(&detail);
                            stdout.push('\n');
                        }
                        _ => {}
                    }
                    self.invalidate(idx);
                }
            }
            AgentEvent::ToolCallEnd {
                tool_id,
                tool_name: _,
                arguments,
                status,
                duration_ms,
                ..
            } => {
                let render_status = ToolCallRenderStatus::from(status);
                let idx = self.messages.iter().rposition(|m| {
                    matches!(
                        m,
                        ChatMessage::ToolCall { tool_id: tid, .. }
                        | ChatMessage::BashExecution { tool_id: tid, .. }
                        if tid == &tool_id
                    )
                });
                if let Some(idx) = idx {
                    match &mut self.messages[idx] {
                        ChatMessage::ToolCall {
                            status: s,
                            duration_ms: d,
                            arguments_summary: args,
                            ..
                        } => {
                            *s = render_status;
                            *d = Some(duration_ms);
                            if args.is_empty() {
                                *args = arguments.clone();
                            }
                        }
                        ChatMessage::BashExecution {
                            duration_ms: d,
                            stdout,
                            exit_code,
                            ..
                        } => {
                            *d = Some(duration_ms);
                            if let Some(exit_pos) = stdout.rfind("exit code:") {
                                let after = &stdout[exit_pos + 10..];
                                if let Ok(code) = after.trim().parse::<i32>() {
                                    *exit_code = Some(code);
                                }
                            }
                        }
                        _ => {}
                    }
                    self.invalidate(idx);
                }
            }
            AgentEvent::Error {
                message, category, ..
            } => {
                self.push_message(ChatMessage::Error { message, category });
            }
            AgentEvent::PhaseSummary {
                completed,
                next_steps,
                ..
            } => {
                self.push_message(ChatMessage::Summary {
                    completed,
                    next_steps,
                });
            }
            AgentEvent::CompactionComplete {
                messages_replaced,
                tokens_before,
                tokens_after,
                summary_text,
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
                        })
                        .collect();
                    self.prefix_dirty = true;
                }
            }
            _ => {}
        }
    }

    /// 添加用户消息，解析 @file 引用
    pub fn push_user_message(&mut self, text: String) {
        let file_refs = extract_file_refs(&text);
        self.push_message(ChatMessage::User { text, file_refs });
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
    ) -> Vec<Line<'static>> {
        let mut all_lines: Vec<Line<'static>> = Vec::with_capacity(self.messages.len() * 3);

        for (idx, msg) in self.messages.iter().enumerate() {
            let is_last = idx == self.messages.len() - 1;

            // Add blank line between messages
            if !all_lines.is_empty() {
                all_lines.push(Line::from(""));
            }

            let mut msg_lines = render_message(msg, area.width, renderers, theme, tick);

            // Streaming cursor: if last message is an active Assistant text, blink cursor
            if is_last && agent_busy {
                if let ChatMessage::Assistant { text } = msg {
                    if !text.is_empty() && !msg_lines.is_empty() {
                        // Blink every 2 ticks (~100ms at 50ms poll)
                        let show_cursor = tick % 4 < 2;
                        let last = msg_lines.pop().unwrap();
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

/// 渲染单条消息
fn render_message(
    msg: &ChatMessage,
    width: u16,
    renderers: &ToolRendererRegistry,
    theme: &Theme,
    tick: usize,
) -> Vec<Line<'static>> {
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
        } => {
            let icon: String = if *active {
                if (tick / 4) % 2 == 0 {
                    "●".into()
                } else {
                    "○".into()
                }
            } else {
                "●".into()
            };
            let icon_color = theme.tool_status.success;
            // Show full content when: expanded by user, or thinking completed (not active)
            let show_full = *expanded || (!*active && !text.is_empty());
            if text.is_empty() {
                vec![Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                    Span::styled("Thinking...", Style::default().fg(theme.ui.footer_text)),
                ])]
            } else if show_full {
                let mut lines = vec![Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                    Span::styled(
                        "Thinking",
                        Style::default()
                            .fg(theme.ui.footer_text)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])];
                let mut content_lines =
                    crate::markdown::render_markdown_with_theme(text, theme, Some(w));
                // Remove trailing blank lines from markdown rendering
                while content_lines.last().is_some_and(|l| l.spans.is_empty()) {
                    content_lines.pop();
                }
                lines.extend(
                    content_lines
                        .into_iter()
                        .map(|l| l.style(Style::default().fg(theme.ui.footer_text))),
                );
                lines
            } else {
                let preview: String = text.chars().take(60).collect();
                vec![Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                    Span::styled(
                        format!("Thinking — {preview}..."),
                        Style::default().fg(theme.ui.footer_text),
                    ),
                ])]
            }
        }
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
            *expanded,
            renderers,
            theme,
            width,
            tick,
        ),
        ChatMessage::BashExecution {
            command,
            exit_code,
            stdout,
            duration_ms,
            with_agent,
            ..
        } => render_bash(
            command,
            exit_code,
            stdout,
            duration_ms,
            *with_agent,
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
) -> Vec<Line<'static>> {
    let (icon, color) = match status {
        ToolCallRenderStatus::Running => {
            let dot = if (tick / 4) % 2 == 0 { "●" } else { "○" };
            (dot.to_string(), theme.tool_status.success)
        }
        ToolCallRenderStatus::Success => ("●".to_string(), theme.tool_status.success),
        ToolCallRenderStatus::Failed => ("✗".to_string(), theme.tool_status.failed),
        ToolCallRenderStatus::AwaitConfirm => ("…".to_string(), theme.tool_status.await_confirm),
        ToolCallRenderStatus::Pending => ("○".to_string(), theme.tool_status.pending),
    };

    let duration_str = duration_ms
        .map(|d| {
            if d < 1000 {
                format!("{d}ms")
            } else {
                format!("{:.1}s", d as f64 / 1000.0)
            }
        })
        .unwrap_or_default();

    // Use custom renderer for the call header
    let renderer = renderers.get(tool_name);
    let call_lines = renderer.render_call(args, width, theme);

    let mut lines = Vec::new();

    // Header line with status icon and duration
    let header = format!("{icon} {tool_name} {duration_str}");
    lines.push(Line::from(vec![Span::styled(
        header,
        Style::default().fg(color),
    )]));

    // Renderer summary lines
    for cl in call_lines {
        lines.push(cl);
    }

    if expanded {
        if let Some(res) = result {
            let result_lines = renderer.render_result(res, width, theme);
            for rl in result_lines {
                lines.push(rl);
            }
        }
    }

    lines
}

fn render_bash(
    command: &str,
    exit_code: &Option<i32>,
    stdout: &str,
    duration_ms: &Option<u64>,
    with_agent: bool,
    theme: &Theme,
    tick: usize,
) -> Vec<Line<'static>> {
    let prefix = if with_agent { "!shell" } else { "!!shell" };
    let (icon, color) = match exit_code {
        None => {
            let dot = if (tick / 4) % 2 == 0 { "●" } else { "○" };
            (dot.to_string(), theme.tool_status.success)
        }
        Some(0) => ("●".to_string(), theme.tool_status.success),
        Some(_) => ("✗".to_string(), theme.tool_status.failed),
    };

    let duration_str = duration_ms
        .map(|d| {
            if d < 1000 {
                format!("{d}ms")
            } else {
                format!("{:.1}s", d as f64 / 1000.0)
            }
        })
        .unwrap_or_default();

    let header = format!("{icon} {prefix} {command} {duration_str}");

    let mut lines = vec![Line::from(vec![Span::styled(
        header,
        Style::default().fg(color),
    )])];

    if !stdout.is_empty() {
        let all_lines: Vec<&str> = stdout.lines().collect();
        let max_show = 100;
        for line in all_lines.iter().take(max_show) {
            lines.push(Line::from(Span::raw(line.to_string())));
        }
        if all_lines.len() > max_show {
            lines.push(Line::from(Span::styled(
                format!("... ({} more lines)", all_lines.len() - max_show),
                Style::default().fg(theme.ui.footer_text),
            )));
        }
    }

    lines
}

/// 从 bash 工具参数中提取命令
fn extract_bash_command(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
            return cmd.to_string();
        }
    }
    args.to_string()
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
                path.push(chars.next().unwrap());
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
    use uncode_core::event::ErrorCategory;
    use uncode_core::message::UsageInfo;

    fn make_text_delta(content: &str) -> AgentEvent {
        AgentEvent::ContentDelta {
            delta_type: DeltaType::Text,
            content: content.to_string(),
        }
    }

    fn make_thinking_delta(content: &str) -> AgentEvent {
        AgentEvent::ContentDelta {
            delta_type: DeltaType::Thinking,
            content: content.to_string(),
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
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments: r#"{"path":"src/main.rs"}"#.into(),
            status: ToolCallStatus::Success,
            duration_ms: 42,
            output_size: Some(1024),
        });

        if let ChatMessage::ToolCall {
            status,
            duration_ms,
            ..
        } = &state.messages[1]
        {
            assert_eq!(*status, ToolCallRenderStatus::Success);
            assert_eq!(*duration_ms, Some(42));
        } else {
            panic!("Expected ToolCall message");
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

        if let ChatMessage::ToolCall {
            arguments_summary, ..
        } = &state.messages[1]
        {
            assert!(
                arguments_summary.is_empty(),
                "summary should be empty at start"
            );
        } else {
            panic!("Expected ToolCall message");
        }

        // LLM finishes streaming arguments → progress pushes them immediately
        state.handle_event(AgentEvent::ToolCallProgress {
            tool_id: "t1".into(),
            progress_type: uncode_core::event::ProgressType::Spinner,
            detail: r#"{"path":"crates/uncode-tui/src/chat.rs"}"#.into(),
        });

        if let ChatMessage::ToolCall {
            arguments_summary,
            status,
            result,
            ..
        } = &state.messages[1]
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
            panic!("Expected ToolCall message");
        }

        // Tool finishes → final state
        state.handle_event(AgentEvent::ToolCallEnd {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments: r#"{"path":"crates/uncode-tui/src/chat.rs"}"#.into(),
            status: ToolCallStatus::Success,
            duration_ms: 120,
            output_size: Some(2048),
        });

        if let ChatMessage::ToolCall {
            status,
            duration_ms,
            ..
        } = &state.messages[1]
        {
            assert_eq!(*status, ToolCallRenderStatus::Success);
            assert_eq!(*duration_ms, Some(120));
        } else {
            panic!("Expected ToolCall message");
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

        if let ChatMessage::ToolCall {
            arguments_summary, ..
        } = &state.messages[0]
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
            &state.messages[0],
            ChatMessage::BashExecution { command, .. } if command == "cargo test"
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
    fn test_summary_message() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::PhaseSummary {
            phase: 1,
            completed: vec!["分析代码".into()],
            issues: vec![],
            next_steps: vec!["实现功能".into()],
            token_usage: UsageInfo::default(),
        });
        assert!(matches!(
            &state.messages[0],
            ChatMessage::Summary { completed, next_steps, .. }
            if completed.len() == 1 && next_steps.len() == 1
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
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_render_lines_light_theme() {
        let state = ChatState::new();
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::light();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
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
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        // 验证自定义渲染器输出包含路径信息
        assert!(combined.contains("src/main.rs"));
        // 验证状态图标
        assert!(combined.contains("●"));
        // 验证耗时
        assert!(combined.contains("150ms"));
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
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
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
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
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
            exit_code: Some(0),
            stdout: "running 5 tests\nall passed".into(),
            stderr: String::new(),
            duration_ms: Some(3200),
            with_agent: true,
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("cargo test"));
        assert!(combined.contains("●"));
        assert!(combined.contains("3.2s"));
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
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
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
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("分析 @Cargo.toml"));
    }

    #[test]
    fn test_streaming_cursor_when_busy() {
        let mut state = ChatState::new();
        state.handle_event(AgentEvent::ContentDelta {
            delta_type: DeltaType::Text,
            content: "Hello world".to_string(),
        });
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);

        // tick 0 — cursor visible (tick % 4 < 2)
        let lines = state.render_lines(area, &renderers, &theme, 0, true);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("█"), "cursor should be visible at tick 0");

        // tick 2 — cursor hidden (tick % 4 >= 2)
        let lines = state.render_lines(area, &renderers, &theme, 2, true);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(!combined.contains("█"), "cursor should be hidden at tick 2");

        // Not busy — no cursor
        let lines = state.render_lines(area, &renderers, &theme, 0, false);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(!combined.contains("█"), "no cursor when agent not busy");
    }
}
