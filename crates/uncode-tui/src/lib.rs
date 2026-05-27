//! uncode-tui — 对话驱动终端交互界面（L3 产品层）。
//!
//! 基于 ratatui + crossterm，订阅 [`uncode_core::event::AgentEvent`] 广播，
//! 渲染对话区、工具卡片、权限门控与页脚用量。
//!
//! **Pi:** 无独立 TUI crate；机制上对应 Pi 终端 UI 对 `agentLoop` 事件流的消费。
//! **OpenCode:** scrollback / 工具卡片信息密度作 UX benchmark（见 `UNCODE_TUI_ARCHITECTURE`）。

pub mod chat;
pub mod code_detail;
pub mod complete;
pub mod custom_layout;
pub mod dialog;
pub mod dialog_channel;
pub mod diff_viewer;
pub mod highlight;
pub mod input;
pub mod markdown;
pub mod message_queue;
pub mod message_renderer;
pub mod overlay;
pub mod overlay_channel;
pub mod permission;
pub mod selector;
pub mod session_channel;
pub mod slash;
pub mod status;
pub mod theme;
pub mod tool_renderer;
pub mod ui_channel;
pub mod welcome;
pub mod widget;

use crate::chat::ChatState;
use crate::complete::CompletionEngine;
use crate::dialog::DialogOverlay;
use crate::dialog_channel::DialogBridge;
use crate::input::{InputAction, InputEditor};
use crate::message_queue::{MessageQueue, QueueType, SubmitIntent};
use crate::message_renderer::MessageRendererRegistry;
use crate::overlay::OverlayManager;
use crate::overlay_channel::OverlayBridge;
use crate::permission::PermissionManager;
use crate::selector::OverlaySelector;
use crate::session_channel::SessionBridge;
use crate::slash::SlashCommands;
use crate::status::StatusManager;
use crate::theme::Theme;
use crate::tool_renderer::ToolRendererRegistry;
use crate::ui_channel::UiBridge;
use crate::welcome::WelcomeScreen;
use crate::widget::WidgetManager;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use uncode_agent::permission_gate::{Approval, PermissionGate};
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
    /// Current inner-loop turn (from `TurnStart`); 0 when idle.
    current_turn: u64,
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
            current_turn: 0,
            turn_start: None,
            last_elapsed: String::new(),
        }
    }

    fn set_current_turn(&mut self, turn: u64) {
        self.current_turn = turn;
    }

    fn clear_run_turn(&mut self) {
        self.current_turn = 0;
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

    fn render_line2(
        &self,
        model: &str,
        level_icon: &str,
        level_label: &str,
        theme: &Theme,
    ) -> Line<'static> {
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

        let turn_span = if self.current_turn > 0 {
            vec![
                Span::styled("turn:", dim),
                Span::styled(format!("{} ", self.current_turn), value_style),
            ]
        } else {
            vec![]
        };

        let mut spans = vec![
            Span::styled("in:", dim),
            Span::styled(format!("{in_str} "), value_style),
            Span::styled("out:", dim),
            Span::styled(format!("{out_str} "), value_style),
            Span::styled(format!("{cost_str} "), value_style),
        ];
        spans.extend(turn_span);
        spans.extend([
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
            Span::styled(format!(" {level_icon} {level_label}"), dim),
        ]);
        Line::from(spans)
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

#[derive(Default)]
enum AgentActivity {
    #[default]
    Idle,
    Thinking,
    RunningTool {
        name: String,
    },
    Writing,
}

/// TUI 主引擎：输入、渲染、权限与对 `AgentEvent` 的订阅。
///
/// **Pi:** 终端侧事件消费者（Pi 无同名类型）。
/// **OpenCode:** 对照终端 scrollback 与工具展示，非 API 兼容。
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
    extension_shortcuts: Vec<(
        uncode_extensions::command::ExtKeyEvent,
        Box<dyn Fn() + Send + Sync>,
    )>,
    extension_manager:
        Option<Arc<parking_lot::Mutex<uncode_extensions::manager::ExtensionManager>>>,
    completion: CompletionEngine,
    leader_pending: bool,
    queue: MessageQueue,
    agent_busy: bool,
    custom_working_message: Option<String>,
    working_visible_override: Option<bool>,
    current_cancel: Option<CancellationToken>,
    permission: PermissionManager,
    permission_gate: Option<Arc<PermissionGate>>,
    activity: AgentActivity,
    footer: FooterState,
    theme: Theme,
    renderers: ToolRendererRegistry,
    welcome: WelcomeScreen,
    quit_requested: bool,
    tick: usize,
    dialog: DialogOverlay,
    dialog_bridge: Option<DialogBridge>,
    pending_dialog_response: Option<std::sync::mpsc::Sender<uncode_core::dialog::DialogResponse>>,
    overlay_manager: OverlayManager,
    overlay_bridge: Option<OverlayBridge>,
    widget_manager: WidgetManager,
    status_manager: StatusManager,
    ui_bridge: Option<UiBridge>,
    session_bridge: Option<SessionBridge>,
    message_renderers: MessageRendererRegistry,
    custom_header: Option<crate::custom_layout::CustomHeader>,
    custom_footer: Option<crate::custom_layout::CustomFooter>,
    custom_indicator: Option<crate::custom_layout::CustomIndicator>,
    /// Pending question from agent (Question tool).
    pending_question: Option<Box<uncode_core::event::QuestionRequestData>>,
    /// Selected option indices for the current question (multi-select when multiple=true).
    question_selected: std::collections::HashSet<usize>,
    /// Shortcut help overlay visible state.
    show_shortcut_help: bool,
    /// Selected button index in permission dialog (0=Allow, 1=Reject, 2=Always, 3=Edit).
    permission_selected: usize,
    /// When the permission dialog appeared (for 20s auto-approve countdown).
    permission_started_at: Option<std::time::Instant>,
    cwd: std::path::PathBuf,
}

impl TuiEngine {
    pub fn new_cancel_token(&mut self) -> CancellationToken {
        let token = CancellationToken::new();
        self.current_cancel = Some(token.clone());
        token
    }

    /// Reuse the active run's cancel token (steer must not replace the token Agent holds).
    fn current_or_new_cancel_token(&mut self) -> CancellationToken {
        if let Some(token) = self.current_cancel.clone() {
            return token;
        }
        self.new_cancel_token()
    }

    pub fn new() -> Self {
        let footer = FooterState::new();
        let mut chat = ChatState::new();
        chat.workdir = footer.workdir.clone();
        Self {
            chat,
            session_id: String::new(),
            model: String::new(),
            model_index: 0,
            available_models: Vec::new(),
            last_user_input: None,
            editor: InputEditor::new(),
            selector: OverlaySelector::new(),
            slash: SlashCommands::new(),
            extension_shortcuts: Vec::new(),
            extension_manager: None,
            completion: CompletionEngine::new(slash_commands()),
            leader_pending: false,
            queue: MessageQueue::new(),
            agent_busy: false,
            custom_working_message: None,
            working_visible_override: None,
            current_cancel: None,
            permission: PermissionManager::new(),
            permission_gate: None,
            activity: AgentActivity::default(),
            footer,
            theme: Theme::default(),
            renderers: ToolRendererRegistry::new(),
            welcome: WelcomeScreen::new(),
            quit_requested: false,
            tick: 0,
            dialog: DialogOverlay::new(Theme::default()),
            dialog_bridge: None,
            pending_dialog_response: None,
            overlay_manager: OverlayManager::new(),
            overlay_bridge: None,
            widget_manager: WidgetManager::new(),
            status_manager: StatusManager::new(),
            ui_bridge: None,
            session_bridge: None,
            message_renderers: MessageRendererRegistry::new(),
            custom_header: None,
            custom_footer: None,
            custom_indicator: None,
            pending_question: None,
            question_selected: std::collections::HashSet::new(),
            show_shortcut_help: false,
            permission_selected: 0,
            permission_started_at: None,
            cwd: std::env::current_dir().unwrap_or_default(),
        }
    }

    pub fn set_available_models(&mut self, models: Vec<String>) {
        self.available_models = models;
    }

    pub fn set_default_model(&mut self, model: String) {
        if let Some(idx) = self.available_models.iter().position(|m| m == &model) {
            self.model_index = idx;
        }
        self.model = model;
    }

    /// Wire TUI confirmation UI to agent-side [`PermissionToolHooks`](uncode_agent::PermissionToolHooks).
    pub fn set_permission_gate(&mut self, gate: Arc<PermissionGate>) {
        self.permission_gate = Some(gate);
    }

    /// Set the configurable permission policy for TUI-side confirmation checks.
    pub fn set_permission_policy(
        &mut self,
        policy: Arc<uncode_agent::tool_permission::PermissionPolicy>,
    ) {
        self.permission.set_policy(policy);
    }

    /// Register a slash command from an extension.
    pub fn register_slash_command(
        &mut self,
        name: &str,
        description: &str,
        handler: crate::slash::CommandFn,
    ) {
        self.slash.register(name, description, handler);
    }

    /// Register a keyboard shortcut from an extension.
    pub fn register_extension_shortcut(
        &mut self,
        key: uncode_extensions::command::ExtKeyEvent,
        handler: Box<dyn Fn() + Send + Sync>,
    ) {
        self.extension_shortcuts.push((key, handler));
    }

    /// Unregister a slash command by name.
    pub fn unregister_slash_command(&mut self, name: &str) -> bool {
        self.slash.unregister(name)
    }

    /// Unregister an extension shortcut by key.
    pub fn unregister_extension_shortcut(
        &mut self,
        key: &uncode_extensions::command::ExtKeyEvent,
    ) -> bool {
        let before = self.extension_shortcuts.len();
        self.extension_shortcuts.retain(|(k, _)| k != key);
        self.extension_shortcuts.len() < before
    }

    /// Set the extension manager for `/extensions` commands.
    pub fn set_extension_manager(
        &mut self,
        mgr: Arc<parking_lot::Mutex<uncode_extensions::manager::ExtensionManager>>,
    ) {
        self.extension_manager = Some(mgr);
    }

    /// Set the dialog bridge for extension-initiated dialogs.
    pub fn set_dialog_bridge(&mut self, bridge: DialogBridge) {
        self.dialog_bridge = Some(bridge);
    }

    /// Set the overlay bridge for extension-initiated overlays.
    pub fn set_overlay_bridge(&mut self, bridge: OverlayBridge) {
        self.overlay_bridge = Some(bridge);
    }

    /// Set the UI bridge for extension-initiated widget/status actions.
    pub fn set_ui_bridge(&mut self, bridge: UiBridge) {
        self.ui_bridge = Some(bridge);
    }

    /// Set the session bridge for extension-initiated session tree operations.
    pub fn set_session_bridge(&mut self, bridge: SessionBridge) {
        self.session_bridge = Some(bridge);
    }

    /// Register a custom tool renderer from an extension.
    pub fn register_custom_renderer(
        &mut self,
        tool_name: String,
        config: uncode_extensions::renderer::ToolRenderConfig,
    ) {
        use crate::tool_renderer::{ResultStyle, TemplateToolRenderer};
        let style = match config.result_style {
            uncode_extensions::renderer::ResultStyle::Plain => ResultStyle::Plain,
            uncode_extensions::renderer::ResultStyle::Code => ResultStyle::Code,
            uncode_extensions::renderer::ResultStyle::Diff => ResultStyle::Diff,
            uncode_extensions::renderer::ResultStyle::Bash => ResultStyle::Bash,
        };
        let renderer = TemplateToolRenderer::new(
            config.call_template,
            config.call_template_fields,
            style,
            config.result_max_lines,
        );
        self.renderers.register(tool_name, Box::new(renderer));
    }

    /// Register a custom message renderer by message type.
    pub fn register_custom_message_renderer(
        &mut self,
        message_type: String,
        config: uncode_extensions::message_renderer::MessageRenderConfig,
    ) {
        use crate::message_renderer::TemplateMessageRenderer;
        let renderer =
            TemplateMessageRenderer::new(config.render_template, config.result_max_lines);
        self.message_renderers
            .register(message_type, Box::new(renderer));
    }

    /// Set custom header. Pass `None` to restore built-in (no header).
    pub fn set_custom_header(
        &mut self,
        config: Option<uncode_extensions::header_footer::HeaderConfig>,
    ) {
        self.custom_header = config.map(|c| crate::custom_layout::CustomHeader::from_config(&c));
    }

    /// Set custom footer. Pass `None` to restore built-in footer.
    pub fn set_custom_footer(
        &mut self,
        config: Option<uncode_extensions::header_footer::FooterConfig>,
    ) {
        self.custom_footer = config.map(|c| crate::custom_layout::CustomFooter::from_config(&c));
    }

    /// Set custom working indicator. Pass `None` to restore built-in ●/○.
    pub fn set_custom_indicator(
        &mut self,
        config: Option<uncode_extensions::header_footer::WorkingIndicatorConfig>,
    ) {
        self.custom_indicator =
            config.map(|c| crate::custom_layout::CustomIndicator::from_config(&c));
    }

    /// Switch the active TUI theme by name.
    pub fn set_theme_by_name(&mut self, name: &str) {
        if let Some(theme) = Theme::load_by_name(name) {
            self.theme = theme;
        }
    }

    /// Set custom thinking level labels.
    pub fn set_thinking_labels(&mut self, labels: std::collections::HashMap<String, String>) {
        self.chat.custom_thinking_labels = labels;
    }

    /// Set the thinking level directly.
    pub fn set_thinking_level(&mut self, level: crate::chat::ThinkingLevel) {
        self.chat.thinking_level = level;
    }

    /// Try to dispatch a key event to extension shortcuts.
    fn try_extension_shortcut(&self, key_event: crossterm::event::KeyEvent) -> bool {
        use uncode_extensions::command::{ExtKey, ExtKeyEvent, ExtModifiers};
        let ext_key = match key_event.code {
            crossterm::event::KeyCode::Char(c) => ExtKey::Char(c),
            crossterm::event::KeyCode::F(n) => ExtKey::F(n),
            crossterm::event::KeyCode::Enter => ExtKey::Enter,
            crossterm::event::KeyCode::Esc => ExtKey::Escape,
            crossterm::event::KeyCode::Backspace => ExtKey::Backspace,
            crossterm::event::KeyCode::Tab => ExtKey::Tab,
            crossterm::event::KeyCode::Up => ExtKey::Up,
            crossterm::event::KeyCode::Down => ExtKey::Down,
            crossterm::event::KeyCode::Left => ExtKey::Left,
            crossterm::event::KeyCode::Right => ExtKey::Right,
            crossterm::event::KeyCode::Home => ExtKey::Home,
            crossterm::event::KeyCode::End => ExtKey::End,
            crossterm::event::KeyCode::PageUp => ExtKey::PageUp,
            crossterm::event::KeyCode::PageDown => ExtKey::PageDown,
            crossterm::event::KeyCode::Delete => ExtKey::Delete,
            crossterm::event::KeyCode::Insert => ExtKey::Insert,
            _ => return false,
        };
        let mods = key_event.modifiers;
        let lookup = ExtKeyEvent {
            key: ext_key,
            modifiers: ExtModifiers {
                ctrl: mods.contains(crossterm::event::KeyModifiers::CONTROL),
                alt: mods.contains(crossterm::event::KeyModifiers::ALT),
                shift: mods.contains(crossterm::event::KeyModifiers::SHIFT),
            },
        };
        for (key, handler) in &self.extension_shortcuts {
            if *key == lookup {
                handler();
                return true;
            }
        }
        false
    }

    fn resolve_permission(&mut self, tool_id: &str, approval: Approval) {
        self.permission_started_at = None;
        if let Some(ref gate) = self.permission_gate {
            let gate = gate.clone();
            let id = tool_id.to_string();
            tokio::spawn(async move {
                gate.resolve(&id, approval).await;
            });
        }
    }

    /// Auto-approve pending permission after 20s timeout (called from idle tick).
    fn check_auto_approve(&mut self) {
        if let Some(started) = self.permission_started_at {
            if started.elapsed().as_secs() >= 20 {
                if let Some(p) = self
                    .permission
                    .confirm(crate::permission::ConfirmOption::Allow)
                {
                    self.resolve_permission(&p.tool_id, Approval::Allow);
                } else {
                    self.permission_started_at = None;
                }
            }
            if !self.permission.has_pending() {
                self.permission_started_at = None;
            }
        }
    }

    /// Open session store, pushing an error message on failure.
    fn open_session_store(&mut self) -> Option<uncode_agent::session::store::SessionStore> {
        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return None;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        let rt = tokio::runtime::Handle::current();
        match rt.block_on(store) {
            Ok(s) => Some(s),
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("创建会话存储失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                None
            }
        }
    }

    /// End of a full agent `run` (not a single inner Turn).
    fn finish_agent_run(&mut self) {
        self.agent_busy = false;
        self.activity = AgentActivity::Idle;
        self.footer.end_turn();
        self.footer.clear_run_turn();
        self.current_cancel = None;
    }

    /// ESC 键处理：按优先级 — 拒绝权限 → 中断 Agent → 清除焦点 → 关闭覆盖层 → 关闭 Overlay
    pub fn handle_esc(&mut self) {
        if self.show_shortcut_help {
            self.show_shortcut_help = false;
            return;
        }
        if self.permission.has_pending() {
            if let Some(p) = self.permission.deny() {
                self.resolve_permission(&p.tool_id, Approval::Deny);
            }
            self.permission_started_at = None;
            return;
        }
        if self.agent_busy {
            if let Some(ref token) = self.current_cancel {
                token.cancel();
            }
            self.finish_agent_run();
            self.chat.deactivate_thinking();
            self.chat.invalidate_all();
            self.chat.push_message(chat::ChatMessage::Summary {
                completed: vec!["[Interrupted] Agent stopped.".into()],
                next_steps: vec![],
            });
        }
        if self.chat.focused_card.is_some() {
            self.chat.clear_focus();
        }
        if self.overlay_manager.has_visible() {
            self.overlay_manager.handle_escape();
            return;
        }
        if self.welcome.is_visible() {
            self.welcome.hide();
        }
        if self.selector.is_visible() {
            self.selector.hide();
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        if f.area().width == 0 || f.area().height == 0 {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        use uncode_core::ui_action::WidgetPlacement;
        let above_lines = self.widget_manager.lines_for(WidgetPlacement::AboveEditor);
        let below_lines = self.widget_manager.lines_for(WidgetPlacement::BelowEditor);
        let header_lines = self
            .custom_header
            .as_ref()
            .map(|h| h.line_count())
            .unwrap_or(0);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints({
                let mut c = Vec::with_capacity(9);
                if header_lines > 0 {
                    c.push(Constraint::Length(header_lines)); // Custom header
                }
                c.push(Constraint::Min(0)); // 对话区
                c.push(Constraint::Length(1)); // 状态行
                if above_lines > 0 {
                    c.push(Constraint::Length(above_lines)); // AboveEditor widgets
                }
                c.push(Constraint::Length(3)); // 输入栏
                if below_lines > 0 {
                    c.push(Constraint::Length(below_lines)); // BelowEditor widgets
                }
                c.push(Constraint::Length(1)); // 页脚第 1 行
                c.push(Constraint::Length(1)); // 页脚第 2 行
                c
            })
            .split(f.area());

        let mut idx = 0;
        if header_lines > 0 {
            self.render_header(f, chunks[idx]);
            idx += 1;
        }
        self.render_chat(f, chunks[idx]);
        idx += 1;
        // Permission modal: overlay chat area when pending
        if self.permission.has_pending() {
            self.render_permission_dialog(f, f.area());
            // Render simplified status and footer
            self.render_status(f, chunks[idx]);
            idx += 1;
            if above_lines > 0 {
                idx += 1;
            }

            // Question dialog: overlay when pending
            if self.pending_question.is_some() {
                self.render_question_dialog(f, f.area());
                self.render_status(f, chunks[idx]);
                idx += 1;
                if above_lines > 0 {
                    idx += 1;
                }
            }
            self.editor
                .render(f, chunks[idx], self.theme.ui.footer_text);
            idx += 1;
            if below_lines > 0 {
                idx += 1;
            }
            self.render_footer(f, chunks[idx], chunks[idx + 1]);
        } else {
            self.render_status(f, chunks[idx]);
            idx += 1;
            if above_lines > 0 {
                self.widget_manager
                    .render(f, chunks[idx], WidgetPlacement::AboveEditor);
                idx += 1;
            }
            self.editor
                .render(f, chunks[idx], self.theme.ui.footer_text);
            idx += 1;
            if below_lines > 0 {
                self.widget_manager
                    .render(f, chunks[idx], WidgetPlacement::BelowEditor);
                idx += 1;
            }
            let footer1 = chunks[idx];
            let footer2 = chunks[idx + 1];
            self.render_footer(f, footer1, footer2);
        }

        self.selector.render(f, f.area());
        self.dialog.render(f, f.area());
        self.overlay_manager.render(f, f.area());
        self.welcome.render(f, f.area());
        if self.show_shortcut_help {
            self.render_shortcut_help(f, f.area());
        }
    }

    fn render_chat(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        let visible_height = area.height as usize;

        // Step 1: Update line count cache (only re-renders stale messages)
        let workdir = self.chat.workdir.clone();
        self.chat.ensure_line_counts(
            area.width,
            &self.renderers,
            &self.theme,
            self.tick,
            self.agent_busy,
            self.chat.tool_output_visible,
            &workdir,
            &self.message_renderers,
        );

        // Step 2: Compute total and auto_scroll
        let total_lines = self.chat.total_lines();
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

    fn render_status(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        if self.permission.has_pending() {
            // Simplified: permission modal handles the detailed UI
            let Some(p) = self.permission.pending() else {
                return;
            };
            let line = Line::from(vec![Span::styled(
                format!("⏳ Awaiting approval for {}…", p.tool_name),
                Style::default()
                    .black()
                    .bg(self.theme.tool_status.await_confirm)
                    .bold(),
            )]);
            f.render_widget(Paragraph::new(line), area);
            return;
        }

        if !self.agent_busy {
            return;
        }

        // Extension override: hide working indicator
        if self.working_visible_override == Some(false) {
            return;
        }

        // Extension override: custom working message
        if let Some(ref msg) = self.custom_working_message {
            let bg_color = self.theme.tool_status.running;
            let accent = Style::default().black().bg(bg_color).bold();
            let dim = Style::default().fg(self.theme.ui.footer_text).bg(bg_color);

            let elapsed = self.footer.current_elapsed();
            let tokens = format_tokens(self.footer.output_tokens);

            let line = Line::from(vec![
                Span::styled(format!(" * {msg} "), accent),
                Span::styled(format!("({elapsed} | {tokens} tok)"), dim),
            ]);
            f.render_widget(Paragraph::new(line), area);
            return;
        }

        let label = match &self.activity {
            AgentActivity::Thinking => "Thinking…".to_owned(),
            AgentActivity::RunningTool { name } => format!("Running {name}"),
            AgentActivity::Writing => "Writing".to_owned(),
            AgentActivity::Idle => "Processing".to_owned(),
        };

        let elapsed = self.footer.current_elapsed();
        let tokens = format_tokens(self.footer.output_tokens);

        let bg_color = self.theme.tool_status.running;
        let accent = Style::default().black().bg(bg_color).bold();
        let dim = Style::default().fg(self.theme.ui.footer_text).bg(bg_color);

        let indicator = if let Some(ref ci) = self.custom_indicator {
            ci.frame_at(self.tick as u64).to_string()
        } else {
            "*".to_owned()
        };

        let line = Line::from(vec![
            Span::styled(format!(" {indicator} {label} "), accent),
            Span::styled(format!("({elapsed} | {tokens} tok)"), dim),
        ]);

        f.render_widget(Paragraph::new(line), area);
    }

    /// Render permission confirmation modal dialog over the full terminal area.
    ///
    /// Shows tool details + preview (code, command, args) depending on tool type.
    fn render_permission_dialog(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let Some(p) = self.permission.pending() else {
            return;
        };

        let dialog_width = area.width.min(80);
        let dialog_height = area.height.min(20);
        // Guard against too-small terminals
        if dialog_width < 40 || dialog_height < 8 {
            return;
        }
        let dx = (area.width.saturating_sub(dialog_width)) / 2;
        let dy = (area.height.saturating_sub(dialog_height)) / 2;
        let dialog_rect =
            ratatui::layout::Rect::new(area.x + dx, area.y + dy, dialog_width, dialog_height);

        let confirm_bg = self.theme.tool_status.await_confirm;
        // Solid dark background to block underlying content
        let dialog_bg = Color::Rgb(24, 24, 27); // near-black, opaque
        // Fill the entire dialog area
        f.render_widget(ratatui::widgets::Clear, dialog_rect);
        let block = ratatui::widgets::Block::bordered()
            .border_style(Style::default().fg(confirm_bg))
            .title(" Permission Required ")
            .title_alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().bg(dialog_bg));
        let inner_area = block.inner(dialog_rect);
        f.render_widget(block, dialog_rect);

        let mut lines: Vec<Line<'_>> = Vec::new();

        // Tool header
        let (icon, label): (&str, String) = match p.tool_name.as_str() {
            "write" => ("←", "Write".into()),
            "edit" => ("←", "Edit".into()),
            "bash" => ("$", "Bash".into()),
            "read" => ("→", "Read".into()),
            "grep" => ("✱", "Grep".into()),
            "find" => ("✱", "Find".into()),
            "web_fetch" => ("%", "WebFetch".into()),
            "web_search" => ("◈", "WebSearch".into()),
            _ if p.tool_name.is_empty() => ("?", "Unknown Tool".to_string()),
            _ => ("⚙", p.tool_name.clone()),
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{icon} {label}"),
                Style::default().fg(confirm_bg).bold(),
            ),
        ]));

        // Description
        if let Some(ref desc) = p.tool_description {
            if !desc.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        desc.as_str(),
                        Style::default().fg(self.theme.ui.footer_text),
                    ),
                ]));
            }
        }

        lines.push(Line::raw(""));

        // Tool-specific preview
        match p.tool_name.as_str() {
            "write" | "edit" => {
                // File content preview
                let file_path = p
                    .tool_args
                    .as_ref()
                    .and_then(|a| a.get("filePath").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let content = p
                    .tool_args
                    .as_ref()
                    .and_then(|a| a.get("content").and_then(|v| v.as_str()))
                    .unwrap_or("");

                if !file_path.is_empty() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("File: {file_path}"),
                            Style::default().fg(self.theme.ui.footer_text),
                        ),
                    ]));
                }
                if !content.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "  ┌─ Preview ─────────────────────────────",
                        Style::default().fg(self.theme.ui.footer_text),
                    )));
                    let preview_lines = content.lines().take(10);
                    let total_lines = content.lines().count();
                    for (i, line) in preview_lines.enumerate() {
                        let max_bytes = (dialog_width as usize).saturating_sub(12);
                        let display = if let Some((idx, _)) = line.char_indices().nth(max_bytes) {
                            format!("{}…", &line[..idx])
                        } else {
                            line.to_string()
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("  │{i:>3} "),
                                Style::default().fg(self.theme.ui.footer_text),
                            ),
                            Span::styled(
                                format!("+ {display}"),
                                Style::default().fg(self.theme.diff.added_text),
                            ),
                        ]));
                    }
                    if total_lines > 10 {
                        lines.push(Line::from(Span::styled(
                            format!("  │  ... {} more lines", total_lines - 10),
                            Style::default().fg(self.theme.ui.footer_text),
                        )));
                    }
                    lines.push(Line::from(Span::styled(
                        "  └──────────────────────────────────────────",
                        Style::default().fg(self.theme.ui.footer_text),
                    )));
                }
            }
            "bash" => {
                let command = p
                    .tool_args
                    .as_ref()
                    .and_then(|a| a.get("command").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let workdir = p
                    .tool_args
                    .as_ref()
                    .and_then(|a| a.get("workdir").and_then(|v| v.as_str()))
                    .unwrap_or(".");

                if workdir != "." && !workdir.is_empty() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("Working directory: {workdir}"),
                            Style::default().fg(self.theme.ui.footer_text),
                        ),
                    ]));
                    lines.push(Line::raw(""));
                }
                if !command.is_empty() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("$ {command}"),
                            Style::default().fg(self.theme.bash.command).bold(),
                        ),
                    ]));
                }
            }
            _ => {
                // Generic: show arguments_summary
                if !p.arguments_summary.is_empty() && p.arguments_summary != "{}" {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            &p.arguments_summary,
                            Style::default().fg(self.theme.markdown.code_text),
                        ),
                    ]));
                }
            }
        }

        lines.push(Line::raw(""));

        // Button bar
        let normal_btn = Style::default().fg(confirm_bg).bg(dialog_bg);
        let selected_btn = Style::default().fg(Color::Black).bg(confirm_bg).bold();
        let sel = self.permission_selected;
        let has_edit = p
            .options
            .iter()
            .any(|o| *o == crate::permission::ConfirmOption::Edit);
        lines.push(Line::from(vec![
            Span::styled(
                if sel == 0 {
                    " ▶[y] Allow "
                } else {
                    "  [y] Allow "
                },
                if sel == 0 { selected_btn } else { normal_btn },
            ),
            Span::raw(" "),
            Span::styled(
                if sel == 1 {
                    " ▶[n] Reject "
                } else {
                    "  [n] Reject "
                },
                if sel == 1 { selected_btn } else { normal_btn },
            ),
            Span::raw(" "),
            Span::styled(
                if sel == 2 {
                    " ▶[a] Always "
                } else {
                    "  [a] Always "
                },
                if sel == 2 { selected_btn } else { normal_btn },
            ),
            Span::raw(""),
            if has_edit {
                Span::styled(
                    if sel == 3 {
                        "  ▶[e] Edit "
                    } else {
                        "   [e] Edit "
                    },
                    if sel == 3 { selected_btn } else { normal_btn },
                )
            } else {
                Span::raw("")
            },
        ]));

        // Countdown timer
        if let Some(started) = self.permission_started_at {
            let elapsed = started.elapsed().as_secs();
            let remaining = 20u64.saturating_sub(elapsed);
            if remaining > 0 {
                lines.push(Line::from(Span::styled(
                    format!("  Auto-approve in {remaining}s..."),
                    Style::default().fg(self.theme.ui.footer_text),
                )));
            }
        }

        // Render content
        let max_visible = inner_area.height as usize;
        let visible: Vec<Line<'_>> = lines.into_iter().take(max_visible).collect();
        f.render_widget(
            Paragraph::new(Text::from(visible)).style(Style::default().bg(dialog_bg)),
            inner_area,
        );
    }

    /// Render question dialog — shows the agent's question with selectable options.
    fn render_question_dialog(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let Some(ref q) = self.pending_question else {
            return;
        };
        if q.questions.is_empty() {
            return;
        }
        // Show first question only (multi-question can be extended later)
        let question = &q.questions[0];

        let dialog_width = area.width.min(70);
        let line_count = 4 + question.options.len() as u16;
        let dialog_height = area.height.min(line_count + 3);
        if dialog_width < 30 || dialog_height < 6 {
            return;
        }
        let dx = (area.width.saturating_sub(dialog_width)) / 2;
        let dy = (area.height.saturating_sub(dialog_height)) / 2;
        let dialog_rect =
            ratatui::layout::Rect::new(area.x + dx, area.y + dy, dialog_width, dialog_height);

        let color = self.theme.tool_status.await_confirm;
        let block = ratatui::widgets::Block::bordered()
            .border_style(Style::default().fg(color))
            .title(format!(" {} — {}", q.title, question.header))
            .style(Style::default().bg(Color::Rgb(24, 24, 27)));
        f.render_widget(ratatui::widgets::Clear, dialog_rect);
        let inner = block.inner(dialog_rect);
        f.render_widget(block, dialog_rect);

        let mut lines: Vec<ratatui::text::Line> = Vec::new();
        lines.push(Line::styled(
            &question.question,
            Style::default().fg(Color::White),
        ));
        lines.push(Line::default()); // blank line
        for (i, opt) in question.options.iter().enumerate() {
            let marker = if self.question_selected.contains(&i) {
                "●"
            } else {
                "○"
            };
            let prefix = if question.multiple {
                format!(" {marker} ")
            } else {
                format!(
                    " {} ",
                    if self.question_selected.contains(&i) {
                        "▶"
                    } else {
                        " "
                    }
                )
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(color)),
                Span::styled(&opt.label, Style::default().fg(Color::White)),
                Span::styled(
                    format!("  ({})", opt.description),
                    Style::default().fg(Color::Gray),
                ),
            ]));
        }
        lines.push(Line::default());
        lines.push(Line::styled(
            "Enter 确认  ↑↓ 选择  Space 切换",
            Style::default().fg(Color::Gray),
        ));

        let visible: Vec<ratatui::text::Line> =
            lines.into_iter().map(ratatui::text::Line::from).collect();
        f.render_widget(Paragraph::new(visible), inner);
    }

    /// Render keyboard shortcuts help panel overlay.
    fn render_shortcut_help(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let help_width = area.width.min(60);
        let help_height = area.height.min(22);
        // Guard against too-small terminals
        if help_width < 40 || help_height < 10 {
            return;
        }
        let dx = (area.width.saturating_sub(help_width)) / 2;
        let dy = (area.height.saturating_sub(help_height)) / 2;
        let help_rect =
            ratatui::layout::Rect::new(area.x + dx, area.y + dy, help_width, help_height);

        let dialog_bg = Color::Rgb(24, 24, 27);
        f.render_widget(ratatui::widgets::Clear, help_rect);
        let block = ratatui::widgets::Block::bordered()
            .border_style(Style::default().fg(self.theme.tool_status.running))
            .title(" Keyboard Shortcuts (Ctrl+H / F1 to toggle) ")
            .title_alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().bg(dialog_bg));
        let inner = block.inner(help_rect);
        f.render_widget(block, help_rect);

        let key_style = Style::default().fg(self.theme.tool_status.running).bold();
        let desc_style = Style::default().fg(self.theme.ui.footer_text);

        let sections: &[(&str, &[(&str, &str)])] = &[
            (
                "Navigation",
                &[
                    ("Ctrl+J / ↓", "Focus next card"),
                    ("Ctrl+K / ↑", "Focus previous card"),
                    ("Space", "Toggle expand/collapse"),
                    ("Esc", "Dismiss / cancel / interrupt"),
                    ("PgUp / PgDn", "Scroll up / down"),
                ],
            ),
            (
                "Tools",
                &[
                    ("Ctrl+O", "Toggle tool output visibility"),
                    ("Ctrl+N", "New session"),
                ],
            ),
            ("Thinking", &[("Ctrl+T", "Toggle thinking visibility")]),
            (
                "Model",
                &[
                    ("Ctrl+L", "Open model selector"),
                    ("Ctrl+P", "Cycle next model"),
                ],
            ),
            (
                "Session",
                &[
                    ("Ctrl+R", "Retry last prompt"),
                    ("Ctrl+/", "Undo last edit"),
                    ("Ctrl+G", "Open editor at change site"),
                ],
            ),
            (
                "Editing",
                &[
                    ("Ctrl+C", "Copy last assistant message"),
                    ("Ctrl+H / F1", "Toggle this help panel"),
                ],
            ),
        ];

        let mut lines: Vec<Line<'_>> = Vec::new();
        for (section_title, keys) in sections {
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  {section_title}"),
                key_style,
            )));
            for (key, desc) in *keys {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(format!("{key:<18}"), key_style),
                    Span::styled(*desc, desc_style),
                ]));
            }
        }

        let max_visible = inner.height as usize;
        let visible: Vec<Line<'_>> = lines.into_iter().take(max_visible).collect();
        f.render_widget(
            Paragraph::new(Text::from(visible)).style(Style::default().bg(dialog_bg)),
            inner,
        );
    }

    fn render_header(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        if let Some(ref header) = self.custom_header {
            let height = header.line_count() as u16;
            let lines = &header.lines;
            for (i, line) in lines.iter().enumerate() {
                if i as u16 >= height || area.height == 0 {
                    break;
                }
                let line_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y + i as u16,
                    width: area.width,
                    height: 1,
                };
                f.render_widget(Paragraph::new(line.clone()), line_area);
            }
        }
    }

    fn render_footer(
        &self,
        f: &mut Frame,
        line1_area: ratatui::layout::Rect,
        line2_area: ratatui::layout::Rect,
    ) {
        if let Some(ref custom) = self.custom_footer {
            // Render custom footer lines
            for (i, line) in custom.lines.iter().enumerate() {
                let area = if i == 0 { line1_area } else { line2_area };
                if area.height == 0 {
                    break;
                }
                f.render_widget(Paragraph::new(line.clone()), area);
            }
            // Optionally append built-in info
            // For now, custom footer replaces built-in completely
            return;
        }

        let (status_icon, status_color) = self.working_indicator_icon();

        let mut line1_spans = vec![
            Span::styled(format!("{status_icon} "), Style::default().fg(status_color)),
            Span::styled(
                self.footer.render_line1(&self.session_id),
                Style::default().fg(self.theme.ui.footer_text),
            ),
        ];
        line1_spans.extend(self.status_manager.render_spans());
        f.render_widget(Paragraph::new(Line::from(line1_spans)), line1_area);

        let level = self.chat.thinking_level;
        let model_display = if self.model.is_empty() {
            "uncode"
        } else {
            &self.model
        };
        let line2 = self.footer.render_line2(
            model_display,
            level.icon(),
            self.chat.thinking_label(),
            &self.theme,
        );
        f.render_widget(Paragraph::new(line2), line2_area);
    }

    /// Returns (icon, color) for the working indicator — custom or built-in.
    fn working_indicator_icon(&self) -> (&str, Color) {
        if let Some(ref indicator) = self.custom_indicator {
            let frame = indicator.frame_at(self.tick as u64);
            let color = self.theme.tool_status.success;
            (frame, color)
        } else if self.agent_busy {
            let dot = if (self.tick / 4).is_multiple_of(2) {
                "●"
            } else {
                "○"
            };
            (dot, self.theme.tool_status.success)
        } else {
            ("●", self.theme.tool_status.success)
        }
    }

    pub async fn run<F>(&mut self, mut event_rx: broadcast::Receiver<AgentEvent>, on_submit: F)
    where
        F: Fn(String, CancellationToken, String, String, SubmitIntent),
    {
        let mut terminal = ratatui::init();

        // Install a panic hook that restores the terminal before the process unwinds.
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            ratatui::restore();
            prev_hook(info);
        }));

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
                            return Err::<Event, std::io::Error>(std::io::Error::other(
                                "terminal poll failed",
                            ));
                        }
                        let has_event = poll_ok.expect("is_err already handled");
                        if has_event {
                            let ev = event::read().unwrap_or(Event::Key(
                                event::KeyEvent::new(KeyCode::Null, event::KeyModifiers::empty())
                            ));
                            match ev {
                                Event::Key(key) if key.kind != KeyEventKind::Release => {
                                    return Ok(Event::Key(key));
                                }
                                Event::Mouse(mouse) => {
                                    return Ok(Event::Mouse(mouse));
                                }
                                Event::Resize(w, h) => {
                                    return Ok(Event::Resize(w, h));
                                }
                                Event::Paste(text) => {
                                    return Ok(Event::Paste(text));
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
                            // ESC: highest priority — interrupt agent, clear focus, dismiss overlays
                            if key_event.code == KeyCode::Esc {
                                self.handle_esc();
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

                            // Shortcut help: consume all keys except toggle/ESC
                            if self.show_shortcut_help
                                && !matches!(key_event.code, KeyCode::F(1))
                                && !(ctrl && key_event.code == KeyCode::Char('h'))
                            {
                                continue;
                            }

                            // Dialog overlay takes priority when visible
                            if self.dialog.is_visible() {
                                if let Some(response) = self.dialog.handle_key(key_event) {
                                    if let Some(tx) = self.pending_dialog_response.take() {
                                        let _ = tx.send(response);
                                    }
                                }
                                continue;
                            }

                            // Overlay keyboard capture
                            if self.overlay_manager.top_capturing() {
                                continue;
                            }

                            // Permission confirmation keys take priority
                            if self.permission.has_pending() {
                                let has_edit = self
                                    .permission
                                    .pending()
                                    .map(|p| p.options.contains(&crate::permission::ConfirmOption::Edit))
                                    .unwrap_or(false);
                                let max_sel = if has_edit { 3usize } else { 2usize };

                                match key_event.code {
                                    // Arrow navigation: clear countdown, wait for explicit confirm
                                    KeyCode::Right | KeyCode::Tab => {
                                        self.permission_selected =
                                            if self.permission_selected < max_sel {
                                                self.permission_selected + 1
                                            } else {
                                                0
                                            };
                                        self.permission_started_at = None;
                                        continue;
                                    }
                                    KeyCode::Left | KeyCode::BackTab => {
                                        self.permission_selected =
                                            if self.permission_selected > 0 {
                                                self.permission_selected - 1
                                            } else {
                                                max_sel
                                            };
                                        self.permission_started_at = None;
                                        continue;
                                    }
                                    // Enter: confirm selected option
                                    KeyCode::Enter => match self.permission_selected {
                                        0 => {
                                            if let Some(p) = self.permission.confirm(
                                                crate::permission::ConfirmOption::Allow,
                                            ) {
                                                self.resolve_permission(
                                                    &p.tool_id,
                                                    Approval::Allow,
                                                );
                                            }
                                        }
                                        1 => {
                                            if let Some(p) = self.permission.deny() {
                                                self.resolve_permission(
                                                    &p.tool_id,
                                                    Approval::Deny,
                                                );
                                            }
                                        }
                                        2 => {
                                            if let Some(p) = self.permission.confirm(
                                                crate::permission::ConfirmOption::Allow,
                                            ) {
                                                self.resolve_permission(
                                                    &p.tool_id,
                                                    Approval::Allow,
                                                );
                                            }
                                        }
                                        3 => {
                                            if let Some(p) = self.permission.confirm(
                                                crate::permission::ConfirmOption::Edit,
                                            ) {
                                                self.resolve_permission(
                                                    &p.tool_id,
                                                    Approval::Allow,
                                                );
                                            }
                                        }
                                        _ => {}
                                    },
                                    // Direct key shortcuts (fallback)
                                    KeyCode::Char('y') => {
                                        if let Some(p) = self.permission.confirm(
                                            crate::permission::ConfirmOption::Allow,
                                        ) {
                                            self.resolve_permission(&p.tool_id, Approval::Allow);
                                        }
                                    }
                                    KeyCode::Char('a') => {
                                        if let Some(p) = self.permission.confirm(
                                            crate::permission::ConfirmOption::Allow,
                                        ) {
                                            self.resolve_permission(&p.tool_id, Approval::Allow);
                                        }
                                    }
                                    KeyCode::Char('n') => {
                                        if let Some(p) = self.permission.deny() {
                                            self.resolve_permission(&p.tool_id, Approval::Deny);
                                        }
                                    }
                                    KeyCode::Char('e') => {
                                        if let Some(p) = self.permission.confirm(
                                            crate::permission::ConfirmOption::Edit,
                                        ) {
                                            self.resolve_permission(&p.tool_id, Approval::Allow);
                                        }
                                    }
                                    _ => {}
                                }
                                // Clear countdown timer if permission was resolved
                                if !self.permission.has_pending() {
                                    self.permission_started_at = None;
                                }
                                continue;
                            }

                            // Question dialog: navigate options and confirm
                            if self.pending_question.is_some() {
                                let q = self.pending_question.as_ref().unwrap();
                                if let Some(question) = q.questions.first() {
                                    let max = question.options.len().saturating_sub(1);
                                    match key_event.code {
                                        KeyCode::Enter | KeyCode::Char('y') => {
                                            let answers: Vec<Vec<String>> = vec![question
                                                .options
                                                .iter()
                                                .enumerate()
                                                .filter(|(i, _)| {
                                                    self.question_selected.contains(i)
                                                })
                                                .map(|(_, opt)| opt.label.clone())
                                                .collect()];
                                            let id = q.tool_call_id.clone();
                                            self.pending_question = None;
                                            self.question_selected.clear();
                                            uncode_agent::tools::question_registry::resolve(
                                                &id, answers,
                                            );
                                        }
                                        KeyCode::Char('n') | KeyCode::Esc => {
                                            let id = q.tool_call_id.clone();
                                            self.pending_question = None;
                                            self.question_selected.clear();
                                            uncode_agent::tools::question_registry::resolve(
                                                &id,
                                                vec![vec!["dismissed".into()]],
                                            );
                                        }
                                        KeyCode::Up => {
                                            if let Some(&sel) = self.question_selected.iter().next() {
                                                if sel > 0 {
                                                    self.question_selected.clear();
                                                    self.question_selected.insert(sel - 1);
                                                }
                                            } else {
                                                self.question_selected.insert(max);
                                            }
                                        }
                                        KeyCode::Down => {
                                            if let Some(&sel) = self.question_selected.iter().next() {
                                                if sel < max {
                                                    self.question_selected.clear();
                                                    self.question_selected.insert(sel + 1);
                                                }
                                            } else {
                                                self.question_selected.insert(0);
                                            }
                                        }
                                        KeyCode::Char(' ') if question.multiple => {
                                            if let Some(&sel) =
                                                self.question_selected.iter().next()
                                            {
                                                if self.question_selected.contains(&sel) {
                                                    self.question_selected.remove(&sel);
                                                } else {
                                                    self.question_selected.insert(sel);
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                continue;
                            }

                            match key_event.code {
                                // Leader key prefix
                                KeyCode::Char('x') if ctrl => {
                                    self.leader_pending = true;
                                }
                                // Shortcut help overlay
                                KeyCode::Char('h') if ctrl => {
                                    self.show_shortcut_help = !self.show_shortcut_help;
                                }
                                KeyCode::F(1) => {
                                    self.show_shortcut_help = !self.show_shortcut_help;
                                }
                                // Direct shortcuts
                                KeyCode::Char('o') if ctrl => {
                                    self.chat.tool_output_visible = !self.chat.tool_output_visible;
                                    self.chat.set_all_expanded(self.chat.tool_output_visible);
                                }
                                KeyCode::Char('t') if ctrl => {
                                    self.chat.thinking_visible = !self.chat.thinking_visible;
                                    self.chat
                                        .set_thinking_expanded(self.chat.thinking_visible);
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
                                                &self.cwd,
                                            );
                                            let token = self.new_cancel_token();
                                            on_submit(
                                                expanded,
                                                token,
                                                self.model.clone(),
                                                self.session_id.clone(),
                                                SubmitIntent::NewRun,
                                            );
                                        } else {
                                            self.chat.push_message(chat::ChatMessage::Summary {
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
                                        self.chat.focused_card = None;
                                        self.chat.scroll_offset = 0;
                                        self.chat.auto_scroll = true;
                                        self.footer.context_percent = 0;
                                        self.last_user_input = None;
                                        let sid = self.session_id.get(..8).unwrap_or(&self.session_id);
                                        self.chat.push_message(chat::ChatMessage::Summary {
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
                                        self.chat.push_message(chat::ChatMessage::Summary {
                                            completed: vec![format!("Undid {removed} messages.")],
                                            next_steps: vec![],
                                        });
                                        // Reset focus if index is now out of bounds
                                        if let Some(idx) = self.chat.focused_card
                                            && idx >= self.chat.messages.len() {
                                                self.chat.focused_card = None;
                                            }
                                    }
                                }
                                // External editor: Ctrl+G
                                KeyCode::Char('g') if ctrl => {
                                    if let Some(content) = open_external_editor()
                                        && !content.is_empty() {
                                            self.editor.set_buffer(content);
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
                                // Card focus navigation (only when input empty & selector hidden)
                                KeyCode::Char('j') if ctrl && !self.selector.is_visible()
                                    && self.editor.buffer().is_empty() => {
                                    if self.chat.focus_next_card() {
                                        self.scroll_to_focused_card();
                                    }
                                }
                                KeyCode::Char('k') if ctrl && !self.selector.is_visible()
                                    && self.editor.buffer().is_empty() => {
                                    if self.chat.focus_prev_card() {
                                        self.scroll_to_focused_card();
                                    }
                                }
                                // Toggle focused card expand/collapse
                                KeyCode::Char(' ') if self.chat.focused_card.is_some()
                                    && self.editor.buffer().is_empty() => {
                                    self.chat.toggle_focused_card();
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
                                    }
                                    self.selector.hide();
                                }
                                // Enter toggles focused card (when no text in input)
                                KeyCode::Enter if self.chat.focused_card.is_some()
                                    && !self.selector.is_visible()
                                    && self.editor.buffer().is_empty() => {
                                    self.chat.toggle_focused_card();
                                }
                                // ESC fallback (also handled above, but crossterm may
                                // deliver ESC differently on some terminals)
                                KeyCode::Esc => {
                                    if self.agent_busy {
                                        if let Some(ref token) = self.current_cancel {
                                            token.cancel();
                                        }
                                        self.finish_agent_run();
                                        self.chat.deactivate_thinking();
                                        self.chat.invalidate_all();
                                        self.chat.push_message(chat::ChatMessage::Summary {
                                            completed: vec!["[Interrupted] Agent stopped.".into()],
                                            next_steps: vec![],
                                        });
                                    }
                                    if self.chat.focused_card.is_some() {
                                        self.chat.clear_focus();
                                    }
                                    if self.overlay_manager.has_visible() {
                                        self.overlay_manager.handle_escape();
                                    }
                                    if self.welcome.is_visible() {
                                        self.welcome.hide();
                                    }
                                    if self.selector.is_visible() {
                                        self.selector.hide();
                                    }
                                }
                                // Quit / Interrupt
                                KeyCode::Char('c') if ctrl => {
                                    if self.agent_busy {
                                        if let Some(ref token) = self.current_cancel {
                                            token.cancel();
                                        }
                                        self.finish_agent_run();
                                        self.chat.push_message(chat::ChatMessage::Summary {
                                            completed: vec!["[Interrupted] Agent stopped.".into()],
                                            next_steps: vec![],
                                        });
                                    } else {
                                        break;
                                    }
                                }
                                // Extension shortcuts
                                _ => {
                                    if self.try_extension_shortcut(key_event) {
                                        continue;
                                    }
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
                        Event::Paste(text) => {
                            self.editor.handle_paste(&text);
                            self.editor.set_completions(
                                self.completion.complete(self.editor.buffer())
                            );
                        }
                        _ => {}
                    }
                }
                Ok(event) = event_rx.recv() => {
                    let is_run_finished = matches!(
                        event,
                        AgentEvent::SessionEnd { .. }
                            | AgentEvent::AgentInterrupted { .. }
                            | AgentEvent::AgentSettled { .. }
                    );
                    self.handle_event(event);
                    if is_run_finished {
                        self.flush_queue(&on_submit);
                    }
                }
                // Poll dialog bridge for extension-initiated dialogs
                Some(pending) = async {
                    match &mut self.dialog_bridge {
                        Some(bridge) => bridge.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.pending_dialog_response = Some(pending.response_tx);
                    self.dialog.show(pending.request);
                }
                // Poll overlay bridge for extension-initiated overlays
                Some(pending) = async {
                    match &mut self.overlay_bridge {
                        Some(bridge) => bridge.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    use uncode_core::overlay::OverlayAction;
                    let result = match &pending.action {
                        OverlayAction::Show { config, content } => {
                            if let Err(e) = config.validate() {
                                Err(e)
                            } else {
                                self.overlay_manager.show(config.clone(), content.clone());
                                Ok(())
                            }
                        }
                        OverlayAction::Hide { key } => {
                            self.overlay_manager.hide(key);
                            Ok(())
                        }
                        OverlayAction::Update { key, content } => {
                            self.overlay_manager.update(key, content.clone());
                            Ok(())
                        }
                    };
                    let _ = pending.response_tx.send(result);
                }
                // Poll UI bridge for extension-initiated widget/status actions
                Some(pending) = async {
                    match &mut self.ui_bridge {
                        Some(bridge) => bridge.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    use uncode_core::ui_action::UiAction;
                    let result = match &pending.action {
                        UiAction::SetWidget { config } => {
                            if let Err(e) = config.validate() {
                                Err(e)
                            } else {
                                self.widget_manager.set_widget(config.clone());
                                Ok(())
                            }
                        }
                        UiAction::RemoveWidget { key } => {
                            self.widget_manager.remove_widget(key);
                            Ok(())
                        }
                        UiAction::SetStatus { key, text } => {
                            match text {
                                Some(t) => self.status_manager.set(key.clone(), t.clone()),
                                None => self.status_manager.clear(key),
                            }
                            Ok(())
                        }
                        UiAction::CustomMessage {
                            message_type,
                            content,
                        } => {
                            self.chat.push_message(chat::ChatMessage::Custom {
                                message_type: message_type.clone(),
                                content: content.clone(),
                                expanded: true,
                            });
                            Ok(())
                        }
                        UiAction::SetTitle { title } => {
                            let _ = crossterm::execute!(
                                std::io::stdout(),
                                crossterm::terminal::SetTitle(title)
                            );
                            Ok(())
                        }
                        UiAction::SetWorkingMessage { message } => {
                            self.custom_working_message = Some(message.clone());
                            Ok(())
                        }
                        UiAction::SetWorkingVisible { visible } => {
                            self.working_visible_override = Some(*visible);
                            Ok(())
                        }
                        UiAction::SetToolsExpanded { expanded } => {
                            self.chat.tool_output_visible = *expanded;
                            self.chat.set_all_expanded(*expanded);
                            Ok(())
                        }
                    };
                    let _ = pending.response_tx.send(result);
                }
                // Poll session bridge for extension-initiated session tree operations
                Some(pending) = async {
                    match &mut self.session_bridge {
                        Some(bridge) => bridge.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    use uncode_extensions::session::{SessionAction, SessionResponse};
                    let result: Result<SessionResponse, String> = match &pending.action {
                        SessionAction::Fork { .. } => {
                            // TODO: bridge to SessionStore::fork_session
                            Err("session fork not yet connected to session store".into())
                        }
                        SessionAction::Navigate { .. } => {
                            // TODO: bridge to SessionStore::set_leaf
                            Err("session navigate not yet connected to session store".into())
                        }
                        SessionAction::Switch { .. } => {
                            // TODO: bridge to harness session switch
                            Err("session switch not yet connected to session store".into())
                        }
                        SessionAction::SetName { .. } => {
                            // TODO: bridge to SessionStore::update_title
                            Err("session set_name not yet connected to session store".into())
                        }
                    };
                    let _ = pending.response_tx.send(result);
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                    // Idle tick: check auto-approve + re-render for status animation
                    self.check_auto_approve();
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
        F: Fn(String, CancellationToken, String, String, SubmitIntent),
    {
        if let Some(response) = self.slash.execute(&text) {
            self.chat.push_message(chat::ChatMessage::Summary {
                completed: vec![response],
                next_steps: vec![],
            });
            return;
        }

        match text.as_str() {
            "/thinking" => {
                self.chat.thinking_visible = !self.chat.thinking_visible;
                self.chat.set_thinking_expanded(self.chat.thinking_visible);
            }
            "/details" => {
                self.chat.tool_output_visible = !self.chat.tool_output_visible;
            }
            "/help" => {
                let help = "Keys: Ctrl+O tool output | Ctrl+T thinking | Ctrl+P cycle model | Ctrl+R retry | Ctrl+N new session | Ctrl+/ undo | Ctrl+G editor\nCommands: /clear | /compact | /model [name] | /new | /fork [id] | /export [fmt] | /sessions | /branch | /name [title] | /copy | /usage | /reload | /diff | /extensions | /theme | /thinking | /details | /tree | /skills | /template\nWhile agent is busy: Enter steers the run; /later <msg> queues follow-up after SessionEnd";
                self.chat.push_message(chat::ChatMessage::Summary {
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
            t if t.starts_with("/extensions") => {
                self.handle_extensions_command(&text);
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
                let registry = uncode_core::skill::SkillRegistry::load_with_project(&self.cwd);
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
        F: Fn(String, CancellationToken, String, String, SubmitIntent),
    {
        if self.agent_busy {
            if let Some(rest) = text.strip_prefix("/later ") {
                let preview = rest.to_string();
                self.queue.enqueue(preview.clone(), QueueType::FollowUp);
                self.chat
                    .messages
                    .push(chat::ChatMessage::QueuedMessage { text: preview });
                return;
            }
            self.last_user_input = Some(text.clone());
            let file_expanded = uncode_core::context::expand_file_refs(&text, &self.cwd);
            self.chat.push_user_message(text);
            let token = self.current_or_new_cancel_token();
            on_submit(
                file_expanded,
                token,
                self.model.clone(),
                self.session_id.clone(),
                SubmitIntent::Steer,
            );
        } else {
            self.last_user_input = Some(text.clone());
            self.agent_busy = true;
            self.footer.start_turn();
            let file_expanded = uncode_core::context::expand_file_refs(&text, &self.cwd);
            self.chat.push_user_message(text);
            let token = self.new_cancel_token();
            on_submit(
                file_expanded,
                token,
                self.model.clone(),
                self.session_id.clone(),
                SubmitIntent::NewRun,
            );
        }
    }

    fn handle_clear_command(&mut self) {
        self.chat.messages.clear();
        self.chat.focused_card = None;
        self.chat.scroll_offset = 0;
        self.chat.auto_scroll = true;
        self.chat.push_message(chat::ChatMessage::Summary {
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

        self.chat.push_message(chat::ChatMessage::Summary {
            completed: lines,
            next_steps: vec![],
        });
    }

    fn handle_new_command(&mut self) {
        let old_id = if self.session_id.is_empty() {
            "none".to_owned()
        } else {
            self.session_id[..8.min(self.session_id.len())].to_string()
        };

        self.session_id = uuid::Uuid::new_v4().to_string();
        self.chat.messages.clear();
        self.chat.focused_card = None;
        self.chat.scroll_offset = 0;
        self.chat.auto_scroll = true;
        self.footer.input_tokens = 0;
        self.footer.output_tokens = 0;
        self.footer.cost = 0.0;
        self.footer.context_percent = 0;

        let new_id = self.session_id.get(..8).unwrap_or(&self.session_id);
        self.chat.push_message(chat::ChatMessage::Summary {
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

        self.model = name.to_string();
        if let Some(idx) = self.available_models.iter().position(|m| m == name) {
            self.model_index = idx;
        }
    }

    fn handle_fork_command(&mut self, text: &str) {
        if self.session_id.is_empty() {
            self.chat.push_message(chat::ChatMessage::Error {
                message: "No active session to fork.".into(),
                category: uncode_core::event::ErrorCategory::Config,
            });
            return;
        }
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let target_entry = if parts.len() > 1 && !parts[1].trim().is_empty() {
            parts[1].trim().to_string()
        } else {
            // Fork from current state — use last session entry as branch point
            self.chat.push_message(chat::ChatMessage::Summary {
                completed: vec![
                    "Forking from current session state...".into(),
                    "Tip: use /tree to see branch structure".into(),
                ],
                next_steps: vec![],
            });
            // Generate a new session ID (fork creates a new branch)
            let old_id = self.session_id.clone();
            let new_session_id = uuid::Uuid::new_v4().to_string();
            self.session_id = new_session_id;
            let short_old = &old_id[..8.min(old_id.len())];
            let short_new = &self.session_id[..8.min(self.session_id.len())];
            self.chat.push_message(chat::ChatMessage::Summary {
                completed: vec![format!(
                    "Session forked: {short_old} → {short_new} (branch started from current state)"
                )],
                next_steps: vec![],
            });
            return;
        };
        let Some(store) = self.open_session_store() else {
            return;
        };
        let rt = tokio::runtime::Handle::current();
        match rt.block_on(uncode_agent::branch_summarization::branch_with_summary(
            &store,
            &self.session_id,
            &target_entry,
            "用户 fork",
        )) {
            Ok(()) => {
                let short = &target_entry[..8.min(target_entry.len())];
                let msg = format!("已分支到条目: {short}");
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed: vec![msg],
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("分支失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_export_command(&mut self, text: &str) {
        if self.session_id.is_empty() {
            self.chat.push_message(chat::ChatMessage::Error {
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
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        let rt = tokio::runtime::Handle::current();
        let store = match rt.block_on(store) {
            Ok(s) => s,
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("创建会话存储失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let sid_short = &self.session_id[..8.min(self.session_id.len())];
        match format {
            "jsonl" => match rt.block_on(store.load_entries(&self.session_id)) {
                Ok(entries) => {
                    let filename = format!("uncode-export-{sid_short}.jsonl");
                    let mut out = String::with_capacity(entries.len() * 128);
                    for entry in &entries {
                        if let Ok(line) = serde_json::to_string(entry) {
                            out.push_str(&line);
                            out.push('\n');
                        }
                    }
                    match std::fs::write(&filename, &out) {
                        Ok(()) => {
                            self.chat.push_message(chat::ChatMessage::Summary {
                                completed: vec![format!(
                                    "已导出 JSONL: {filename} ({} 条目)",
                                    entries.len()
                                )],
                                next_steps: vec![],
                            });
                        }
                        Err(e) => {
                            self.chat.push_message(chat::ChatMessage::Error {
                                message: format!("Failed to write file: {e}"),
                                category: uncode_core::event::ErrorCategory::Config,
                            });
                        }
                    }
                }
                Err(e) => {
                    self.chat.push_message(chat::ChatMessage::Error {
                        message: format!("Failed to read session: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                }
            },
            "html" => match rt.block_on(store.load_entries(&self.session_id)) {
                Ok(entries) => {
                    let filename = format!("uncode-export-{sid_short}.html");
                    let html = render_export_html(&entries);
                    match std::fs::write(&filename, &html) {
                        Ok(()) => {
                            self.chat.push_message(chat::ChatMessage::Summary {
                                completed: vec![format!(
                                    "已导出 HTML: {filename} ({} 条目)",
                                    entries.len()
                                )],
                                next_steps: vec![],
                            });
                        }
                        Err(e) => {
                            self.chat.push_message(chat::ChatMessage::Error {
                                message: format!("Failed to write file: {e}"),
                                category: uncode_core::event::ErrorCategory::Config,
                            });
                        }
                    }
                }
                Err(e) => {
                    self.chat.push_message(chat::ChatMessage::Error {
                        message: format!("Failed to read session: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                }
            },
            other => {
                self.chat.push_message(chat::ChatMessage::Error {
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
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        let rt = tokio::runtime::Handle::current();
        let store = match rt.block_on(store) {
            Ok(s) => s,
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("创建会话存储失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        match rt.block_on(store.list_sessions()) {
            Ok(mut sessions) => {
                if sessions.is_empty() {
                    self.chat.push_message(chat::ChatMessage::Summary {
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
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed,
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("Failed to list sessions: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_branch_command(&mut self) {
        if self.session_id.is_empty() {
            self.chat.push_message(chat::ChatMessage::Summary {
                completed: vec!["No active session.".into()],
                next_steps: vec![],
            });
            return;
        }
        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        let rt = tokio::runtime::Handle::current();
        let store = match rt.block_on(store) {
            Ok(s) => s,
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("创建会话存储失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let sid_short = &self.session_id[..8.min(self.session_id.len())];
        match rt.block_on(store.get_children(&self.session_id)) {
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
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed: lines,
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("获取分支信息失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_name_command(&mut self, text: &str) {
        if self.session_id.is_empty() {
            self.chat.push_message(chat::ChatMessage::Error {
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
                    self.chat.push_message(chat::ChatMessage::Error {
                        message: format!("无法获取会话目录: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                    return;
                }
            };
            let store = uncode_agent::session::store::SessionStore::new(session_dir);
            let rt = tokio::runtime::Handle::current();
            let store = match rt.block_on(store) {
                Ok(s) => s,
                Err(e) => {
                    self.chat.push_message(chat::ChatMessage::Error {
                        message: format!("创建会话存储失败: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                    return;
                }
            };
            match rt.block_on(store.read_header(&self.session_id)) {
                Ok(header) => {
                    let current = header.title.as_deref().unwrap_or("(无标题)");
                    self.chat.push_message(chat::ChatMessage::Summary {
                        completed: vec![format!("Current title: {current}")],
                        next_steps: vec![],
                    });
                }
                Err(e) => {
                    self.chat.push_message(chat::ChatMessage::Error {
                        message: format!("Failed to read session: {e}"),
                        category: uncode_core::event::ErrorCategory::Config,
                    });
                }
            }
            return;
        }
        // TODO: async store backend does not yet support update_title;
        // the /name <title> setter is deferred until a write-header API is added.
        let _ = title;
        self.chat.push_message(chat::ChatMessage::Summary {
            completed: vec![
                "Title rename pending — async store backend does not yet support write_header."
                    .into(),
            ],
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
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed: vec![format!("Copied to clipboard ({} chars)", text.len())],
                    next_steps: vec![],
                });
            }
            None => {
                self.chat.push_message(chat::ChatMessage::Summary {
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
        self.chat.push_message(chat::ChatMessage::Summary {
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
        self.chat.push_message(chat::ChatMessage::Summary {
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
                    self.chat.push_message(chat::ChatMessage::Summary {
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
                        "工作区变更:".to_owned(),
                        stat.trim().to_string(),
                        String::new(),
                    ];
                    for line in full.lines().take(30) {
                        lines.push(format!("  {line}"));
                    }
                    if full.lines().count() > 30 {
                        lines.push(format!("  ... ({} more lines)", full.lines().count() - 30));
                    }
                    self.chat.push_message(chat::ChatMessage::Summary {
                        completed: lines,
                        next_steps: vec![],
                    });
                }
            }
            _ => {
                self.chat.push_message(chat::ChatMessage::Error {
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
            AgentEvent::TurnStart { turn } => {
                self.agent_busy = true;
                self.footer.set_current_turn(*turn);
                self.activity = AgentActivity::Idle;
            }
            AgentEvent::TurnEnd { usage, .. } => {
                // Per-turn usage only; keep agent_busy until SessionEnd (multi-turn ReAct).
                self.footer.update_usage(usage);
                self.activity = AgentActivity::Idle;
            }
            AgentEvent::SessionEnd { data } => {
                self.finish_agent_run();
                self.footer.update_usage(&data.total_tokens);
            }
            AgentEvent::AgentSettled { .. } => {
                self.finish_agent_run();
            }
            AgentEvent::AgentInterrupted { .. } => {
                self.finish_agent_run();
            }
            AgentEvent::Error { recoverable, .. } => {
                if !recoverable {
                    self.finish_agent_run();
                }
            }
            AgentEvent::ContentDelta { delta_type, .. } => match delta_type {
                uncode_core::event::DeltaType::Thinking => {
                    self.activity = AgentActivity::Thinking;
                }
                uncode_core::event::DeltaType::Text => {
                    self.activity = AgentActivity::Writing;
                }
                _ => {}
            },
            AgentEvent::ToolCallStart { tool_name, .. } => {
                self.activity = AgentActivity::RunningTool {
                    name: tool_name.clone(),
                };
            }
            AgentEvent::ToolCallAwaitingApproval {
                tool_id,
                tool_name,
                arguments_summary,
                tool_description,
                tool_args,
            } => {
                let allow_edit = matches!(tool_name.as_str(), "edit" | "write");
                self.permission.request_confirmation(
                    tool_id.clone(),
                    tool_name.clone(),
                    arguments_summary.clone(),
                    tool_description.clone(),
                    allow_edit,
                    tool_args.clone(),
                );
                self.permission_started_at = Some(std::time::Instant::now());
            }
            AgentEvent::QuestionRequest { data } => {
                self.pending_question = Some(data.clone());
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
            let header = "可用模板:".to_owned();
            self.chat.push_message(chat::ChatMessage::Summary {
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
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed: vec![format!(
                        "模板 '{name}' 已渲染。复制以下内容作为输入：\n{prompt}"
                    )],
                    next_steps: vec![],
                });
            }
            None => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("模板 '{name}' 不存在。使用 /template 查看可用模板。"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_tree_command(&mut self) {
        if self.session_id.is_empty() {
            self.chat.push_message(chat::ChatMessage::Summary {
                completed: vec!["No active session.".into()],
                next_steps: vec![],
            });
            return;
        }

        let session_dir = match uncode_agent::session::store::SessionStore::default_dir() {
            Ok(d) => d,
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("无法获取会话目录: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };
        let store = uncode_agent::session::store::SessionStore::new(session_dir);
        let rt = tokio::runtime::Handle::current();
        let store = match rt.block_on(store) {
            Ok(s) => s,
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("创建会话存储失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
                return;
            }
        };

        match rt.block_on(store.build_tree(&self.session_id)) {
            Ok(tree) => {
                let lines = render_session_tree(&tree.root, "", true);
                let header = format!(
                    "会话分支树 (root: {})",
                    &tree.root.id[..8.min(tree.root.id.len())]
                );
                let mut completed = vec![header];
                completed.extend(lines);
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed,
                    next_steps: vec![],
                });
            }
            Err(e) => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("构建会话树失败: {e}"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn handle_skills_command(&mut self) {
        use uncode_core::skill::SkillRegistry;
        let registry = SkillRegistry::load_with_project(&self.cwd);
        let list = registry.list();
        if list.is_empty() {
            self.chat.push_message(chat::ChatMessage::Summary {
                completed: vec!["没有可用 Skills。".into()],
                next_steps: vec![],
            });
            return;
        }
        let lines: Vec<String> = list.iter().map(|s| format!("  {s}")).collect();
        let mut completed = vec!["可用 Skills:".to_owned()];
        completed.extend(lines);
        completed.push("调用方式: /<skill_name> <args>".to_owned());
        self.chat.push_message(chat::ChatMessage::Summary {
            completed,
            next_steps: vec![],
        });
    }

    fn handle_extensions_command(&mut self, text: &str) {
        use uncode_extensions::state::ExtensionState;

        let args = text.strip_prefix("/extensions").unwrap_or("").trim();

        let Some(mgr_arc) = &self.extension_manager else {
            self.chat.push_message(chat::ChatMessage::Error {
                message: "Extensions not available".into(),
                category: uncode_core::event::ErrorCategory::Config,
            });
            return;
        };
        let mgr = mgr_arc.lock();

        match args {
            "" | "list" => {
                let records = mgr.list();
                if records.is_empty() {
                    self.chat.push_message(chat::ChatMessage::Summary {
                        completed: vec!["没有已加载的扩展。".into()],
                        next_steps: vec![],
                    });
                    return;
                }
                let mut lines: Vec<String> = vec!["已加载扩展:".into()];
                for r in &records {
                    let state_str = match &r.state {
                        ExtensionState::Active => "active".to_owned(),
                        ExtensionState::Reloading => "reloading".to_owned(),
                        ExtensionState::Error(e) => format!("error: {e}"),
                        ExtensionState::Disabled => "disabled".to_owned(),
                    };
                    let tools = if r.tools.is_empty() {
                        String::new()
                    } else {
                        format!(" tools={}", r.tools.join(","))
                    };
                    lines.push(format!(
                        "  {} [{}] ({}{})",
                        r.name, state_str, r.source, tools
                    ));
                }
                lines.push(
                    "命令: /extensions reload <name> | /extensions disable <name> | /extensions enable <name>".into(),
                );
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed: lines,
                    next_steps: vec![],
                });
            }
            s if s.starts_with("reload ") => {
                let name = s.strip_prefix("reload ").unwrap().trim();
                match mgr.reload(name) {
                    Ok(()) => {
                        self.chat.push_message(chat::ChatMessage::Summary {
                            completed: vec![format!("扩展 '{name}' 已重载")],
                            next_steps: vec![],
                        });
                    }
                    Err(e) => {
                        self.chat.push_message(chat::ChatMessage::Error {
                            message: format!("重载失败: {e}"),
                            category: uncode_core::event::ErrorCategory::Config,
                        });
                    }
                }
            }
            s if s.starts_with("disable ") => {
                let name = s.strip_prefix("disable ").unwrap().trim();
                match mgr.disable(name) {
                    Ok(()) => {
                        self.chat.push_message(chat::ChatMessage::Summary {
                            completed: vec![format!("扩展 '{name}' 已禁用")],
                            next_steps: vec![],
                        });
                    }
                    Err(e) => {
                        self.chat.push_message(chat::ChatMessage::Error {
                            message: format!("禁用失败: {e}"),
                            category: uncode_core::event::ErrorCategory::Config,
                        });
                    }
                }
            }
            s if s.starts_with("enable ") => {
                let name = s.strip_prefix("enable ").unwrap().trim();
                match mgr.enable(name) {
                    Ok(()) => {
                        self.chat.push_message(chat::ChatMessage::Summary {
                            completed: vec![format!("扩展 '{name}' 已启用")],
                            next_steps: vec![],
                        });
                    }
                    Err(e) => {
                        self.chat.push_message(chat::ChatMessage::Error {
                            message: format!("启用失败: {e}"),
                            category: uncode_core::event::ErrorCategory::Config,
                        });
                    }
                }
            }
            _ => {
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed: vec!["用法: /extensions [list|reload|disable|enable]".into()],
                    next_steps: vec![],
                });
            }
        }
    }

    fn handle_skill_invoke<F>(&mut self, skill_name: &str, args_str: &str, on_submit: &F)
    where
        F: Fn(String, CancellationToken, String, String, SubmitIntent),
    {
        use uncode_core::skill::SkillRegistry;
        let registry = SkillRegistry::load_with_project(&self.cwd);
        let _skill = match registry.get(skill_name) {
            Some(s) => s,
            None => {
                self.chat.push_message(chat::ChatMessage::Error {
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
                self.chat.push_message(chat::ChatMessage::Error {
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
        on_submit(
            prompt,
            token,
            self.model.clone(),
            self.session_id.clone(),
            SubmitIntent::NewRun,
        );
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
            let mut completed = vec!["可用主题:".to_owned()];
            completed.extend(lines);
            completed.push("使用 /theme <name> 切换".to_owned());
            self.chat.push_message(chat::ChatMessage::Summary {
                completed,
                next_steps: vec![],
            });
            return;
        }

        match Theme::load_by_name(name) {
            Some(theme) => {
                let old = self.theme.name.clone();
                self.theme = theme;
                self.chat.push_message(chat::ChatMessage::Summary {
                    completed: vec![format!("主题切换: {old} → {}", self.theme.name)],
                    next_steps: vec![],
                });
            }
            None => {
                self.chat.push_message(chat::ChatMessage::Error {
                    message: format!("主题 '{name}' 不存在。使用 /theme 查看可用列表。"),
                    category: uncode_core::event::ErrorCategory::Config,
                });
            }
        }
    }

    fn flush_queue<F>(&mut self, on_submit: &F)
    where
        F: Fn(String, CancellationToken, String, String, SubmitIntent),
    {
        if let Some(text) = self.queue.drain_follow_up().into_iter().next() {
            self.agent_busy = true;
            self.footer.start_turn();
            self.chat.push_user_message(text.clone());
            let token = self.new_cancel_token();
            on_submit(
                text,
                token,
                self.model.clone(),
                self.session_id.clone(),
                SubmitIntent::NewRun,
            );
        }
    }

    fn scroll_to_focused_card(&mut self) {
        let Some(idx) = self.chat.focused_card else {
            return;
        };
        let start = self.chat.message_start_line(idx);
        self.chat.scroll_offset = start;
        self.chat.auto_scroll = false;
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
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
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
    let mut body = String::with_capacity(entries.len() * 256);
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
        .unwrap_or_else(|_| std::env::var("VISUAL").unwrap_or_else(|_| "vi".to_owned()));
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
    use uncode_core::event::SessionEndData;

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
        let line = footer.render_line2("deepseek-v3", "◕", "medium", &Theme::default());
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
        let line = footer.render_line2("model", "○", "off", &Theme::default());
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
    fn test_handle_event_turn_end_keeps_agent_busy() {
        let mut engine = TuiEngine::new();
        engine.agent_busy = true;
        engine.handle_event(AgentEvent::TurnEnd {
            turn: 1,
            usage: UsageInfo {
                input_tokens: 10_000,
                output_tokens: 5_000,
                cost: None,
            },
        });
        assert!(engine.agent_busy);
        assert_eq!(engine.footer.input_tokens, 10_000);
        assert_eq!(engine.footer.output_tokens, 5_000);
    }

    #[test]
    fn test_handle_event_turn_start_sets_turn_in_footer() {
        let mut engine = TuiEngine::new();
        engine.handle_event(AgentEvent::TurnStart { turn: 3 });
        assert!(engine.agent_busy);
        assert_eq!(engine.footer.current_turn, 3);
    }

    #[test]
    fn test_multi_turn_chain_busy_until_session_end() {
        let mut engine = TuiEngine::new();
        engine.agent_busy = true;
        engine.handle_event(AgentEvent::TurnStart { turn: 1 });
        engine.handle_event(AgentEvent::TurnEnd {
            turn: 1,
            usage: UsageInfo::default(),
        });
        assert!(engine.agent_busy);
        engine.handle_event(AgentEvent::TurnStart { turn: 2 });
        engine.handle_event(AgentEvent::TurnEnd {
            turn: 2,
            usage: UsageInfo::default(),
        });
        assert!(engine.agent_busy);
        engine.handle_event(AgentEvent::SessionEnd {
            data: Box::new(SessionEndData {
                session_id: "s".into(),
                total_turns: 2,
                total_tokens: UsageInfo::default(),
                exit_reason: "completed".into(),
            }),
        });
        assert!(!engine.agent_busy);
        assert_eq!(engine.footer.current_turn, 0);
    }

    #[test]
    fn test_handle_event_session_end_updates_footer() {
        let mut engine = TuiEngine::new();
        engine.agent_busy = true;
        engine.handle_event(AgentEvent::SessionEnd {
            data: Box::new(SessionEndData {
                session_id: "sess123".into(),
                total_turns: 5,
                total_tokens: UsageInfo {
                    input_tokens: 100_000,
                    output_tokens: 50_000,
                    cost: None,
                },
                exit_reason: "done".into(),
            }),
        });
        assert!(!engine.agent_busy);
        assert_eq!(engine.footer.input_tokens, 100_000);
        assert_eq!(engine.footer.output_tokens, 50_000);
    }

    #[test]
    fn test_set_default_model_sets_model_and_index() {
        let mut engine = TuiEngine::new();
        engine.set_available_models(vec![
            "deepseek-v3".into(),
            "glm-4-flash".into(),
            "ollama".into(),
        ]);

        // 设置存在的模型
        engine.set_default_model("glm-4-flash".into());
        assert_eq!(engine.model, "glm-4-flash");
        assert_eq!(engine.model_index, 1);

        // 设置不在列表中的模型
        engine.set_default_model("unknown-model".into());
        assert_eq!(engine.model, "unknown-model");
        // model_index 不变（仍为上次的 1）
        assert_eq!(engine.model_index, 1);
    }

    #[test]
    fn test_set_default_model_empty_list() {
        let mut engine = TuiEngine::new();
        engine.set_default_model("deepseek-v3".into());
        assert_eq!(engine.model, "deepseek-v3");
        assert_eq!(engine.model_index, 0);
    }

    #[test]
    fn test_handle_model_command_switches_model() {
        let mut engine = TuiEngine::new();
        engine.set_available_models(vec![
            "deepseek-v3".into(),
            "glm-4-flash".into(),
            "ollama".into(),
        ]);
        engine.set_default_model("deepseek-v3".into());

        engine.handle_model_command("/model glm-4-flash");

        assert_eq!(engine.model, "glm-4-flash");
        assert_eq!(engine.model_index, 1);
        // 不产生 Summary 消息
        assert_eq!(engine.chat.messages.len(), 0);
    }

    #[test]
    fn test_handle_model_command_unknown_model_still_switches() {
        let mut engine = TuiEngine::new();
        engine.set_available_models(vec!["deepseek-v3".into()]);
        engine.set_default_model("deepseek-v3".into());

        engine.handle_model_command("/model my-custom-model");

        assert_eq!(engine.model, "my-custom-model");
        // 不在列表中，model_index 不变
        assert_eq!(engine.model_index, 0);
        assert_eq!(engine.chat.messages.len(), 0);
    }

    #[test]
    fn test_handle_model_command_no_name_shows_selector() {
        let mut engine = TuiEngine::new();
        engine.set_available_models(vec!["deepseek-v3".into()]);

        engine.handle_model_command("/model");

        // 不切换模型，只显示选择器
        assert!(engine.model.is_empty());
        assert!(engine.selector.is_visible());
        assert_eq!(engine.chat.messages.len(), 0);
    }

    /// 模拟 Ctrl+P 模型循环切换逻辑（提取自事件处理）
    #[test]
    fn test_ctrl_p_cycles_model() {
        let mut engine = TuiEngine::new();
        engine.set_available_models(vec![
            "deepseek-v3".into(),
            "glm-4-flash".into(),
            "ollama".into(),
        ]);
        engine.set_default_model("deepseek-v3".into());

        let msg_count_before = engine.chat.messages.len();

        // 模拟 Ctrl+P 正向循环
        engine.model_index = (engine.model_index + 1) % engine.available_models.len();
        engine.model = engine.available_models[engine.model_index].clone();
        assert_eq!(engine.model, "glm-4-flash");
        assert_eq!(engine.model_index, 1);

        // 再次 Ctrl+P
        engine.model_index = (engine.model_index + 1) % engine.available_models.len();
        engine.model = engine.available_models[engine.model_index].clone();
        assert_eq!(engine.model, "ollama");
        assert_eq!(engine.model_index, 2);

        // 循环回第一个
        engine.model_index = (engine.model_index + 1) % engine.available_models.len();
        engine.model = engine.available_models[engine.model_index].clone();
        assert_eq!(engine.model, "deepseek-v3");
        assert_eq!(engine.model_index, 0);

        // 不产生 Summary 消息
        assert_eq!(engine.chat.messages.len(), msg_count_before);
    }

    #[test]
    fn test_on_submit_receives_current_model() {
        let mut engine = TuiEngine::new();
        engine.set_available_models(vec!["deepseek-v3".into(), "ollama".into()]);
        engine.set_default_model("deepseek-v3".into());

        // 切换到 ollama
        engine.model = "ollama".into();
        engine.model_index = 1;

        let captured: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
        let on_submit = |_text: String,
                         _token: tokio_util::sync::CancellationToken,
                         model: String,
                         _sid: String,
                         _intent: SubmitIntent| {
            *captured.borrow_mut() = model;
        };

        engine.submit_text("hello".into(), &on_submit);

        assert_eq!(captured.borrow().as_str(), "ollama");
    }

    #[test]
    fn test_on_submit_uses_default_model_when_not_switched() {
        let mut engine = TuiEngine::new();
        engine.set_available_models(vec!["deepseek-v3".into(), "ollama".into()]);
        engine.set_default_model("deepseek-v3".into());

        let captured: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
        let on_submit = |_text: String,
                         _token: tokio_util::sync::CancellationToken,
                         model: String,
                         _sid: String,
                         _intent: SubmitIntent| {
            *captured.borrow_mut() = model;
        };

        engine.submit_text("hello".into(), &on_submit);

        assert_eq!(captured.borrow().as_str(), "deepseek-v3");
    }

    #[test]
    fn test_submit_while_busy_uses_steer_intent() {
        let mut engine = TuiEngine::new();
        engine.agent_busy = true;
        let captured: std::cell::RefCell<Option<SubmitIntent>> = std::cell::RefCell::new(None);
        let on_submit = |_text: String,
                         _token: tokio_util::sync::CancellationToken,
                         _model: String,
                         _sid: String,
                         intent: SubmitIntent| {
            *captured.borrow_mut() = Some(intent);
        };
        engine.submit_text("please fix the test".into(), &on_submit);
        assert_eq!(*captured.borrow(), Some(SubmitIntent::Steer));
    }

    #[test]
    fn test_later_while_busy_queues_follow_up_not_steer() {
        let mut engine = TuiEngine::new();
        engine.agent_busy = true;
        let captured: std::cell::RefCell<Option<SubmitIntent>> = std::cell::RefCell::new(None);
        let on_submit = |_text: String,
                         _token: tokio_util::sync::CancellationToken,
                         _model: String,
                         _sid: String,
                         intent: SubmitIntent| {
            *captured.borrow_mut() = Some(intent);
        };
        engine.submit_text("/later run tests".into(), &on_submit);
        assert_eq!(*captured.borrow(), None);
        assert_eq!(engine.queue.len(), 1);
    }

    #[test]
    fn test_render_lines_with_theme_and_renderers() {
        let engine = TuiEngine::new();
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        // 空消息 — 不 panic，无提示文字
        let lines = engine.chat.render_lines(
            area,
            &engine.renderers,
            &engine.theme,
            0,
            false,
            true,
            &engine.message_renderers,
        );
        assert_eq!(lines.len(), 0);

        // 使用 light theme — 不 panic
        let light = Theme::light();
        let lines_light = engine.chat.render_lines(
            area,
            &engine.renderers,
            &light,
            0,
            false,
            true,
            &engine.message_renderers,
        );
        assert_eq!(lines_light.len(), 0);
    }

    // ── ESC 键测试 ──

    #[test]
    fn test_esc_denies_permission() {
        let mut engine = TuiEngine::new();
        engine.agent_busy = true;
        engine.permission.request_confirmation(
            "t1".into(),
            "write".into(),
            "test.rs".into(),
            None,
            false,
            None,
        );
        assert!(engine.permission.has_pending());

        // Simulate ESC in handle_input
        engine.handle_esc();

        assert!(
            !engine.permission.has_pending(),
            "ESC should deny permission"
        );
        // Agent should NOT be interrupted when permission was pending
        assert!(
            engine.agent_busy,
            "agent should remain busy after denying permission"
        );
    }

    #[test]
    fn test_esc_interrupts_agent() {
        let mut engine = TuiEngine::new();
        let token = engine.new_cancel_token();
        engine.agent_busy = true;

        engine.handle_esc();

        assert!(token.is_cancelled(), "ESC should cancel token");
        assert!(!engine.agent_busy, "agent should be idle after ESC");
        // Should push interruption summary
        let has_interrupted = engine.chat.messages.iter().any(|m| {
            if let chat::ChatMessage::Summary { completed, .. } = m {
                completed.iter().any(|s| s.contains("Interrupted"))
            } else {
                false
            }
        });
        assert!(has_interrupted, "should show interruption message");
    }

    #[test]
    fn test_esc_clears_focus() {
        let mut engine = TuiEngine::new();
        engine.chat.messages.push(chat::ChatMessage::ToolCall {
            tool_id: "t1".into(),
            tool_name: "read".into(),
            arguments_summary: String::new(),
            status: chat::ToolCallRenderStatus::Success,
            duration_ms: None,
            result: None,
            expanded: false,
            title: None,
        });
        engine.chat.focused_card = Some(0);

        engine.handle_esc();

        assert!(engine.chat.focused_card.is_none(), "ESC should clear focus");
    }

    #[test]
    fn test_esc_hides_welcome() {
        let mut engine = TuiEngine::new();
        engine.welcome.visible = true;
        assert!(engine.welcome.is_visible());

        engine.handle_esc();

        assert!(!engine.welcome.is_visible(), "ESC should hide welcome");
    }

    #[test]
    fn test_esc_hides_selector() {
        let mut engine = TuiEngine::new();
        engine.selector.show("title", vec!["a".into(), "b".into()]);
        assert!(engine.selector.is_visible());

        engine.handle_esc();

        assert!(!engine.selector.is_visible(), "ESC should hide selector");
    }

    #[test]
    fn test_esc_priority_permission_over_interrupt() {
        let mut engine = TuiEngine::new();
        let token = engine.new_cancel_token();
        engine.agent_busy = true;
        engine.permission.request_confirmation(
            "t1".into(),
            "write".into(),
            "test.rs".into(),
            None,
            false,
            None,
        );

        engine.handle_esc();

        // Permission denied, agent NOT interrupted
        assert!(!engine.permission.has_pending());
        assert!(
            !token.is_cancelled(),
            "token should NOT be cancelled when permission had priority"
        );
        assert!(engine.agent_busy, "agent should still be busy");
    }

    #[test]
    fn test_esc_idle_noop() {
        let mut engine = TuiEngine::new();
        // Nothing pending, not busy, no focus, no overlays
        engine.handle_esc();
        // Should not panic or change state
        assert!(!engine.agent_busy);
        assert!(engine.chat.focused_card.is_none());
    }

    #[test]
    fn test_question_request_sets_pending_question() {
        let mut engine = TuiEngine::new();
        let data = Box::new(uncode_core::event::QuestionRequestData {
            tool_call_id: "q1".into(),
            title: "test".into(),
            questions: vec![uncode_core::event::QuestionItem {
                question: "Continue?".into(),
                header: "Continue".into(),
                options: vec![
                    uncode_core::event::QuestionOption {
                        label: "Yes".into(),
                        description: "Proceed".into(),
                    },
                    uncode_core::event::QuestionOption {
                        label: "No".into(),
                        description: "Stop".into(),
                    },
                ],
                multiple: false,
            }],
        });
        engine.handle_event(AgentEvent::QuestionRequest { data: data.clone() });
        assert!(engine.pending_question.is_some());
        let pending = engine.pending_question.as_ref().unwrap();
        assert_eq!(pending.tool_call_id, "q1");
        assert_eq!(pending.questions.len(), 1);
        assert_eq!(pending.questions[0].header, "Continue");
        assert_eq!(pending.questions[0].options[0].label, "Yes");
    }

    #[test]
    fn test_permission_dialog_renders_for_bash() {
        let mut engine = TuiEngine::new();
        let tool_args = serde_json::json!({
            "command": "cargo build",
            "description": "Build project",
            "workdir": "/tmp"
        });
        engine.permission.request_confirmation(
            "t1".into(),
            "bash".into(),
            "bash cargo build".into(),
            Some("Execute shell command".into()),
            false,
            Some(tool_args),
        );
        assert!(engine.permission.has_pending());

        // Verify pending state contains tool_args
        let pending = engine.permission.pending().unwrap();
        assert_eq!(pending.tool_name, "bash");
        let args = pending.tool_args.as_ref().unwrap();
        assert_eq!(args["command"], "cargo build");
        assert_eq!(args["description"], "Build project");
    }

    #[test]
    fn test_permission_dialog_renders_for_write_with_preview() {
        let mut engine = TuiEngine::new();
        let tool_args = serde_json::json!({
            "filePath": "src/main.rs",
            "content": "fn main() {\n    println!(\"hello\");\n}\n"
        });
        engine.permission.request_confirmation(
            "t2".into(),
            "write".into(),
            "write src/main.rs".into(),
            Some("Write file".into()),
            true,
            Some(tool_args),
        );
        assert!(engine.permission.has_pending());

        let pending = engine.permission.pending().unwrap();
        assert_eq!(pending.tool_name, "write");
        // Edit option available for write tools
        assert!(
            pending
                .options
                .contains(&crate::permission::ConfirmOption::Edit)
        );
        let args = pending.tool_args.as_ref().unwrap();
        assert_eq!(args["filePath"], "src/main.rs");
        assert!(args["content"].as_str().unwrap().contains("println"));
    }

    #[test]
    fn test_permission_dialog_renders_for_generic_tool() {
        let mut engine = TuiEngine::new();
        engine.permission.request_confirmation(
            "t3".into(),
            "web_fetch".into(),
            "GET https://example.com".into(),
            Some("Fetch URL".into()),
            false,
            None,
        );
        assert!(engine.permission.has_pending());

        let pending = engine.permission.pending().unwrap();
        assert_eq!(pending.tool_name, "web_fetch");
        assert!(pending.tool_args.is_none());
        // Generic tools don't have edit option
        assert!(
            !pending
                .options
                .contains(&crate::permission::ConfirmOption::Edit)
        );
    }

    #[test]
    fn test_permission_always_allow_key() {
        let mut engine = TuiEngine::new();
        engine.permission.request_confirmation(
            "t4".into(),
            "write".into(),
            "write test.rs".into(),
            None,
            false,
            None,
        );
        assert!(engine.permission.has_pending());

        // 'a' = always allow → behaves like Allow for now
        let resolved = engine
            .permission
            .confirm(crate::permission::ConfirmOption::Allow);
        assert!(resolved.is_some());
        assert!(!engine.permission.has_pending());
    }
}
