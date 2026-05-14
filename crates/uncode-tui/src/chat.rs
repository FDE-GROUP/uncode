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
        rendered_cache: Option<Vec<Line<'static>>>,
    },
    Thinking {
        text: String,
        expanded: bool,
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

/// 对话状态容器
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: u16,
    pub auto_scroll: bool,
    pub tool_output_visible: bool,
    pub thinking_visible: bool,
    pub thinking_level: ThinkingLevel,
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
        }
    }

    /// 处理 AgentEvent，更新对话消息列表
    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::ContentDelta {
                delta_type,
                content,
            } => match delta_type {
                DeltaType::Text => self.append_assistant_text(&content),
                DeltaType::Thinking => self.append_thinking_text(&content),
                _ => {}
            },
            AgentEvent::ToolCallStart {
                tool_id,
                tool_name,
                arguments_summary,
            } => {
                self.finalize_assistant();
                if tool_name == "bash" {
                    let command = extract_bash_command(&arguments_summary);
                    self.messages.push(ChatMessage::BashExecution {
                        tool_id,
                        command,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        duration_ms: None,
                        with_agent: true,
                    });
                } else {
                    self.messages.push(ChatMessage::ToolCall {
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
                if let Some(msg) = self.messages.iter_mut().rev().find(|m| match m {
                    ChatMessage::ToolCall { tool_id: tid, .. }
                    | ChatMessage::BashExecution { tool_id: tid, .. } => tid == &tool_id,
                    _ => false,
                }) {
                    match msg {
                        ChatMessage::ToolCall { result, .. } => {
                            let r = result.get_or_insert_with(String::new);
                            r.push_str(&detail);
                            r.push('\n');
                        }
                        ChatMessage::BashExecution { stdout, .. } => {
                            stdout.push_str(&detail);
                            stdout.push('\n');
                        }
                        _ => {}
                    }
                }
            }
            AgentEvent::ToolCallEnd {
                tool_id,
                status,
                duration_ms,
                ..
            } => {
                let render_status = ToolCallRenderStatus::from(status);
                if let Some(msg) = self.messages.iter_mut().rev().find(|m| match m {
                    ChatMessage::ToolCall { tool_id: tid, .. }
                    | ChatMessage::BashExecution { tool_id: tid, .. } => tid == &tool_id,
                    _ => false,
                }) {
                    match msg {
                        ChatMessage::ToolCall {
                            status: s,
                            duration_ms: d,
                            ..
                        } => {
                            *s = render_status;
                            *d = Some(duration_ms);
                        }
                        ChatMessage::BashExecution {
                            duration_ms: d,
                            stdout,
                            ..
                        } => {
                            *d = Some(duration_ms);
                            if let Some(exit_pos) = stdout.rfind("exit code:") {
                                let after = &stdout[exit_pos + 10..];
                                if let Ok(_code) = after.trim().parse::<i32>() {
                                    // TODO: store exit_code when field is available
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            AgentEvent::Error {
                message, category, ..
            } => {
                self.messages.push(ChatMessage::Error { message, category });
            }
            AgentEvent::PhaseSummary {
                completed,
                next_steps,
                ..
            } => {
                self.messages.push(ChatMessage::Summary {
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
                self.messages.push(ChatMessage::CompactionSummary {
                    messages_replaced,
                    tokens_before,
                    tokens_after,
                    summary_text,
                });
            }
            AgentEvent::MessageQueued { text } => {
                self.messages.push(ChatMessage::QueuedMessage { text });
            }
            AgentEvent::MessageDelivered { text } => {
                self.messages
                    .retain(|m| !matches!(m, ChatMessage::QueuedMessage { text: t } if t == &text));
            }
            _ => {}
        }
    }

    /// 添加用户消息，解析 @file 引用
    pub fn push_user_message(&mut self, text: String) {
        let file_refs = extract_file_refs(&text);
        self.messages.push(ChatMessage::User { text, file_refs });
    }

    /// 追加 Assistant 文本
    fn append_assistant_text(&mut self, content: &str) {
        if let Some(ChatMessage::Assistant {
            text,
            rendered_cache,
        }) = self.messages.last_mut()
        {
            text.push_str(content);
            *rendered_cache = None; // invalidate cache
        } else {
            self.messages.push(ChatMessage::Assistant {
                text: content.to_string(),
                rendered_cache: None,
            });
        }
    }

    /// 追加思考文本
    fn append_thinking_text(&mut self, content: &str) {
        if let Some(ChatMessage::Thinking { text, .. }) = self.messages.last_mut() {
            text.push_str(content);
        } else {
            self.messages.push(ChatMessage::Thinking {
                text: content.to_string(),
                expanded: self.thinking_visible,
            });
        }
    }

    /// 最终化 Assistant 消息（缓存 markdown 渲染）
    fn finalize_assistant(&mut self) {
        if let Some(ChatMessage::Assistant {
            text,
            rendered_cache,
        }) = self.messages.last_mut()
        {
            if rendered_cache.is_none() && !text.is_empty() {
                *rendered_cache = Some(crate::markdown::render_markdown(text));
            }
        }
    }

    /// 渲染对话区可见行
    pub fn render_lines(
        &self,
        area: Rect,
        renderers: &ToolRendererRegistry,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let mut all_lines: Vec<Line<'static>> = Vec::with_capacity(self.messages.len() * 3);

        if self.messages.is_empty() {
            all_lines.push(Line::from(Span::styled(
                "描述你的需求，Agent 会自动完成。",
                Style::default().fg(theme.ui.footer_text),
            )));
            return all_lines;
        }

        for msg in &self.messages {
            let msg_lines = render_message(msg, area.width, renderers, theme);
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
) -> Vec<Line<'static>> {
    let w = width.saturating_sub(2) as usize; // account for padding
    match msg {
        ChatMessage::User { text, file_refs } => render_user_message(text, file_refs, w, theme),
        ChatMessage::Assistant {
            text,
            rendered_cache,
        } => {
            if let Some(cached) = rendered_cache {
                cached.clone()
            } else if text.is_empty() {
                vec![]
            } else {
                crate::markdown::render_markdown(text)
            }
        }
        ChatMessage::Thinking { text, expanded } => {
            if *expanded {
                let mut lines = vec![Line::from(Span::styled(
                    "💭 思考过程",
                    Style::default()
                        .fg(theme.markdown.heading)
                        .add_modifier(Modifier::BOLD),
                ))];
                let content_lines = crate::markdown::render_markdown(text);
                lines.extend(
                    content_lines
                        .into_iter()
                        .map(|l| l.style(Style::default().fg(theme.ui.footer_text))),
                );
                lines
            } else if text.is_empty() {
                vec![]
            } else {
                let preview: String = text.chars().take(60).collect();
                vec![Line::from(vec![
                    Span::styled(" 💭 ", Style::default().fg(theme.markdown.heading)),
                    Span::styled(
                        format!("思考过程 — {preview}..."),
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
        ),
        ChatMessage::BashExecution {
            command,
            exit_code,
            stdout,
            duration_ms,
            with_agent,
            ..
        } => render_bash(command, exit_code, stdout, duration_ms, *with_agent, theme),
        ChatMessage::Error { message, .. } => vec![Line::from(vec![
            Span::styled(" ✖ ", Style::default().fg(theme.ui.error_message)),
            Span::styled(message.clone(), Style::default().fg(theme.ui.error_message)),
        ])],
        ChatMessage::Summary {
            completed,
            next_steps,
        } => {
            let mut lines = vec![Line::from(Span::styled(
                " 📝 阶段总结",
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
                " 📝 上下文压缩",
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
            Span::styled(" ⏳ ", Style::default().fg(theme.ui.footer_text)),
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
) -> Vec<Line<'static>> {
    let (icon, color) = match status {
        ToolCallRenderStatus::Running => ("🔄", theme.tool_status.running),
        ToolCallRenderStatus::Success => ("✅", theme.tool_status.success),
        ToolCallRenderStatus::Failed => ("❌", theme.tool_status.failed),
        ToolCallRenderStatus::AwaitConfirm => ("⚠️", theme.tool_status.await_confirm),
        ToolCallRenderStatus::Pending => ("⏳", theme.tool_status.pending),
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
    let call_lines = renderer.render_call(args, width);

    let mut lines = Vec::new();

    // Header line with status icon and duration
    let header = format!("🛠 {tool_name} {icon} {duration_str}");
    lines.push(Line::from(vec![
        Span::styled(" ┌─ ", Style::default().fg(color)),
        Span::styled(header, Style::default().fg(color)),
    ]));

    // Renderer summary lines
    for cl in call_lines {
        lines.push(Line::from(Span::styled(" │ ", Style::default().fg(color))));
        lines.push(cl);
    }

    if expanded {
        if let Some(res) = result {
            let result_lines = renderer.render_result(res, width);
            for rl in result_lines {
                lines.push(Line::from(Span::styled(" │ ", Style::default().fg(color))));
                lines.push(rl);
            }
        }
    }

    lines.push(Line::from(Span::styled(" └─", Style::default().fg(color))));
    lines
}

fn render_bash(
    command: &str,
    exit_code: &Option<i32>,
    stdout: &str,
    duration_ms: &Option<u64>,
    with_agent: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let prefix = if with_agent { "!" } else { "!!" };
    let (icon, color) = match exit_code {
        None => ("🔄", theme.tool_status.running),
        Some(0) => ("✅", theme.tool_status.success),
        Some(_) => ("❌", theme.tool_status.failed),
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

    let header = format!("{prefix} {command} {icon} {duration_str}");

    let mut lines = vec![Line::from(vec![
        Span::styled(" ┌─ ", Style::default().fg(color)),
        Span::styled(header, Style::default().fg(color)),
    ])];

    if !stdout.is_empty() {
        for line in stdout.lines().take(15) {
            lines.push(Line::from(vec![
                Span::styled(" │ ", Style::default().fg(color)),
                Span::raw(line.to_string()),
            ]));
        }
        if stdout.lines().count() > 15 {
            lines.push(Line::from(vec![
                Span::styled(" │ ", Style::default().fg(color)),
                Span::styled("...", Style::default().fg(theme.ui.footer_text)),
            ]));
        }
    }

    lines.push(Line::from(Span::styled(" └─", Style::default().fg(color))));
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
    fn test_finalize_assistant_caches_markdown() {
        let mut state = ChatState::new();
        state.handle_event(make_text_delta("**bold** text"));
        assert!(matches!(
            &state.messages[0],
            ChatMessage::Assistant {
                rendered_cache: None,
                ..
            }
        ));
        state.finalize_assistant();
        assert!(matches!(
            &state.messages[0],
            ChatMessage::Assistant {
                rendered_cache: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn test_render_lines_default_theme() {
        let state = ChatState::new();
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::default_dark();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(area, &renderers, &theme);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().contains("描述你的需求"));
    }

    #[test]
    fn test_render_lines_light_theme() {
        let state = ChatState::new();
        let renderers = ToolRendererRegistry::new();
        let theme = Theme::light();
        let area = Rect::new(0, 0, 80, 24);
        let lines = state.render_lines(area, &renderers, &theme);
        assert_eq!(lines.len(), 1);
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
        let lines = state.render_lines(area, &renderers, &theme);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        // 验证自定义渲染器输出包含路径信息
        assert!(combined.contains("src/main.rs"));
        // 验证状态图标
        assert!(combined.contains("✅"));
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
        let lines = state.render_lines(area, &renderers, &theme);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("编译失败"));
        assert!(combined.contains("✖"));
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
        let lines = state.render_lines(area, &renderers, &theme);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("阶段总结"));
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
        let lines = state.render_lines(area, &renderers, &theme);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("cargo test"));
        assert!(combined.contains("✅"));
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
        let lines = state.render_lines(area, &renderers, &theme);
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
        let lines = state.render_lines(area, &renderers, &theme);
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("分析 @Cargo.toml"));
    }
}
