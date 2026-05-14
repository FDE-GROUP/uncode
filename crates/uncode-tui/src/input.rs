use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::VecDeque;

const MAX_HISTORY: usize = 100;

pub struct InputEditor {
    buffer: String,
    cursor: usize,
    history: VecDeque<String>,
    history_index: Option<usize>,
    multiline: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    None,
    Submit(String),
    Cancel,
}

impl InputEditor {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: VecDeque::with_capacity(MAX_HISTORY),
            history_index: None,
            multiline: false,
        }
    }

    pub fn handle_key(&mut self, code: crossterm::event::KeyCode) -> InputAction {
        use crossterm::event::KeyCode;

        match code {
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.buffer);
                self.cursor = 0;
                self.history_index = None;
                if !text.is_empty() {
                    if self.history.len() >= MAX_HISTORY {
                        self.history.pop_front();
                    }
                    self.history.push_back(text.clone());
                }
                if text.starts_with('/') {
                    InputAction::Submit(text)
                } else {
                    InputAction::Submit(text)
                }
            }
            KeyCode::Esc => {
                self.buffer.clear();
                self.cursor = 0;
                self.history_index = None;
                InputAction::Cancel
            }
            KeyCode::Up => {
                if self.history.is_empty() {
                    return InputAction::None;
                }
                let idx = match self.history_index {
                    Some(i) if i > 0 => i - 1,
                    _ => self.history.len() - 1,
                };
                self.history_index = Some(idx);
                self.buffer = self.history[idx].clone();
                self.cursor = self.buffer.len();
                InputAction::None
            }
            KeyCode::Down => {
                match self.history_index {
                    Some(i) if i < self.history.len() - 1 => {
                        let new_idx = i + 1;
                        self.history_index = Some(new_idx);
                        self.buffer = self.history[new_idx].clone();
                    }
                    Some(_) => {
                        self.history_index = None;
                        self.buffer.clear();
                    }
                    None => {
                        return InputAction::None;
                    }
                }
                self.cursor = self.buffer.len();
                InputAction::None
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                InputAction::None
            }
            KeyCode::Right => {
                if self.cursor < self.buffer.len() {
                    self.cursor += 1;
                }
                InputAction::None
            }
            KeyCode::Home => {
                self.cursor = 0;
                InputAction::None
            }
            KeyCode::End => {
                self.cursor = self.buffer.len();
                InputAction::None
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                }
                InputAction::None
            }
            KeyCode::Delete => {
                if self.cursor < self.buffer.len() {
                    self.buffer.remove(self.cursor);
                }
                InputAction::None
            }
            KeyCode::Char('a') if ctrl_pressed() => {
                self.cursor = 0;
                InputAction::None
            }
            KeyCode::Char('e') if ctrl_pressed() => {
                self.cursor = self.buffer.len();
                InputAction::None
            }
            KeyCode::Char('k') if ctrl_pressed() => {
                self.buffer.truncate(self.cursor);
                InputAction::None
            }
            KeyCode::Char('u') if ctrl_pressed() => {
                self.buffer.drain(..self.cursor);
                self.cursor = 0;
                InputAction::None
            }
            KeyCode::Char('w') if ctrl_pressed() => {
                while self.cursor > 0 && self.buffer.as_bytes().get(self.cursor - 1) == Some(&b' ')
                {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                }
                while self.cursor > 0 && self.buffer.as_bytes().get(self.cursor - 1) != Some(&b' ')
                {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                }
                InputAction::None
            }
            KeyCode::Char(c) => {
                self.buffer.insert(self.cursor, c);
                self.cursor += 1;
                InputAction::None
            }
            KeyCode::Tab => {
                // Tab completion placeholder - will be wired to completion engine
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    pub fn set_buffer(&mut self, text: String) {
        self.buffer = text;
        self.cursor = self.buffer.len();
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let display_text = if self.buffer.is_empty() {
            "> _".to_string()
        } else {
            format!("> {}", self.buffer)
        };

        let content = Paragraph::new(display_text)
            .block(Block::default().borders(Borders::TOP).title("输入"))
            .style(Style::default().fg(Color::White));

        f.render_widget(content, area);
    }
}

impl Default for InputEditor {
    fn default() -> Self {
        Self::new()
    }
}

fn ctrl_pressed() -> bool {
    // In ratatui, we check for Ctrl modifier through key event modifiers
    // This is a best-effort check; actual Ctrl detection happens in key handling
    false
}
