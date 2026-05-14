//! uncode-tui — 终端四面板交互界面
//!
//! 基于 ratatui + crossterm 实现，订阅 AgentLoop 事件流，
//! 实时渲染四个面板：任务清单、工具调用、思考过程、阶段总结。

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use tokio::sync::broadcast;
use uncode_core::event::{AgentEvent, TaskStatus};

pub struct TuiEngine {
    events: Vec<AgentEvent>,
    current_task: String,
    current_tools: Vec<String>,
    current_thinking: String,
    current_summary: String,
    status_text: String,
    session_id: String,
}

impl TuiEngine {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            current_task: String::new(),
            current_tools: Vec::new(),
            current_thinking: String::new(),
            current_summary: String::new(),
            status_text: "uncode v0.1 | 就绪".into(),
            session_id: String::new(),
        }
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::SessionStart {
                ref session_id, ..
            } => {
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
            AgentEvent::ContentDelta {
                ref content, ..
            } => {
                self.current_thinking.push_str(content);
            }
            AgentEvent::ToolCallStart {
                ref tool_name, ..
            } => {
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
            AgentEvent::Error { ref message, .. } => {
                self.status_text = format!("⚠️ {message}");
            }
            AgentEvent::SessionEnd {
                ref exit_reason, ..
            } => {
                self.status_text = format!("uncode v0.1 | 结束: {exit_reason}");
            }
            _ => {}
        }
        self.events.push(event);
    }

    pub fn render(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.area());

        // Status bar
        let status = Paragraph::new(self.status_text.as_str())
            .style(Style::default().fg(Color::Gray))
            .block(Block::default());
        f.render_widget(status, chunks[0]);

        // Main 4-panel layout
        let main = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(chunks[1]);

        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(main[0]);

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(main[1]);

        // Panel 1: 任务清单
        self.render_task_list(f, top[0]);

        // Panel 2: 工具调用
        self.render_tool_calls(f, top[1]);

        // Panel 3: 思考过程
        self.render_thinking(f, bottom[0]);

        // Panel 4: 阶段总结
        self.render_summary(f, bottom[1]);

        // Input area
        let input = Paragraph::new("> ")
            .block(Block::default().borders(Borders::TOP).title("输入"));
        f.render_widget(input, chunks[2]);
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

        let content = Paragraph::new(text.to_string())
            .block(Block::default().borders(Borders::ALL).title("🛠️ 工具调用"))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(content, area);
    }

    fn render_thinking(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let text = if self.current_thinking.is_empty() {
            "等待 Agent 开始思考...".to_string()
        } else {
            let lines: Vec<&str> = self.current_thinking.lines().collect();
            let start = lines.len().saturating_sub(5);
            lines[start..].join("\n")
        };

        let content = Paragraph::new(text.to_string())
            .block(Block::default().borders(Borders::ALL).title("💭 思考过程"))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(content, area);
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

    pub async fn run<F>(&mut self, mut event_rx: broadcast::Receiver<AgentEvent>, on_key: F)
    where
        F: Fn(KeyCode),
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
                    on_key(key_event);
                    if key_event == KeyCode::Esc {
                        break;
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
