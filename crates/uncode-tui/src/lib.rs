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

use crate::chat::ChatState;
use crate::complete::CompletionEngine;
use crate::input::{InputAction, InputEditor};
use crate::message_queue::{MessageQueue, QueueType};
use crate::permission::PermissionManager;
use crate::selector::OverlaySelector;
use crate::slash::SlashCommands;
use crate::theme::Theme;
use crate::tool_renderer::ToolRendererRegistry;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use tokio::sync::broadcast;
use uncode_core::event::AgentEvent;
use uncode_core::message::UsageInfo;

/// 页脚状态 — Token 统计、费用、上下文使用率
struct FooterState {
    workdir: String,
    git_branch: String,
    input_tokens: u64,
    output_tokens: u64,
    cost: f64,
    context_percent: u8,
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
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        Self {
            workdir,
            git_branch,
            input_tokens: 0,
            output_tokens: 0,
            cost: 0.0,
            context_percent: 0,
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

    fn render_line1(&self, session_id: &str) -> String {
        let sid = if session_id.is_empty() {
            String::new()
        } else {
            format!(" session:{}", &session_id[..session_id.len().min(8)])
        };
        format!("{} {}{}", self.workdir, self.git_branch, sid)
    }

    fn render_line2(&self, model: &str, level_icon: &str) -> Line<'static> {
        let in_str = format_tokens(self.input_tokens);
        let out_str = format_tokens(self.output_tokens);
        let cost_str = format!("${:.4}", self.cost);
        let ctx_color = if self.context_percent > 80 {
            Color::Red
        } else {
            Color::DarkGray
        };

        Line::from(vec![
            Span::styled(
                format!("in:{in_str} "),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("out:{out_str} "),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("{cost_str} "), Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("ctx:{}% ", self.context_percent),
                Style::default().fg(ctx_color),
            ),
            Span::styled(model.to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {level_icon}"),
                Style::default().fg(Color::DarkGray),
            ),
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

pub struct TuiEngine {
    chat: ChatState,
    session_id: String,
    model: String,
    editor: InputEditor,
    selector: OverlaySelector,
    slash: SlashCommands,
    completion: CompletionEngine,
    leader_pending: bool,
    queue: MessageQueue,
    agent_busy: bool,
    permission: PermissionManager,
    footer: FooterState,
    theme: Theme,
    renderers: ToolRendererRegistry,
}

impl TuiEngine {
    pub fn new() -> Self {
        Self {
            chat: ChatState::new(),
            session_id: String::new(),
            model: String::new(),
            editor: InputEditor::new(),
            selector: OverlaySelector::new(),
            slash: SlashCommands::new(),
            completion: CompletionEngine::new(slash_commands()),
            leader_pending: false,
            queue: MessageQueue::new(),
            agent_busy: false,
            permission: PermissionManager::new(),
            footer: FooterState::new(),
            theme: Theme::default(),
            renderers: ToolRendererRegistry::new(),
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
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

        let border_color = self.chat.thinking_level.border_color();
        self.editor.render(f, chunks[1], border_color);

        self.render_footer(f, chunks[2], chunks[3]);

        self.selector.render(f, f.area());
    }

    fn render_chat(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let lines = self.chat.render_lines(area, &self.renderers, &self.theme);
        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default());
        f.render_widget(content, area);
    }

    fn render_footer(
        &self,
        f: &mut Frame,
        line1_area: ratatui::layout::Rect,
        line2_area: ratatui::layout::Rect,
    ) {
        let line1 = self.footer.render_line1(&self.session_id);
        f.render_widget(
            Paragraph::new(line1).style(Style::default().fg(self.theme.ui.footer_text)),
            line1_area,
        );

        let level = self.chat.thinking_level;
        let model_display = if self.model.is_empty() {
            "uncode"
        } else {
            &self.model
        };
        let line2 = self.footer.render_line2(model_display, level.icon());
        f.render_widget(Paragraph::new(line2), line2_area);
    }

    pub async fn run<F>(&mut self, mut event_rx: broadcast::Receiver<AgentEvent>, on_submit: F)
    where
        F: Fn(String),
    {
        let mut terminal = ratatui::init();
        loop {
            if let Err(e) = terminal.draw(|f| self.render(f)) {
                eprintln!("terminal draw failed: {e}");
                break;
            }

            tokio::select! {
                Ok(event) = event_rx.recv() => {
                    let is_turn_end = matches!(event, AgentEvent::TurnEnd { .. } | AgentEvent::SessionEnd { .. });
                    self.handle_event(event);
                    if is_turn_end {
                        self.flush_queue(&on_submit);
                    }
                }
                Ok(key_event) = async {
                    loop {
                        if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                            if let Event::Key(key) = event::read().unwrap_or(Event::Key(
                                event::KeyEvent::new(KeyCode::Null, event::KeyModifiers::empty())
                            )) {
                                if key.kind == KeyEventKind::Press {
                                    return Ok::<KeyEvent, std::io::Error>(key);
                                }
                            }
                        }
                        tokio::task::yield_now().await;
                    }
                } => {
                    if self.leader_pending {
                        self.leader_pending = false;
                        self.handle_leader_key(key_event);
                        continue;
                    }

                    let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);

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
                                "切换模型".into(),
                                vec!["deepseek-v3".into(), "glm-5.1".into(), "ollama".into()],
                            );
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
                        KeyCode::Enter if self.selector.is_visible() => self.selector.hide(),
                        // Quit
                        KeyCode::Char('c') if ctrl => break,
                        // Default: pass to input editor
                        KeyCode::Esc => {
                            let _ = self.editor.handle_key(key_event);
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
            }
        }
        ratatui::restore();
    }

    fn handle_submit<F>(&mut self, text: String, on_submit: &F)
    where
        F: Fn(String),
    {
        if let Some(response) = self.slash.execute(&text) {
            // Slash command response displayed in chat
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
                let help = "快捷键: Ctrl+O 工具输出 | Ctrl+T 思考 | Ctrl+L 模型 | Shift+Tab 思考级别 | Ctrl+X 前缀命令 | Ctrl+C 退出";
                self.chat.messages.push(chat::ChatMessage::Summary {
                    completed: vec![help.into()],
                    next_steps: vec![],
                });
            }
            _ => {
                if self.agent_busy {
                    self.queue.enqueue(text.clone(), QueueType::FollowUp);
                    self.chat
                        .messages
                        .push(chat::ChatMessage::QueuedMessage { text });
                } else {
                    self.agent_busy = true;
                    self.chat.push_user_message(text.clone());
                    on_submit(text);
                }
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
                    "切换模型".into(),
                    vec!["deepseek-v3".into(), "glm-5.1".into(), "ollama".into()],
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
                self.footer.update_usage(usage);
            }
            AgentEvent::SessionEnd { total_tokens, .. } => {
                self.agent_busy = false;
                self.footer.update_usage(total_tokens);
            }
            _ => {}
        }
        self.chat.handle_event(&event);
    }

    /// Agent 闲下来后提交排队消息
    fn flush_queue<F>(&mut self, on_submit: &F)
    where
        F: Fn(String),
    {
        if let Some(text) = self.queue.drain_follow_up() {
            self.agent_busy = true;
            self.chat.push_user_message(text.clone());
            on_submit(text);
        }
    }
}

impl Default for TuiEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn slash_commands() -> Vec<String> {
    vec![
        "help".into(),
        "quit".into(),
        "thinking".into(),
        "details".into(),
        "issues".into(),
    ]
}
