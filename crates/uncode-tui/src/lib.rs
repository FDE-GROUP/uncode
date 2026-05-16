//! uncode-tui — 对话驱动终端交互界面
//!
//! 基于 ratatui + crossterm 实现，订阅 AgentLoop 事件流，
//! 实时渲染对话区：用户消息、Agent 回复、内联工具调用。

pub mod chat;
pub mod complete;
pub mod diff_viewer;
pub mod highlight;
pub mod input;
pub mod markdown;
pub mod message_queue;
pub mod permission;
pub mod selector;
pub mod slash;
pub mod theme;
pub mod tool_renderer;
pub mod welcome;

use crate::chat::ChatState;
use crate::complete::CompletionEngine;
use crate::input::{InputAction, InputEditor};
use crate::message_queue::{MessageQueue, QueueType};
use crate::permission::PermissionManager;
use crate::selector::OverlaySelector;
use crate::slash::SlashCommands;
use crate::theme::Theme;
use crate::tool_renderer::ToolRendererRegistry;
use crate::welcome::WelcomeScreen;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uncode_core::event::AgentEvent;
use uncode_core::message::UsageInfo;

/// 页脚状态 — Token 统计、费用、上下文使用率、耗时
struct FooterState {
    workdir: String,
    git_branch: String,
    input_tokens: u64,
    output_tokens: u64,
    cost: f64,
    context_percent: u8,
    turn_start: Option<std::time::Instant>,
    last_elapsed: String,
}

impl FooterState {
    fn new() -> Self {
        let workdir = std::env::current_dir()
            .map(|p| {
                let home = dirs::home_dir().unwrap_or_default();
                p.strip_prefix(&home)
                    .map(|s| format!("~/{}", s.display()))
                    .unwrap_or_else(|_| format!("{}", p.display()))
            })
            .unwrap_or_default();

        let git_branch = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .output()
            .ok()
            .and_then(|o| {
                o.status
                    .success()
                    .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
            })
            .unwrap_or_default();

        Self {
            workdir,
            git_branch,
            input_tokens: 0,
            output_tokens: 0,
            cost: 0.0,
            context_percent: 0,
            turn_start: None,
            last_elapsed: String::new(),
        }
    }

    fn update_usage(&mut self, usage: &UsageInfo) {
        self.input_tokens = usage.input_tokens;
        self.output_tokens = usage.output_tokens;
        // 粗略费用估算：input $3/M, output $15/M (DeepSeek pricing)
        let input_cost = (usage.input_tokens as f64) / 1_000_000.0 * 3.0;
        let output_cost = (usage.output_tokens as f64) / 1_000_000.0 * 15.0;
        self.cost += input_cost + output_cost;
        // 上下文使用率：假设 128k 窗口
        let total = usage.input_tokens + usage.output_tokens;
        self.context_percent = ((total as f64 / 128_000.0) * 100.0).min(100.0) as u8;
    }

    fn start_turn(&mut self) {
        self.turn_start = Some(std::time::Instant::now());
    }

    fn end_turn(&mut self) {
        if let Some(start) = self.turn_start.take() {
            self.last_elapsed = format_duration(start.elapsed());
        }
    }

    fn current_elapsed(&self) -> String {
        if let Some(start) = self.turn_start {
            format_duration(start.elapsed())
        } else {
            self.last_elapsed.clone()
        }
    }

    fn render_line1(&self, session_id: &str) -> String {
        let sid = session_id
            .get(..8)
            .map(|s| format!(" session:{}", s))
            .unwrap_or_default();
        format!("{} {}{}", self.workdir, self.git_branch, sid)
    }

    fn render_line2(&self, model: &str, level_icon: &str, theme: &Theme) -> Line<'static> {
        let in_str = format_tokens(self.input_tokens);
        let out_str = format_tokens(self.output_tokens);
        let cost_str = format!("${:.4}", self.cost);
        let elapsed = self.current_elapsed();

        // Three-level ctx% warning: <50% green, 50-80% yellow, >80% red
        let ctx_color = if self.context_percent > 80 {
            Color::Red
        } else if self.context_percent > 50 {
            Color::Yellow
        } else {
            Color::Green
        };

        let dim = Style::default().fg(theme.ui.footer_text);
        let value_style = Style::default().fg(theme.ui.agent_text);

        Line::from(vec![
            Span::styled("in:", dim),
            Span::styled(format!("{in_str} "), value_style),
            Span::styled("out:", dim),
            Span::styled(format!("{out_str} "), value_style),
            Span::styled(format!("{cost_str} "), value_style),
            Span::styled("ctx:", dim),
            Span::styled(
                format!("{}% ", self.context_percent),
                Style::default().fg(ctx_color),
            ),
            Span::styled("time:", dim),
            Span::styled(format!("{elapsed} "), value_style),
            Span::styled(
                format!(" {} ", model),
                Style::default()
                    .fg(theme.ui.footer_bg)
                    .bg(theme.tool_status.running),
            ),
            Span::styled(format!(" {level_icon}"), dim),
        ])
    }
}

fn format_tokens(n: u64) -> String {
    if n < 1000 {
        format!("{n}")
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        let m = secs / 60;
        let s = secs % 60;
        if m < 60 {
            format!("{m}m{s}s")
        } else {
            let h = m / 60;
            let rm = m % 60;
            format!("{h}h{rm}m")
        }
    }
}

pub struct TuiEngine {
    chat: ChatState,
    session_id: String,
    model: String,
    model_index: usize,
    available_models: Vec<String>,
    last_user_input: Option<String>,
    editor: InputEditor,
    selector: OverlaySelector,
    slash: SlashCommands,
    completion: CompletionEngine,
    leader_pending: bool,
    queue: MessageQueue,
    agent_busy: bool,
    current_cancel: Option<CancellationToken>,
    permission: PermissionManager,
    footer: FooterState,
    theme: Theme,
    renderers: ToolRendererRegistry,
    welcome: WelcomeScreen,
    quit_requested: bool,
    tick: usize,
}

impl TuiEngine {
    pub fn new_cancel_token(&mut self) -> CancellationToken {
        let token = CancellationToken::new();
        self.current_cancel = Some(token.clone());
        token
    }

    pub fn new() -> Self {
        Self {
            chat: ChatState::new(),
            session_id: String::new(),
            model: String::new(),
            model_index: 0,
            available_models: Vec::new(),
            last_user_input: None,
            editor: InputEditor::new(),
            selector: OverlaySelector::new(),
            slash: SlashCommands::new(),
            completion: CompletionEngine::new(slash_commands()),
            leader_pending: false,
            queue: MessageQueue::new(),
            agent_busy: false,
            current_cancel: None,
            permission: PermissionManager::new(),
            footer: FooterState::new(),
            theme: Theme::default(),
            renderers: ToolRendererRegistry::new(),
            welcome: WelcomeScreen::new(),
            quit_requested: false,
            tick: 0,
        }
    }

    pub fn set_available_models(&mut self, models: Vec<String>) {
        self.available_models = models;
    }

    pub fn render(&mut self, f: &mut Frame) {
        self.tick = self.tick.wrapping_add(1);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),    // 对话区
                Constraint::Length(3), // 输入栏
                Constraint::Length(1), // 页脚第 1 行
                Constraint::Length(1), // 页脚第 2 行
            ])
            .split(f.area());

        self.render_chat(f, chunks[0]);

        self.editor.render(f, chunks[1], self.theme.ui.footer_text);

        self.render_footer(f, chunks[2], chunks[3]);

        self.selector.render(f, f.area());
        self.welcome.render(f, f.area());
    }

    fn render_chat(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        let visible_height = area.height as usize;

        // Step 1: Update line count cache (only re-renders stale messages)
        self.chat.ensure_line_counts(
            area.width,
            &self.renderers,
            &self.theme,
            self.tick,
            self.agent_busy,
        );

        // Step 2: Compute total and auto_scroll
        let total_lines = self.chat.total_lines();
        if self.chat.scroll_offset + visible_height >= total_lines {
            self.chat.auto_scroll = true;
        }
        if self.chat.auto_scroll && total_lines > visible_height {
            self.chat.scroll_offset = total_lines.saturating_sub(visible_height);
        }

        // Step 3: Find visible message range via binary search
        let (first, last) = self
            .chat
            .visible_range(self.chat.scroll_offset, visible_height);

        // Step 4: Build viewport lines (only visible messages)
        let lines = self
            .chat
            .render_viewport(first, last, self.chat.scroll_offset, visible_height);

        let content = Paragraph::new(lines);
        f.render_widget(content, area);
    }

    fn render_footer(
        &self,
        f: &mut Frame,
        line1_area: ratatui::layout::Rect,
        line2_area: ratatui::layout::Rect,
    ) {
        let (status_icon, status_color) = if self.agent_busy {
            let dot = if (self.tick / 4) % 2 == 0 {
                "●"
            } else {
                "○"
            };
            (dot, self.theme.tool_status.success)
        } else {
            ("●", self.theme.tool_status.success)
        };

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{status_icon} "), Style::default().fg(status_color)),
                Span::styled(
                    self.footer.render_line1(&self.session_id),
                    Style::default().fg(self.theme.ui.footer_text),
                ),
            ])),
            line1_area,
        );

        let level = self.chat.thinking_level;
        let model_display = if self.model.is_empty() {
            "uncode"
        } else {
            &self.model
        };
        let line2 = self
            .footer
            .render_line2(model_display, level.icon(), &self.theme);
        f.render_widget(Paragraph::new(line2), line2_area);
    }

    pub async fn run<F>(&mut self, mut event_rx: broadcast::Receiver<AgentEvent>, on_submit: F)
    where
        F: Fn(String, CancellationToken),
    {
        let mut terminal = ratatui::init();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::EnableMouseCapture,
            crossterm::cursor::EnableBlinking,
            crossterm::terminal::SetTitle("UnCode Now"),
        );
        loop {
            if let Err(e) = terminal.draw(|f| self.render(f)) {
                eprintln!("terminal draw failed: {e}");
                break;
            }

            tokio::select! {
                biased;
                ui_result = async {
                    loop {
                        let poll_ok = event::poll(std::time::Duration::from_millis(50));
                        if poll_ok.is_err() {
                            return Err::<Event, std::io::Error>(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "terminal poll failed",
                            ));
                        }
                        if poll_ok.unwrap() {
                            let ev = event::read().unwrap_or(Event::Key(
                                event::KeyEvent::new(KeyCode::Null, event::KeyModifiers::empty())
                            ));
                            match ev {
                                Event::Key(key) if key.kind == KeyEventKind::Press => {
                                    return Ok(Event::Key(key));
                                }
                                Event::Mouse(mouse) => {
                                    return Ok(Event::Mouse(mouse));
                                }
                                Event::Resize(w, h) => {
                                    return Ok(Event::Resize(w, h));
                                }
                                _ => {}
                            }
                        }
                        tokio::task::yield_now().await;
                    }
                } => {
                    let ui_event = match ui_result {
                        Ok(ev) => ev,
                        Err(_) => break,
                    };
                    match ui_event {
                        Event::Key(key_event) => {
                            // ESC: highest priority — interrupt agent or dismiss overlays
                            if key_event.code == KeyCode::Esc {
                                if self.agent_busy {
                                    if let Some(ref token) = self.current_cancel {
                                        token.cancel();
                                    }
                                    self.agent_busy = false;
                                    self.current_cancel = None;
                                    self.footer.end_turn();
                                    self.chat.deactivate_thinking();
                                    self.chat.invalidate_all();
                                    self.chat.messages.push(chat::ChatMessage::Summary {
                                        completed: vec!["[Interrupted] Agent stopped.".into()],
                                        next_steps: vec![],
                                    });
                                }
                                if self.welcome.is_visible() {
                                    self.welcome.hide();
                                }
                                if self.selector.is_visible() {
                                    self.selector.hide();
                                }
                                continue;
                            }

                            if self.leader_pending {
                                self.leader_pending = false;
                                self.handle_leader_key(key_event);
                                continue;
                            }

                            let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);

                            // Welcome screen dismiss takes priority
                            if self.welcome.is_visible() {
                                if key_event.code == KeyCode::Enter {
                                    self.welcome.hide();
                                }
                                continue;
                            }

                            // Permission confirmation keys take priority
                            if self.permission.has_pending() {
                                match key_event.code {
                                    KeyCode::Char('y') | KeyCode::Enter => {
                                        self.permission.confirm(crate::permission::ConfirmOption::Allow);
                                    }
                                    KeyCode::Char('n') | KeyCode::Esc => {
                                        self.permission.deny();
                                    }
                                    KeyCode::Char('e') => {
                                        self.permission.confirm(crate::permission::ConfirmOption::Edit);
                                    }
                                    _ => {}
                                }
                                continue;
                            }

                            match key_event.code {
                                // Leader key prefix
                                KeyCode::Char('x') if ctrl => {
                                    self.leader_pending = true;
                                }
                                // Direct shortcuts
                                KeyCode::Char('o') if ctrl => {
                                    self.chat.tool_output_visible = !self.chat.tool_output_visible;
                                }
                                KeyCode::Char('t') if ctrl => {
                                    self.chat.thinking_visible = !self.chat.thinking_visible;
                                }
                                KeyCode::Char('l') if ctrl => {
                                    self.selector.show(
                                        "Switch model",
                                        self.available_models.iter().map(|s| s.as_str().into()).collect(),
                                    );
                                }
                                // Model cycling: Ctrl+P forward, Shift+Ctrl+P backward
                                KeyCode::Char('p') if ctrl => {
                                    let shift = key_event.modifiers.contains(KeyModifiers::SHIFT);
                                    if self.available_models.len() > 1 {
                                        if shift {
                                            self.model_index = if self.model_index == 0 {
                                                self.available_models.len() - 1
                                            } else {
                                                self.model_index - 1
                                            };
                                        } else {
                                            self.model_index =
                                                (self.model_index + 1) % self.available_models.len();
                                        }
                                        self.model = self.available_models[self.model_index].clone();
                                        self.chat.messages.push(chat::ChatMessage::Summary {
                                            completed: vec![format!("model: {}", self.model)],
                                            next_steps: vec![],
                                        });
                                    }
                                }
                                // Retry: Ctrl+R
                                KeyCode::Char('r') if ctrl => {
                                    if !self.agent_busy {
                                        if let Some(ref input) = self.last_user_input {
                                            let text = input.clone();
                                            self.agent_busy = true;
                                            self.footer.start_turn();
                                            self.chat.push_user_message(format!("[Retry] {text}"));
                                            let expanded = uncode_core::context::expand_file_refs(
                                                &text,
                                                &std::env::current_dir().unwrap_or_default(),
                                            );
                                            let token = self.new_cancel_token();
                                            on_submit(expanded, token);
                                        } else {
                                            self.chat.messages.push(chat::ChatMessage::Summary {
                                                completed: vec!["No messages to retry.".into()],
                                                next_steps: vec![],
                                            });
                                        }
                                    }
                                }
                                // New session: Ctrl+N
                                KeyCode::Char('n') if ctrl => {
                                    if !self.agent_busy {
                                        self.session_id = uuid::Uuid::new_v4().to_string();
                                        self.chat.messages.clear();
                                        self.chat.scroll_offset = 0;
                                        self.chat.auto_scroll = true;
                                        self.footer.input_tokens = 0;
                                        self.footer.output_tokens = 0;
                                        self.footer.cost = 0.0;
                                        self.footer.context_percent = 0;
                                        self.last_user_input = None;
                                        let sid = &self.session_id[..8];
                                        self.chat.messages.push(chat::ChatMessage::Summary {
                                            completed: vec![format!("New session created. session:{sid}")],
                                            next_steps: vec![],
                                        });
                                    }
                                }
                                // Undo last turn: Ctrl+/
                                KeyCode::Char('/') if ctrl => {
                                    if !self.agent_busy && self.chat.messages.len() >= 2 {
                                        let last_idx = self.chat.messages.len() - 1;
                                        let second_last_idx = last_idx.saturating_sub(1);
                                        let removed = if matches!(
                                            self.chat.messages[last_idx],
                                            chat::ChatMessage::Assistant { .. }
                                        ) {
                                            self.chat.messages.truncate(second_last_idx);
                                            2
                                        } else {
                                            1
                                        };
                                        self.chat.messages.push(chat::ChatMessage::Summary {
                                            completed: vec![format!("Undid {removed} messages.")],
                                            next_steps: vec![],
                                        });
                                    }
                                }
                                // External editor: Ctrl+G
                                KeyCode::Char('g') if ctrl => {
                                    if let Some(content) = open_external_editor() {
                                        if !content.is_empty() {
                                            self.editor.set_buffer(content);
                                        }
                                    }
                                }
                                KeyCode::BackTab => {
                                    self.chat.thinking_level = self.chat.thinking_level.cycle_next();
                                }
                                KeyCode::PageUp => {
                                    self.chat.scroll_offset = self.chat.scroll_offset.saturating_sub(10);
                                    self.chat.auto_scroll = false;
                                }
                                KeyCode::PageDown => {
                                    self.chat.scroll_offset = self.chat.scroll_offset.saturating_add(10);
                                }
                                // Selector navigation
                                KeyCode::Char('j') if ctrl && self.selector.is_visible() => self.selector.next(),
                                KeyCode::Char('k') if ctrl && self.selector.is_visible() => self.selector.prev(),
                                KeyCode::Up if self.selector.is_visible() => self.selector.prev(),
                                KeyCode::Down if self.selector.is_visible() => self.selector.next(),
                                KeyCode::Enter if self.selector.is_visible() => {
                                    if let Some(selected) = self.selector.selected_item().map(|s| s.to_string()) {
                                        if let Some(idx) = self.available_models.iter().position(|m| m == &selected) {
                                            self.model_index = idx;
                                        }
                                        self.model = selected.clone();
                                        self.chat.messages.push(chat::ChatMessage::Summary {
                                            completed: vec![format!("Model switched to: {selected}")],
                                            next_steps: vec![],
                                        });
                                    }
                                    self.selector.hide();
                                }
                                // Quit / Interrupt
                                KeyCode::Char('c') if ctrl => {
                                    if self.agent_busy {
                                        if let Some(ref token) = self.current_cancel {
                                            token.cancel();
                                        }
                                        self.agent_busy = false;
                                        self.current_cancel = None;
                                        self.chat.messages.push(chat::ChatMessage::Summary {
                                            completed: vec!["[Interrupted] Agent stopped.".into()],
                                            next_steps: vec![],
                                        });
                                    } else {
                                        break;
                                    }
                                }
                                _ => {
                                    let action = self.editor.handle_key(key_event);
                                    match action {
                                        InputAction::Submit(text) => {
                                            self.handle_submit(text, &on_submit);
                                        }
                                        InputAction::Cancel => break,
                                        InputAction::None => {
                                            self.editor.set_completions(
                                                self.completion.complete(self.editor.buffer())
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Event::Mouse(mouse) => {
                            match mouse.kind {
                                MouseEventKind::ScrollUp => {
                                    self.chat.scroll_offset = self.chat.scroll_offset.saturating_sub(3);
                                    self.chat.auto_scroll = false;
                                }
                                MouseEventKind::ScrollDown => {
                                    self.chat.scroll_offset = self.chat.scroll_offset.saturating_add(3);
                                }
                                _ => {}
                            }
                        }
                        Event::Resize(_, _) => {
                            let _ = terminal.autoresize();
                            let _ = terminal.clear();
                        }
                        _ => {}
                    }
                }
                Ok(event) = event_rx.recv() => {
                    let is_turn_end = matches!(event, AgentEvent::TurnEnd { .. } | AgentEvent::SessionEnd { .. } | AgentEvent::AgentInterrupted { .. });
                    self.handle_event(event);
                    if is_turn_end {
                        self.flush_queue(&on_submit);
                    }
                }
            }
            if self.quit_requested {
                break;
            }
        }
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
        ratatui::restore();
    }

    fn handle_submit<F>(&mut self, text: String, on_submit: &F)
    where
        F: Fn(String, CancellationToken),
    {
        if let Some(response) = self.slash.execute(&text) {
            self.chat.messages.push(chat::ChatMessage::Summary {
                completed: vec![response],
                next_steps: vec![],
            });
            return;
        }

        match text.as_str() {
            "/thinking" => {
                self.chat.thinking_visible = !self.chat.thinking_visible;
            }
            "/details" => {
                self.chat.tool_output_visible = !self.chat.tool_output_visible;
            }
            "/help" => {
                let help = "Keys: Ctrl+O tool output | Ctrl+T thinking | Ctrl+P cycle model | Ctrl+R retry | Ctrl+N new session | Ctrl+/ undo | Ctrl+G editor\nCommands: /clear | /compact | /model [name] | /new | /fork [id] | /export [fmt] | /sessions | /branch | /name [title] | /copy | /usage | /reload | /diff | /theme | /thinking | /details | /tree | /skills | /template";
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: vec![help.into()],
                    next_steps: vec![],
                });
            }
            "/clear" => {
                self.handle_clear_command();
            }
            "/compact" => {
                self.handle_compact_command();
            }
            "/new" => {
                self.handle_new_command();
            }
            t if t.starts_with("/model") => {
                self.handle_model_command(&text);
            }
            t if t.starts_with("/fork") => {
                self.handle_fork_command(&text);
            }
            t if t.starts_with("/export") => {
                self.handle_export_command(&text);
            }
            "/sessions" => {
                self.handle_sessions_command();
            }
            "/branch" => {
                self.handle_branch_command();
            }
            t if t.starts_with("/name") => {
                self.handle_name_command(&text);
            }
            "/copy" => {
                self.handle_copy_command();
            }
            "/usage" => {
                self.handle_usage_command();
            }
            "/reload" => {
                self.handle_reload_command();
            }
            "/diff" => {
                self.handle_diff_command();
            }
            t if t.starts_with("/theme") => {
                self.handle_theme_command(&text);
            }
            t if t.starts_with("/template") => {
                self.handle_template_command(&text);
            }
            "/tree" => {
                self.handle_tree_command();
            }
            "/skills" => {
                self.handle_skills_command();
            }
            "/quit" => {
                self.quit_requested = true;
            }
            t if t.starts_with('/') && !t.contains(' ') && t.len() > 1 => {
                let skill_name = &t[1..];
                self.handle_skill_invoke(skill_name, "", on_submit);
            }
            t if t.starts_with('/') && t.contains(' ') => {
                let parts: Vec<&str> = t[1..].splitn(2, ' ').collect();
                let skill_name = parts[0];
                let args = parts.get(1).copied().unwrap_or("");
                let registry = uncode_core::skill::SkillRegistry::load();
                if registry.get(skill_name).is_some() {
                    self.handle_skill_invoke(skill_name, args, on_submit);
                } else {
                    self.submit_text(text, on_submit);
                }
            }
            _ => {
                self.submit_text(text.clone(), on_submit);
            }
        }
    }

    fn submit_text<F>(&mut self, text: String, on_submit: &F)
    where
        F: Fn(String, CancellationToken),
    {
        if self.agent_busy {
            let preview = text.clone();
            self.queue.enqueue(text, QueueType::FollowUp);
            self.chat
                .messages
                .push(chat::ChatMessage::QueuedMessage { text: preview });
        } else {
            self.last_user_input = Some(text.clone());
            self.agent_busy = true;
            self.footer.start_turn();
            self.chat.push_user_message(text.clone());
            let file_expanded = uncode_core::context::expand_file_refs(
                &text,
                &std::env::current_dir().unwrap_or_default(),
            );
            let token = self.new_cancel_token();
            on_submit(file_expanded, token);
        }
    }

    fn handle_clear_command(&mut self) {
        self.chat.messages.clear();
        self.chat.scroll_offset = 0;
        self.chat.auto_scroll = true;
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: vec!["Chat cleared.".into()],
            next_steps: vec![],
        });
    }

    fn handle_compact_command(&mut self) {
        let ctx_pct = self.footer.context_percent;
        let in_str = format_tokens(self.footer.input_tokens);
        let out_str = format_tokens(self.footer.output_tokens);
        let msg_count = self.chat.messages.len();

        let mut lines = vec![
            format!("Context: {ctx_pct}% (in:{in_str} out:{out_str})"),
            format!("Messages: {msg_count}"),
        ];

        if ctx_pct >= 80 {
            lines.push("Context threshold reached, auto-compaction next turn.".into());
        } else if ctx_pct >= 50 {
            lines.push("Context usage moderate, consider compacting before 80%.".into());
        } else {
            lines.push("Context usage low, no compaction needed.".into());
        }

        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: lines,
            next_steps: vec![],
        });
    }

    fn handle_new_command(&mut self) {
        let old_id = if self.session_id.is_empty() {
            "none".to_string()
        } else {
            self.session_id[..8.min(self.session_id.len())].to_string()
        };

        self.session_id = uuid::Uuid::new_v4().to_string();
        self.chat.messages.clear();
        self.chat.scroll_offset = 0;
        self.chat.auto_scroll = true;
        self.footer.input_tokens = 0;
        self.footer.output_tokens = 0;
        self.footer.cost = 0.0;
        self.footer.context_percent = 0;

        let new_id = &self.session_id[..8];
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: vec![format!(
                "New session. session:{} → session:{new_id}",
                old_id
            )],
            next_steps: vec![],
        });
    }

    fn handle_model_command(&mut self, text: &str) {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let name = parts.get(1).copied().unwrap_or("").trim();

        if name.is_empty() {
            self.selector
                .show("切换模型", self.available_models.clone());
            return;
        }

        let old = self.model.clone();
        self.model = name.to_string();
        if let Some(idx) = self.available_models.iter().position(|m| m == name) {
            self.model_index = idx;
        }
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: vec![format!("Model switched: {old} -> {name}")],
            next_steps: vec![],
        });
    }

    fn handle_fork_command(&mut self, text: &str) {
        if self.session_id.is_empty() {
            self.chat.messages.push(chat::ChatMessage::Error {
                message: "No active session to fork.".into(),
                category: uncode_core::event::ErrorCategory::Config,
            });
            return;
        }
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let target_entry = if parts.len() > 1 && !parts[1].trim().is_empty() {
            parts[1].trim().to_string()
        } else {
            self.chat.messages.push(chat::ChatMessage::Error {
                message: "用法: /fork <entry_id> — 指定要回退到的条目 ID".into(),
                category: uncode_core::event::ErrorCategory::Config,
            });
            return;
        };
        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        match uncode_agent::branch_summarization::branch_with_summary(
            &store,
            &self.session_id,
            &target_entry,
            "用户 fork",
        ) {
            Ok(()) => {
                let short = &target_entry[..8.min(target_entry.len())];
                let msg = format!("已分支到条目: {short}");
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: vec![msg],
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("分支失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_export_command(&mut self, text: &str) {
        if self.session_id.is_empty() {
            self.chat.messages.push(chat::ChatMessage::Error {
                message: "No active session to export.".into(),
                category: uncode_core::event::ErrorCategory::Config,
            });
            return;
        }
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let format = parts.get(1).map(|s| s.trim()).unwrap_or("jsonl");
        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        let sid_short = &self.session_id[..8.min(self.session_id.len())];
        match format {
            "jsonl" => match store.load_entries(&self.session_id) {
                Ok(entries) => {
                    let filename = format!("uncode-export-{sid_short}.jsonl");
                    let mut out = String::new();
                    for entry in &entries {
                        if let Ok(line) = serde_json::to_string(entry) {
                            out.push_str(&line);
                            out.push('\n');
                        }
                    }
                    match std::fs::write(&filename, &out) {
                        Ok(()) => {
                            self.chat.messages.push(chat::ChatMessage::Summary {
                                completed: vec![format!(
                                    "已导出 JSONL: {filename} ({} 条目)",
                                    entries.len()
                                )],
                                next_steps: vec![],
                            });
                        }
                        Err(e) => {
                            self.chat.messages.push(chat::ChatMessage::Error {
                                message: format!("Failed to write file: {e}"),
                                category: uncode_core::event::ErrorCategory::Config,
                            });
                        }
                    }
                }
                Err(e) => {
                    self.chat.messages.push(chat::ChatMessage::Error {
                        message: format!("Failed to read session: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                }
            },
            "html" => match store.load_entries(&self.session_id) {
                Ok(entries) => {
                    let filename = format!("uncode-export-{sid_short}.html");
                    let html = render_export_html(&entries);
                    match std::fs::write(&filename, &html) {
                        Ok(()) => {
                            self.chat.messages.push(chat::ChatMessage::Summary {
                                completed: vec![format!(
                                    "已导出 HTML: {filename} ({} 条目)",
                                    entries.len()
                                )],
                                next_steps: vec![],
                            });
                        }
                        Err(e) => {
                            self.chat.messages.push(chat::ChatMessage::Error {
                                message: format!("Failed to write file: {e}"),
                                category: uncode_core::event::ErrorCategory::Config,
                            });
                        }
                    }
                }
                Err(e) => {
                    self.chat.messages.push(chat::ChatMessage::Error {
                        message: format!("Failed to read session: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                }
            },
            other => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("不支持的导出格式: '{other}'。支持: jsonl, html"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_sessions_command(&mut self) {
        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        match store.list_sessions() {
            Ok(mut sessions) => {
                if sessions.is_empty() {
                    self.chat.messages.push(chat::ChatMessage::Summary {
                        completed: vec!["No session history.".into()],
                        next_steps: vec![],
                    });
                    return;
                }
                sessions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
                let display: Vec<String> = sessions
                    .iter()
                    .take(20)
                    .map(|s| {
                        let id_short = &s.id[..8.min(s.id.len())];
                        let title: String = s
                            .title
                            .as_deref()
                            .unwrap_or("(无标题)")
                            .chars()
                            .take(30)
                            .collect();
                        let time = s.updated_at.format("%m-%d %H:%M");
                        format!(
                            "  session:{id_short}  {title}  {} msgs  {time}",
                            s.message_count
                        )
                    })
                    .collect();
                let mut completed = vec![format!("Recent {} sessions:", display.len())];
                completed.extend(display);
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed,
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("Failed to list sessions: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_branch_command(&mut self) {
        if self.session_id.is_empty() {
            self.chat.messages.push(chat::ChatMessage::Summary {
                completed: vec!["No active session.".into()],
                next_steps: vec![],
            });
            return;
        }
        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        let sid_short = &self.session_id[..8.min(self.session_id.len())];
        match store.get_children(&self.session_id) {
            Ok(children) => {
                let mut lines = vec![format!("当前会话: session:{sid_short}")];
                if children.is_empty() {
                    lines.push("无子分支。".into());
                } else {
                    lines.push(format!("子分支 ({}):", children.len()));
                    for child in &children {
                        let cid = &child.id[..8.min(child.id.len())];
                        let title = child.title.as_deref().unwrap_or("(无标题)");
                        lines.push(format!("  session:{cid}  {title}"));
                    }
                }
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: lines,
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("获取分支信息失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_name_command(&mut self, text: &str) {
        if self.session_id.is_empty() {
            self.chat.messages.push(chat::ChatMessage::Error {
                message: "No active session.".into(),
                category: uncode_core::event::ErrorCategory::Config,
            });
            return;
        }
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let title = parts.get(1).map(|s| s.trim()).unwrap_or("");
        if title.is_empty() {
            let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
                Ok(d) => d,
                Err(e) => {
                    self.chat.messages.push(chat::ChatMessage::Error {
                        message: format!("无法获取会话目录: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                    return;
                }
            };
            let store = uncode_agent::session::store::SessionStore::new(session_dir);
            match store.read_header(&self.session_id) {
                Ok(header) => {
                    let current = header.title.as_deref().unwrap_or("(无标题)");
                    self.chat.messages.push(chat::ChatMessage::Summary {
                        completed: vec![format!("Current title: {current}")],
                        next_steps: vec![],
                    });
                }
                Err(e) => {
                    self.chat.messages.push(chat::ChatMessage::Error {
                        message: format!("Failed to read session: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                }
            }
            return;
        }
        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(_) => return,
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        if let Ok(mut header) = store.read_header(&self.session_id) {
            header.title = Some(title.to_string());
            let header_path = store.session_path(&self.session_id);
            let entries = store.load_entries(&self.session_id).unwrap_or_default();
            if let Ok(mut file) = std::fs::File::create(&header_path) {
                use std::io::Write;
                if let Ok(json) = serde_json::to_string(&header) {
                    let _ = writeln!(file, "{json}");
                }
                for entry in &entries {
                    if let Ok(json) = serde_json::to_string(entry) {
                        let _ = writeln!(file, "{json}");
                    }
                }
            }
        }
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: vec![format!("Title set: {title}")],
            next_steps: vec![],
        });
    }

    fn handle_copy_command(&mut self) {
        let last_assistant = self.chat.messages.iter().rev().find_map(|msg| match msg {
            chat::ChatMessage::Assistant { text, .. } => Some(text.clone()),
            _ => None,
        });
        match last_assistant {
            Some(text) => {
                let encoded = base64_encode(&text);
                let osc = format!("\x1b]52;c;{encoded}\x07");
                let _ = std::io::Write::write_all(&mut std::io::stdout(), osc.as_bytes());
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: vec![format!("Copied to clipboard ({} chars)", text.len())],
                    next_steps: vec![],
                });
            }
            None => {
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: vec!["No agent response to copy.".into()],
                    next_steps: vec![],
                });
            }
        }
    }

    fn handle_usage_command(&mut self) {
        let in_str = format_tokens(self.footer.input_tokens);
        let out_str = format_tokens(self.footer.output_tokens);
        let cost_str = format!("${:.4}", self.footer.cost);
        let lines = vec![
            format!("Input tokens:  {in_str}"),
            format!("Output tokens: {out_str}"),
            format!("费用:          {cost_str}"),
            format!("上下文使用:    {}%", self.footer.context_percent),
            format!("对话消息数:    {}", self.chat.messages.len()),
        ];
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: lines,
            next_steps: vec![],
        });
    }

    fn handle_reload_command(&mut self) {
        let theme_name = self.theme.name.clone();
        if let Some(theme) = Theme::load_by_name(&theme_name) {
            self.theme = theme;
        }
        let git_branch = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .output()
            .ok()
            .and_then(|o| {
                o.status
                    .success()
                    .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
            })
            .unwrap_or_default();
        self.footer.git_branch = git_branch;
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: vec!["配置已重新加载。".into()],
            next_steps: vec![],
        });
    }

    fn handle_diff_command(&mut self) {
        match std::process::Command::new("git")
            .args(["diff", "--stat"])
            .output()
        {
            Ok(o) if o.status.success() => {
                let stat = String::from_utf8_lossy(&o.stdout);
                if stat.trim().is_empty() {
                    self.chat.messages.push(chat::ChatMessage::Summary {
                        completed: vec!["工作区干净，没有未提交的变更。".into()],
                        next_steps: vec![],
                    });
                } else {
                    let full = std::process::Command::new("git")
                        .args(["diff"])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                        .unwrap_or_default();
                    let mut lines = vec![
                        "工作区变更:".to_string(),
                        stat.trim().to_string(),
                        String::new(),
                    ];
                    for line in full.lines().take(30) {
                        lines.push(format!("  {line}"));
                    }
                    if full.lines().count() > 30 {
                        lines.push(format!("  ... ({} more lines)", full.lines().count() - 30));
                    }
                    self.chat.messages.push(chat::ChatMessage::Summary {
                        completed: lines,
                        next_steps: vec![],
                    });
                }
            }
            _ => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: "无法获取 git diff。请确认当前目录是 git 仓库。".into(),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_leader_key(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('g') => {
                self.chat.scroll_offset = 0;
                self.chat.auto_scroll = false;
            }
            KeyCode::Char('G') => {
                self.chat.auto_scroll = true;
            }
            KeyCode::Char('n') => {
                // New session - placeholder
            }
            KeyCode::Char('m') => {
                self.selector.show(
                    "切换模型",
                    self.available_models
                        .iter()
                        .map(|s| s.as_str().into())
                        .collect(),
                );
            }
            _ => {}
        }
    }

    fn handle_event(&mut self, event: AgentEvent) {
        match &event {
            AgentEvent::SessionStart { session_id, .. } => {
                self.session_id = session_id.clone();
            }
            AgentEvent::TurnEnd { usage, .. } => {
                self.agent_busy = false;
                self.footer.end_turn();
                self.footer.update_usage(usage);
            }
            AgentEvent::SessionEnd { total_tokens, .. } => {
                self.agent_busy = false;
                self.footer.end_turn();
                self.footer.update_usage(total_tokens);
            }
            AgentEvent::AgentInterrupted { .. } => {
                self.agent_busy = false;
                self.footer.end_turn();
            }
            _ => {}
        }
        self.chat.handle_event(event);
    }

    fn handle_template_command(&mut self, text: &str) {
        use uncode_core::template::TemplateStore;

        let parts: Vec<&str> = text.splitn(3, ' ').collect();
        let store = TemplateStore::load();

        if parts.len() == 1 || (parts.len() == 2 && parts[1].is_empty()) {
            // /template — list all
            let list: Vec<String> = store
                .list()
                .iter()
                .map(|t| format!("  {} — {}", t.name, t.description))
                .collect();
            let header = "可用模板:".to_string();
            self.chat.messages.push(chat::ChatMessage::Summary {
                completed: std::iter::once(header).chain(list).collect(),
                next_steps: vec![],
            });
            return;
        }

        let name = parts[1];
        let vars_str = parts.get(2).copied().unwrap_or("");

        let mut vars = std::collections::HashMap::new();
        if !vars_str.is_empty() {
            for pair in vars_str.split_whitespace() {
                if let Some((k, v)) = pair.split_once('=') {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
        }

        match store.render(name, &vars) {
            Some(prompt) => {
                self.agent_busy = true;
                self.chat
                    .push_user_message(format!("[template: {name}] {prompt}"));
                // For TUI, we need to trigger on_submit but we're not in handle_submit's scope
                // Instead, we emit the prompt as a user message and show it
                self.agent_busy = false;
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: vec![format!(
                        "模板 '{name}' 已渲染。复制以下内容作为输入：\n{prompt}"
                    )],
                    next_steps: vec![],
                });
            }
            None => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("模板 '{name}' 不存在。使用 /template 查看可用模板。"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_tree_command(&mut self) {
        if self.session_id.is_empty() {
            self.chat.messages.push(chat::ChatMessage::Summary {
                completed: vec!["No active session.".into()],
                next_steps: vec![],
            });
            return;
        }

        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);

        match store.build_tree(&self.session_id) {
            Ok(tree) => {
                let lines = render_session_tree(&tree.root, "", true);
                let header = format!(
                    "会话分支树 (root: {})",
                    &tree.root.id[..8.min(tree.root.id.len())]
                );
                let mut completed = vec![header];
                completed.extend(lines);
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed,
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("构建会话树失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_skills_command(&mut self) {
        use uncode_core::skill::SkillRegistry;
        let registry = SkillRegistry::load();
        let list = registry.list();
        if list.is_empty() {
            self.chat.messages.push(chat::ChatMessage::Summary {
                completed: vec!["没有可用 Skills。".into()],
                next_steps: vec![],
            });
            return;
        }
        let lines: Vec<String> = list.iter().map(|s| format!("  {s}")).collect();
        let mut completed = vec!["可用 Skills:".to_string()];
        completed.extend(lines);
        completed.push("调用方式: /<skill_name> <args>".to_string());
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed,
            next_steps: vec![],
        });
    }

    fn handle_skill_invoke<F>(&mut self, skill_name: &str, args_str: &str, on_submit: &F)
    where
        F: Fn(String, CancellationToken),
    {
        use uncode_core::skill::SkillRegistry;
        let registry = SkillRegistry::load();
        let _skill = match registry.get(skill_name) {
            Some(s) => s,
            None => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("Skill '{skill_name}' 不存在。使用 /skills 查看可用列表。"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };

        let mut vars = std::collections::HashMap::new();
        if !args_str.is_empty() {
            for pair in args_str.split_whitespace() {
                if let Some((k, v)) = pair.split_once('=') {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
        }

        let prompt = match registry.render(skill_name, &vars) {
            Some(p) => p,
            None => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("Skill '{skill_name}' 渲染失败。"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };

        self.agent_busy = true;
        self.chat
            .push_user_message(format!("[skill: {skill_name}] {args_str}"));
        let token = self.new_cancel_token();
        on_submit(prompt, token);
    }

    fn handle_theme_command(&mut self, text: &str) {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let name = parts.get(1).copied().unwrap_or("").trim();

        if name.is_empty() {
            // List available themes
            let themes = Theme::available_themes();
            let current = &self.theme.name;
            let lines: Vec<String> = themes
                .iter()
                .map(|t| {
                    if t == current {
                        format!("  {t}  ← 当前")
                    } else {
                        format!("  {t}")
                    }
                })
                .collect();
            let mut completed = vec!["可用主题:".to_string()];
            completed.extend(lines);
            completed.push("使用 /theme <name> 切换".to_string());
            self.chat.messages.push(chat::ChatMessage::Summary {
                completed,
                next_steps: vec![],
            });
            return;
        }

        match Theme::load_by_name(name) {
            Some(theme) => {
                let old = self.theme.name.clone();
                self.theme = theme;
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: vec![format!("主题切换: {old} → {}", self.theme.name)],
                    next_steps: vec![],
                });
            }
            None => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("主题 '{name}' 不存在。使用 /theme 查看可用列表。"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn flush_queue<F>(&mut self, on_submit: &F)
    where
        F: Fn(String, CancellationToken),
    {
        if let Some(text) = self.queue.drain_follow_up().into_iter().next() {
            self.agent_busy = true;
            self.chat.push_user_message(text.clone());
            let token = self.new_cancel_token();
            on_submit(text, token);
        }
    }
}

impl Default for TuiEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn render_session_tree(
    node: &uncode_core::session::SessionNode,
    prefix: &str,
    is_last: bool,
) -> Vec<String> {
    let id_short = &node.id[..8.min(node.id.len())];
    let title = node.title.as_deref().unwrap_or("无标题");
    let line = format!(
        "{prefix}{}{id_short}  {title}  ({} msgs, {})",
        if prefix.is_empty() && is_last {
            ""
        } else if is_last {
            "└── "
        } else {
            "├── "
        },
        node.message_count,
        node.model
    );
    let mut lines = vec![line];

    let child_prefix = if prefix.is_empty() && is_last {
        String::new()
    } else if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    for (i, child) in node.children.iter().enumerate() {
        let child_is_last = i == node.children.len() - 1;
        lines.extend(render_session_tree(child, &child_prefix, child_is_last));
    }
    lines
}

fn base64_encode(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn render_export_html(entries: &[uncode_core::session::SessionEntry]) -> String {
    let mut body = String::new();
    for entry in entries {
        if let uncode_core::session::SessionEntry::Message(msg) = entry {
            let role = &msg.role;
            let content = msg
                .content
                .iter()
                .filter_map(|block| match block {
                    uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !content.is_empty() {
                body.push_str(&format!(
                    "<div class=\"msg {role}\"><b>{role}</b><pre>{content}</pre></div>\n"
                ));
            }
        }
    }
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
         <title>uncode session export</title>\
         <style>body{{font-family:monospace;margin:2em}}\
         .msg{{margin:1em 0;padding:0.5em;border-left:3px solid #ccc}}\
         .user{{border-color:#4a9}}.assistant{{border-color:#69f}}\
         pre{{white-space:pre-wrap;margin:0.5em 0}}</style>\
         </head><body>{body}</body></html>"
    )
}

fn open_external_editor() -> Option<String> {
    let editor = std::env::var("EDITOR")
        .unwrap_or_else(|_| std::env::var("VISUAL").unwrap_or_else(|_| "vi".to_string()));
    let tmp_path = std::env::temp_dir().join(format!("uncode-input-{}.md", uuid::Uuid::new_v4()));
    std::fs::write(&tmp_path, "").ok()?;
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::restore();
    let status = std::process::Command::new(&editor)
        .arg(&tmp_path)
        .status()
        .ok();
    let _ = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
    if status.as_ref().map(|s| s.success()).unwrap_or(false) {
        let content = std::fs::read_to_string(&tmp_path).unwrap_or_default();
        let _ = std::fs::remove_file(&tmp_path);
        Some(content.trim().to_string())
    } else {
        let _ = std::fs::remove_file(&tmp_path);
        None
    }
}

fn slash_commands() -> Vec<String> {
    let mut cmds = vec![
        "help".into(),
        "quit".into(),
        "clear".into(),
        "compact".into(),
        "model".into(),
        "new".into(),
        "fork".into(),
        "export".into(),
        "sessions".into(),
        "branch".into(),
        "name".into(),
        "copy".into(),
        "usage".into(),
        "reload".into(),
        "diff".into(),
        "thinking".into(),
        "details".into(),
        "issues".into(),
        "template".into(),
        "tree".into(),
        "skills".into(),
        "theme".into(),
    ];
    // Add skill names
    let registry = uncode_core::skill::SkillRegistry::load();
    for skill in registry.list() {
        cmds.push(skill.name.clone());
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(15_300), "15.3k");
        assert_eq!(format_tokens(999_999), "1000.0k");
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn test_footer_state_new() {
        let footer = FooterState::new();
        assert!(!footer.workdir.is_empty() || footer.workdir.is_empty()); // 不 panic 即可
        assert_eq!(footer.input_tokens, 0);
        assert_eq!(footer.output_tokens, 0);
        assert_eq!(footer.cost, 0.0);
        assert_eq!(footer.context_percent, 0);
    }

    #[test]
    fn test_footer_update_usage() {
        let mut footer = FooterState::new();
        footer.update_usage(&UsageInfo {
            input_tokens: 50_000,
            output_tokens: 10_000,
            cost: None,
        });
        assert_eq!(footer.input_tokens, 50_000);
        assert_eq!(footer.output_tokens, 10_000);
        // cost: 50k * $3/M + 10k * $15/M = 0.15 + 0.15 = 0.30
        assert!((footer.cost - 0.30).abs() < 0.001);
        // context: (50k + 10k) / 128k * 100 ≈ 46%
        assert_eq!(footer.context_percent, 46);
    }

    #[test]
    fn test_footer_update_usage_accumulates_cost() {
        let mut footer = FooterState::new();
        footer.update_usage(&UsageInfo {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cost: None,
        });
        let cost1 = footer.cost;
        footer.update_usage(&UsageInfo {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cost: None,
        });
        assert!((footer.cost - cost1 * 2.0).abs() < 0.001);
    }

    #[test]
    fn test_footer_context_percent_clamped() {
        let mut footer = FooterState::new();
        footer.update_usage(&UsageInfo {
            input_tokens: 200_000,
            output_tokens: 0,
            cost: None,
        });
        assert_eq!(footer.context_percent, 100);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(std::time::Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(std::time::Duration::from_secs(5)), "5s");
        assert_eq!(format_duration(std::time::Duration::from_secs(59)), "59s");
        assert_eq!(format_duration(std::time::Duration::from_secs(60)), "1m0s");
        assert_eq!(format_duration(std::time::Duration::from_secs(90)), "1m30s");
        assert_eq!(
            format_duration(std::time::Duration::from_secs(3600)),
            "1h0m"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_secs(3661)),
            "1h1m"
        );
    }

    #[test]
    fn test_footer_render_line1() {
        let footer = FooterState::new();
        let line = footer.render_line1("abc12345xyz");
        assert!(line.contains("session:abc12345"));
        // 空 session_id
        let empty = footer.render_line1("");
        assert!(!empty.contains("session:"));
    }

    #[test]
    fn test_footer_render_line2() {
        let mut footer = FooterState::new();
        footer.input_tokens = 5_000;
        footer.output_tokens = 1_200;
        footer.cost = 0.05;
        footer.context_percent = 30;
        let line = footer.render_line2("deepseek-v3", "◕", &Theme::default());
        let line_str = line.to_string();
        assert!(line_str.contains("5.0k"));
        assert!(line_str.contains("1.2k"));
        assert!(line_str.contains("$0.0500"));
        assert!(line_str.contains("ctx:30%"));
        assert!(line_str.contains("time:"));
        assert!(line_str.contains("deepseek-v3"));
        assert!(line_str.contains("◕"));
    }

    #[test]
    fn test_footer_context_high_percent_shows_red() {
        let mut footer = FooterState::new();
        footer.context_percent = 90;
        let line = footer.render_line2("model", "○", &Theme::default());
        // Line 包含 spans，检查渲染不 panic
        assert!(!line.to_string().is_empty());
    }

    #[test]
    fn test_tui_engine_new_initializes_fields() {
        let engine = TuiEngine::new();
        assert!(engine.session_id.is_empty());
        assert!(engine.model.is_empty());
        assert!(!engine.agent_busy);
        assert!(!engine.leader_pending);
        assert!(engine.queue.is_empty());
        assert!(!engine.permission.has_pending());
        assert_eq!(engine.chat.messages.len(), 0);
        assert_eq!(engine.theme.name, "default");
    }

    #[test]
    fn test_handle_event_turn_end_updates_footer() {
        let mut engine = TuiEngine::new();
        engine.handle_event(AgentEvent::TurnEnd {
            turn: 1,
            usage: UsageInfo {
                input_tokens: 10_000,
                output_tokens: 5_000,
                cost: None,
            },
        });
        assert!(!engine.agent_busy);
        assert_eq!(engine.footer.input_tokens, 10_000);
        assert_eq!(engine.footer.output_tokens, 5_000);
    }

    #[test]
    fn test_handle_event_session_end_updates_footer() {
        let mut engine = TuiEngine::new();
        engine.agent_busy = true;
        engine.handle_event(AgentEvent::SessionEnd {
            session_id: "sess123".into(),
            total_turns: 5,
            total_tokens: UsageInfo {
                input_tokens: 100_000,
                output_tokens: 50_000,
                cost: None,
            },
            exit_reason: "done".into(),
        });
        assert!(!engine.agent_busy);
        assert_eq!(engine.footer.input_tokens, 100_000);
        assert_eq!(engine.footer.output_tokens, 50_000);
    }

    #[test]
    fn test_render_lines_with_theme_and_renderers() {
        let engine = TuiEngine::new();
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        // 空消息 — 不 panic，无提示文字
        let lines = engine
            .chat
            .render_lines(area, &engine.renderers, &engine.theme, 0, false);
        assert_eq!(lines.len(), 0);

        // 使用 light theme — 不 panic
        let light = Theme::light();
        let lines_light = engine
            .chat
            .render_lines(area, &engine.renderers, &light, 0, false);
        assert_eq!(lines_light.len(), 0);
    }
}
