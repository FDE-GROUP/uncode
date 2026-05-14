//! uncode-tui — 终端四面板交互界面
//!
//! 基于 ratatui + crossterm 实现，订阅 AgentLoop 事件流，
//! 实时渲染四个面板：任务清单、工具调用、思考过程、阶段总结。

pub mod code_detail;
pub mod complete;
pub mod diff_viewer;
pub mod highlight;
pub mod input;
pub mod markdown;
pub mod selector;
pub mod slash;

use crate::code_detail::CodeDetailView;
use crate::complete::CompletionEngine;
use crate::diff_viewer::DiffViewer;
use crate::input::{InputAction, InputEditor};
use crate::selector::OverlaySelector;
use crate::slash::SlashCommands;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use tokio::sync::broadcast;
use uncode_core::event::{AgentEvent, TaskStatus};

pub struct TuiEngine {
    current_task: String,
    current_tools: Vec<String>,
    current_thinking: String,
    current_summary: String,
    status_text: String,
    session_id: String,
    editor: InputEditor,
    code_detail: CodeDetailView,
    simple_mode: bool,
    layout_locked: bool,
    slash: SlashCommands,
    completion: CompletionEngine,
    diff: DiffViewer,
    selector: OverlaySelector,
}

impl TuiEngine {
    pub fn new() -> Self {
        Self {
            current_task: String::new(),
            current_tools: Vec::new(),
            current_thinking: String::new(),
            current_summary: String::new(),
            status_text: "uncode v0.1 | 就绪".into(),
            session_id: String::new(),
            editor: InputEditor::new(),
            code_detail: CodeDetailView::new(),
            simple_mode: false,
            layout_locked: false,
            slash: SlashCommands::new(),
            completion: CompletionEngine::new(slash_commands()),
            diff: DiffViewer::new(),
            selector: OverlaySelector::new(),
        }
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::SessionStart { ref session_id, .. } => {
                self.session_id = session_id.clone();
                self.status_text = format!("uncode v0.1 | 会话: {} | 运行中", &session_id[..8]);
            }
            AgentEvent::TaskUpdate {
                ref title,
                ref status,
                ..
            } => {
                let icon = match status {
                    TaskStatus::Pending => "⏳",
                    TaskStatus::Running => "🔄",
                    TaskStatus::Done => "✅",
                    TaskStatus::Failed => "❌",
                    TaskStatus::Blocked => "🚫",
                };
                self.current_task = format!("{icon} {title}");
            }
            AgentEvent::ContentDelta { ref content, .. } => {
                self.current_thinking.push_str(content);
            }
            AgentEvent::ToolCallStart { ref tool_name, .. } => {
                self.current_tools.push(format!("🔄 {tool_name}"));
            }
            AgentEvent::ToolCallEnd {
                ref tool_id,
                ref status,
                ..
            } => {
                let icon = match status {
                    uncode_core::event::ToolCallStatus::Success => "✅",
                    _ => "❌",
                };
                if let Some(t) = self
                    .current_tools
                    .iter_mut()
                    .find(|t| t.contains(tool_id.as_str()))
                {
                    *t = t.replace("🔄", icon);
                }
            }
            AgentEvent::PhaseSummary {
                ref completed,
                ref next_steps,
                ..
            } => {
                self.current_summary = format!(
                    "已完成：{}\n下一步：{}",
                    completed.join("、"),
                    next_steps.join("、")
                );
            }
            AgentEvent::Error {
                ref category,
                ref message,
                ..
            } => {
                let friendly = match category {
                    uncode_core::event::ErrorCategory::Llm => {
                        format!("⚠️ AI 服务暂时不可用，正在重试...")
                    }
                    uncode_core::event::ErrorCategory::Tool => {
                        format!("⚠️ 工具执行出错: {message}")
                    }
                    uncode_core::event::ErrorCategory::Network => {
                        "⚠️ 网络连接中断，等待恢复...".into()
                    }
                    uncode_core::event::ErrorCategory::Config => {
                        format!("⚠️ 配置错误: {message}")
                    }
                };
                self.status_text = friendly;
            }
            AgentEvent::SessionEnd {
                ref exit_reason, ..
            } => {
                self.status_text = format!("uncode v0.1 | 结束: {exit_reason}");
            }
            _ => {}
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.area());

        let status = Paragraph::new(self.status_text.as_str())
            .style(Style::default().fg(Color::Gray))
            .block(Block::default());
        f.render_widget(status, chunks[0]);

        if self.simple_mode {
            self.render_simple_layout(f, chunks[1]);
        } else {
            self.render_full_layout(f, chunks[1]);
        }

        self.editor.render(f, chunks[2]);

        if self.code_detail.is_visible() {
            self.code_detail.render(f, chunks[1]);
        }
        if self.diff.is_visible() {
            self.diff.render(f, chunks[1]);
        }
        self.selector.render(f, f.area());
    }

    fn render_full_layout(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let main = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(area);

        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(main[0]);

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(main[1]);

        self.render_task_list(f, top[0]);
        self.render_tool_calls(f, top[1]);
        self.render_thinking(f, bottom[0]);
        self.render_summary(f, bottom[1]);
    }

    fn render_simple_layout(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let panels = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(area);

        self.render_task_list(f, panels[0]);
        self.render_summary(f, panels[1]);
    }

    fn render_task_list(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let task_text = if self.current_task.is_empty() {
            "描述你的需求，Agent 会自动拆解为任务清单"
        } else {
            &self.current_task
        };

        let content = Paragraph::new(task_text.to_string())
            .block(Block::default().borders(Borders::ALL).title("📋 任务清单"))
            .style(Style::default().fg(Color::White));
        f.render_widget(content, area);
    }

    fn render_tool_calls(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let text = if self.current_tools.is_empty() {
            "等待 Agent 开始工作...".to_string()
        } else {
            self.current_tools.join("\n")
        };

        let content = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("🛠️ 工具调用"))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(content, area);
    }

    fn render_thinking(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let block = Block::default().borders(Borders::ALL).title("💭 思考过程");

        if self.current_thinking.is_empty() {
            let content = Paragraph::new("等待 Agent 开始思考...")
                .block(block)
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(content, area);
        } else {
            let lines = crate::markdown::render_markdown(&self.current_thinking);
            let display: Vec<Line> = lines.into_iter().rev().take(20).rev().collect();
            let content = Paragraph::new(Text::from(display)).block(block);
            f.render_widget(content, area);
        }
    }

    fn render_summary(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let text = if self.current_summary.is_empty() {
            "完成第一个任务后会自动生成阶段总结..."
        } else {
            &self.current_summary
        };

        let content = Paragraph::new(text.to_string())
            .block(Block::default().borders(Borders::ALL).title("📝 阶段总结"))
            .style(Style::default().fg(Color::Green));
        f.render_widget(content, area);
    }

    pub async fn run<F>(&mut self, mut event_rx: broadcast::Receiver<AgentEvent>, on_submit: F)
    where
        F: Fn(String),
    {
        let mut terminal = ratatui::init();
        loop {
            terminal
                .draw(|f| self.render(f))
                .expect("terminal draw failed");

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
                                    return Ok::<KeyCode, std::io::Error>(key.code);
                                }
                            }
                        }
                        tokio::task::yield_now().await;
                    }
                } => {
                    match key_event {
                        KeyCode::Char('d') => self.code_detail.toggle(),
                        KeyCode::Char('e') => self.code_detail.toggle_fullscreen(),
                        KeyCode::Char('n') => self.diff.next_file(),
                        KeyCode::Char('p') => self.diff.prev_file(),
                        KeyCode::Char('j') if self.selector.is_visible() => self.selector.next(),
                        KeyCode::Char('k') if self.selector.is_visible() => self.selector.prev(),
                        KeyCode::Enter if self.selector.is_visible() => self.selector.hide(),
                        KeyCode::Char('l') if !self.layout_locked => {
                            self.layout_locked = true;
                            self.status_text = "uncode v0.1 | 布局已锁定".into();
                        }
                        KeyCode::Esc => break,
                        _ => {
                            let action = self.editor.handle_key(key_event);
                            match action {
                                InputAction::Submit(text) => {
                                    if text == "/simple" {
                                        self.simple_mode = true;
                                        self.status_text = "uncode v0.1 | 简化模式".into();
                                    } else if text == "/full" {
                                        self.simple_mode = false;
                                        self.status_text = "uncode v0.1 | 完整模式".into();
                                    } else if text == "/unlock" {
                                        self.layout_locked = false;
                                        self.status_text = "uncode v0.1 | 布局已解锁".into();
                                    } else if let Some(response) = self.slash.execute(&text) {
                                        self.current_summary = response;
                                    } else {
                                        on_submit(text);
                                    }
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
}

impl Default for TuiEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn slash_commands() -> Vec<String> {
    [
        "simple", "full", "unlock", "help", "quit", "think", "issues",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
