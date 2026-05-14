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
pub mod selector;
pub mod slash;

use crate::chat::ChatState;
use crate::complete::CompletionEngine;
use crate::input::{InputAction, InputEditor};
use crate::selector::OverlaySelector;
use crate::slash::SlashCommands;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph, Wrap};
use tokio::sync::broadcast;
use uncode_core::event::AgentEvent;

pub struct TuiEngine {
    chat: ChatState,
    session_id: String,
    model: String,
    editor: InputEditor,
    selector: OverlaySelector,
    slash: SlashCommands,
    completion: CompletionEngine,
    leader_pending: bool,
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
        let lines = self.chat.render_lines(area);
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
        let cwd = std::env::current_dir()
            .map(|p| {
                let home = dirs::home_dir().unwrap_or_default();
                p.strip_prefix(&home)
                    .map(|s| format!("~/{}", s.display()))
                    .unwrap_or_else(|_| format!("{}", p.display()))
            })
            .unwrap_or_default();

        let branch = std::process::Command::new("git")
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

        let sid = if self.session_id.is_empty() {
            String::new()
        } else {
            format!(" session:{}", &self.session_id[..self.session_id.len().min(8)])
        };

        let footer_line1 = format!("{cwd} {branch}{sid}");
        f.render_widget(
            Paragraph::new(footer_line1).style(Style::default().fg(Color::DarkGray)),
            line1_area,
        );

        let level = self.chat.thinking_level;
        let level_icon = level.icon();
        let model_display = if self.model.is_empty() {
            "uncode"
        } else {
            &self.model
        };
        let footer_line2 = format!("{model_display} {level_icon}");
        f.render_widget(
            Paragraph::new(footer_line2).style(Style::default().fg(Color::DarkGray)),
            line2_area,
        );
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
                    self.handle_event(event);
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
                self.chat.push_user_message(text.clone());
                on_submit(text);
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
            AgentEvent::SessionEnd { .. } => {}
            _ => {}
        }
        self.chat.handle_event(&event);
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
