//! Dialog overlay — 扩展发起的交互对话框（Select / Confirm / Input）。

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};

use uncode_core::dialog::{DialogRequest, DialogResponse};

use crate::theme::Theme;

/// Dialog overlay state machine.
pub struct DialogOverlay {
    visible: bool,
    request: Option<DialogRequest>,
    /// For Select: list navigation state.
    list_state: ListState,
    /// For Input: current text buffer.
    input_buffer: String,
    /// Theme reference for rendering.
    theme: Theme,
}

impl DialogOverlay {
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            request: None,
            list_state: ListState::default(),
            input_buffer: String::new(),
            theme,
        }
    }

    pub fn show(&mut self, request: DialogRequest) {
        self.visible = true;
        self.request = Some(request);
        self.input_buffer.clear();
        // Initialize list state for Select.
        if let Some(DialogRequest::Select { .. }) = &self.request {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
        // Seed Input default.
        if let Some(DialogRequest::Input { default, .. }) = &self.request {
            if let Some(d) = default {
                self.input_buffer = d.clone();
            }
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.request = None;
        self.input_buffer.clear();
        self.list_state.select(None);
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Handle a key event. Returns `Some(response)` when the dialog is completed.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<DialogResponse> {
        use crossterm::event::{KeyCode, KeyModifiers};

        let request = self.request.as_ref()?;

        match request {
            DialogRequest::Select { options, .. } => match key.code {
                KeyCode::Up => {
                    let current = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some(current.saturating_sub(1)));
                    None
                }
                KeyCode::Down => {
                    let current = self.list_state.selected().unwrap_or(0);
                    self.list_state
                        .select(Some((current + 1).min(options.len() - 1)));
                    None
                }
                KeyCode::Enter => {
                    let idx = self.list_state.selected().unwrap_or(0);
                    let response = DialogResponse::Selected(idx);
                    self.hide();
                    Some(response)
                }
                KeyCode::Esc => {
                    self.hide();
                    Some(DialogResponse::Cancelled)
                }
                _ => None,
            },
            DialogRequest::Confirm { .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    self.hide();
                    Some(DialogResponse::Confirmed(true))
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.hide();
                    Some(DialogResponse::Confirmed(false))
                }
                _ => None,
            },
            DialogRequest::Input { .. } => match key.code {
                KeyCode::Enter => {
                    let text = std::mem::take(&mut self.input_buffer);
                    self.hide();
                    Some(DialogResponse::Input(text))
                }
                KeyCode::Esc => {
                    self.hide();
                    Some(DialogResponse::Cancelled)
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                    None
                }
                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        None
                    } else {
                        self.input_buffer.push(c);
                        None
                    }
                }
                _ => None,
            },
        }
    }

    /// Render the dialog overlay.
    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }
        let popup = centered_rect(50, 40, area);
        f.render_widget(Clear, popup);

        // Clone request data to avoid borrow conflict with &mut self.
        let request = match self.request.clone() {
            Some(r) => r,
            None => return,
        };

        match &request {
            DialogRequest::Select { title, options } => {
                self.render_select(f, popup, title, options);
            }
            DialogRequest::Confirm { message } => {
                self.render_confirm(f, popup, message);
            }
            DialogRequest::Input { prompt, .. } => {
                self.render_input(f, popup, prompt);
            }
        }
    }

    fn render_select(&mut self, f: &mut Frame, area: Rect, title: &str, options: &[String]) {
        let items: Vec<ListItem> = options.iter().map(|o| ListItem::new(o.as_str())).collect();
        let list = List::new(items)
            .block(
                Block::bordered()
                    .title(format!(" {title} "))
                    .title_style(Style::default().fg(self.theme.ui.user_message).bold())
                    .border_style(Style::default().fg(self.theme.ui.input_border)),
            )
            .highlight_style(Style::default().fg(self.theme.tool_status.success).bold());
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_confirm(&self, f: &mut Frame, area: Rect, message: &str) {
        let text = vec![
            Line::from(Span::styled(
                message.to_string(),
                Style::default().fg(self.theme.ui.agent_text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "y = Yes  /  n = No  /  Esc = Cancel",
                Style::default().fg(self.theme.ui.footer_text),
            )),
        ];
        let paragraph = Paragraph::new(text).block(
            Block::bordered()
                .title(" Confirm ")
                .title_style(Style::default().fg(self.theme.ui.user_message).bold())
                .border_style(Style::default().fg(self.theme.ui.input_border)),
        );
        f.render_widget(paragraph, area);
    }

    fn render_input(&self, f: &mut Frame, area: Rect, prompt: &str) {
        let text = vec![
            Line::from(Span::styled(
                prompt.to_string(),
                Style::default().fg(self.theme.ui.agent_text),
            )),
            Line::from(""),
            Line::from(vec![
                "> ".fg(self.theme.ui.user_message),
                Span::styled(
                    self.input_buffer.clone(),
                    Style::default().fg(self.theme.ui.agent_text),
                ),
                Span::styled(
                    "│",
                    Style::default().fg(self.theme.ui.user_message).slow_blink(),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Enter = Confirm  /  Esc = Cancel",
                Style::default().fg(self.theme.ui.footer_text),
            )),
        ];
        let paragraph = Paragraph::new(text).block(
            Block::bordered()
                .title(" Input ")
                .title_style(Style::default().fg(self.theme.ui.user_message).bold())
                .border_style(Style::default().fg(self.theme.ui.input_border)),
        );
        f.render_widget(paragraph, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let pw = r.width * percent_x / 100;
    let ph = r.height * percent_y / 100;
    let x = r.x + (r.width - pw) / 2;
    let y = r.y + (r.height - ph) / 2;
    Rect::new(x, y, pw, ph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn test_overlay() -> DialogOverlay {
        DialogOverlay::new(Theme::default())
    }

    #[test]
    fn test_show_hide() {
        let mut o = test_overlay();
        assert!(!o.is_visible());
        o.show(DialogRequest::Confirm {
            message: "test".into(),
        });
        assert!(o.is_visible());
        o.hide();
        assert!(!o.is_visible());
    }

    #[test]
    fn test_confirm_yes() {
        let mut o = test_overlay();
        o.show(DialogRequest::Confirm {
            message: "ok?".into(),
        });
        let resp = o.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Confirmed(true)));
        assert!(!o.is_visible());
    }

    #[test]
    fn test_confirm_no() {
        let mut o = test_overlay();
        o.show(DialogRequest::Confirm {
            message: "ok?".into(),
        });
        let resp = o.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Confirmed(false)));
    }

    #[test]
    fn test_confirm_esc() {
        let mut o = test_overlay();
        o.show(DialogRequest::Confirm {
            message: "ok?".into(),
        });
        let resp = o.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Confirmed(false)));
    }

    #[test]
    fn test_select_enter() {
        let mut o = test_overlay();
        o.show(DialogRequest::Select {
            title: "Pick".into(),
            options: vec!["a".into(), "b".into(), "c".into()],
        });
        let resp = o.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Selected(0)));
    }

    #[test]
    fn test_select_down_then_enter() {
        let mut o = test_overlay();
        o.show(DialogRequest::Select {
            title: "Pick".into(),
            options: vec!["a".into(), "b".into(), "c".into()],
        });
        assert_eq!(
            o.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            None
        );
        let resp = o.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Selected(1)));
    }

    #[test]
    fn test_select_esc() {
        let mut o = test_overlay();
        o.show(DialogRequest::Select {
            title: "Pick".into(),
            options: vec!["a".into()],
        });
        let resp = o.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Cancelled));
    }

    #[test]
    fn test_input_type_and_enter() {
        let mut o = test_overlay();
        o.show(DialogRequest::Input {
            prompt: "Name".into(),
            default: None,
        });
        assert_eq!(
            o.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            o.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            None
        );
        let resp = o.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Input("hi".into())));
    }

    #[test]
    fn test_input_backspace() {
        let mut o = test_overlay();
        o.show(DialogRequest::Input {
            prompt: "Name".into(),
            default: Some("abc".into()),
        });
        assert_eq!(o.input_buffer, "abc");
        assert_eq!(
            o.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            None
        );
        assert_eq!(o.input_buffer, "ab");
        let resp = o.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Input("ab".into())));
    }

    #[test]
    fn test_input_esc() {
        let mut o = test_overlay();
        o.show(DialogRequest::Input {
            prompt: "Name".into(),
            default: None,
        });
        let resp = o.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(resp, Some(DialogResponse::Cancelled));
    }
}
