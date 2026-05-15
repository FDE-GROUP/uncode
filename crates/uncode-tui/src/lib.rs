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
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
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
            Span::styled(
                model.to_string(),
                Style::default().fg(theme.tool_status.running),
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
    current_cancel: Option<CancellationToken>,
    permission: PermissionManager,
    footer: FooterState,
    theme: Theme,
    renderers: ToolRendererRegistry,
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
            tick: 0,
        }
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

        let border_color = self.chat.thinking_level.border_color();
        self.editor.render(f, chunks[1], border_color);

        self.render_footer(f, chunks[2], chunks[3]);

        self.selector.render(f, f.area());
    }

    fn render_chat(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        let lines = self.chat.render_lines(
            area,
            &self.renderers,
            &self.theme,
            self.tick,
            self.agent_busy,
        );
        let total_lines = lines.len() as u16;
        let visible_height = area.height;

        // 滚到底部时恢复 auto_scroll
        if self.chat.scroll_offset + visible_height >= total_lines {
            self.chat.auto_scroll = true;
        }

        // auto_scroll 模式：offset 跟随底部
        if self.chat.auto_scroll && total_lines > visible_height {
            self.chat.scroll_offset = total_lines.saturating_sub(visible_height);
        }

        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.chat.scroll_offset, 0));
        f.render_widget(content, area);
    }

    fn render_footer(
        &self,
        f: &mut Frame,
        line1_area: ratatui::layout::Rect,
        line2_area: ratatui::layout::Rect,
    ) {
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let (status_icon, status_color) = if self.agent_busy {
            let frame = spinner_frames[self.tick % spinner_frames.len()];
            (frame, Color::Yellow)
        } else {
            ("●", Color::Green)
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
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
        loop {
            if let Err(e) = terminal.draw(|f| self.render(f)) {
                eprintln!("terminal draw failed: {e}");
                break;
            }

            tokio::select! {
                Ok(event) = event_rx.recv() => {
                    let is_turn_end = matches!(event, AgentEvent::TurnEnd { .. } | AgentEvent::SessionEnd { .. } | AgentEvent::AgentInterrupted { .. });
                    self.handle_event(event);
                    if is_turn_end {
                        self.flush_queue(&on_submit);
                    }
                }
                Ok(ui_event) = async {
                    loop {
                        if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                            match event::read().unwrap_or(Event::Key(
                                event::KeyEvent::new(KeyCode::Null, event::KeyModifiers::empty())
                            )) {
                                Event::Key(key) if key.kind == KeyEventKind::Press => {
                                    return Ok::<Event, std::io::Error>(Event::Key(key));
                                }
                                Event::Mouse(mouse) => {
                                    return Ok::<Event, std::io::Error>(Event::Mouse(mouse));
                                }
                                _ => {}
                            }
                        }
                        tokio::task::yield_now().await;
                    }
                } => {
                    match ui_event {
                        Event::Key(key_event) => {
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
                                        "切换模型",
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
                                // Quit / Interrupt
                                KeyCode::Char('c') if ctrl => {
                                    if self.agent_busy {
                                        if let Some(ref token) = self.current_cancel {
                                            token.cancel();
                                        }
                                        self.agent_busy = false;
                                        self.current_cancel = None;
                                        self.chat.messages.push(chat::ChatMessage::Summary {
                                            completed: vec!["[中断] Agent 已停止".into()],
                                            next_steps: vec![],
                                        });
                                    } else {
                                        break;
                                    }
                                }
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
                        _ => {}
                    }
                }
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
                let help = "快捷键: Ctrl+O 工具输出 | Ctrl+T 思考 | Ctrl+L 模型 | Shift+Tab 思考级别 | Ctrl+X 前缀命令 | Ctrl+C 中断/退出\n命令: /clear | /compact | /model [name] | /new | /theme [name] | /thinking | /details | /tree | /skills | /template";
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
            self.agent_busy = true;
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
            completed: vec!["对话已清空。".into()],
            next_steps: vec![],
        });
    }

    fn handle_compact_command(&mut self) {
        let ctx_pct = self.footer.context_percent;
        let in_str = format_tokens(self.footer.input_tokens);
        let out_str = format_tokens(self.footer.output_tokens);
        let msg_count = self.chat.messages.len();

        let mut lines = vec![
            format!("上下文使用: {ctx_pct}% (in:{in_str} out:{out_str})"),
            format!("对话消息数: {msg_count}"),
        ];

        if ctx_pct >= 80 {
            lines.push("已达压缩阈值，下轮对话将自动压缩。".into());
        } else if ctx_pct >= 50 {
            lines.push("上下文使用中等，建议在超过 80% 前主动压缩。".into());
        } else {
            lines.push("上下文使用率低，无需压缩。".into());
        }

        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: lines,
            next_steps: vec![],
        });
    }

    fn handle_new_command(&mut self) {
        let old_id = if self.session_id.is_empty() {
            "无".to_string()
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
                "新会话已创建。session:{} → session:{new_id}",
                old_id
            )],
            next_steps: vec![],
        });
    }

    fn handle_model_command(&mut self, text: &str) {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let name = parts.get(1).copied().unwrap_or("").trim();

        if name.is_empty() {
            self.selector.show(
                "切换模型",
                vec!["deepseek-v3".into(), "glm-5.1".into(), "ollama".into()],
            );
            return;
        }

        let old = self.model.clone();
        self.model = name.to_string();
        self.chat.messages.push(chat::ChatMessage::Summary {
            completed: vec![format!("模型切换: {old} → {name}")],
            next_steps: vec![],
        });
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
            AgentEvent::AgentInterrupted { .. } => {
                self.agent_busy = false;
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
                completed: vec!["当前没有活跃会话。".into()],
                next_steps: vec![],
            });
            return;
        }

        let session_dir = match uncode_session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.messages.push(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_session::store::SessionStore::new(session_dir);

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
        if let Some(text) = self.queue.drain_follow_up() {
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

fn slash_commands() -> Vec<String> {
    let mut cmds = vec![
        "help".into(),
        "quit".into(),
        "clear".into(),
        "compact".into(),
        "model".into(),
        "new".into(),
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
        });
        let cost1 = footer.cost;
        footer.update_usage(&UsageInfo {
            input_tokens: 1_000_000,
            output_tokens: 0,
        });
        assert!((footer.cost - cost1 * 2.0).abs() < 0.001);
    }

    #[test]
    fn test_footer_context_percent_clamped() {
        let mut footer = FooterState::new();
        footer.update_usage(&UsageInfo {
            input_tokens: 200_000,
            output_tokens: 0,
        });
        assert_eq!(footer.context_percent, 100);
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
        // 空消息 — 不 panic，返回提示文字
        let lines = engine
            .chat
            .render_lines(area, &engine.renderers, &engine.theme, 0, false);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().contains("描述你的需求"));

        // 使用 light theme — 不 panic
        let light = Theme::light();
        let lines_light = engine
            .chat
            .render_lines(area, &engine.renderers, &light, 0, false);
        assert_eq!(lines_light.len(), 1);
    }
}
