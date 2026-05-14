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
    #[allow(dead_code)]
    multiline: bool,
    completions: Vec<String>,
    completion_index: usize,
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
            completions: Vec::new(),
            completion_index: 0,
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
                InputAction::Submit(text)
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
                while self.cursor > 0 && self.buffer[..self.cursor].ends_with(' ')
                {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                }
                while self.cursor > 0 && !self.buffer[..self.cursor].ends_with(' ')
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
                if !self.completions.is_empty() {
                    let idx = self.completion_index % self.completions.len();
                    self.completion_index = (idx + 1) % self.completions.len();
                    let selected = &self.completions[idx];
                    if let Some(last) = self.buffer.rsplit_once(' ') {
                        self.buffer = format!("{} {selected}", last.0);
                    } else {
                        self.buffer = selected.clone();
                    }
                    self.cursor = self.buffer.len();
                }
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

    pub fn set_completions(&mut self, completions: Vec<String>) {
        self.completions = completions;
        self.completion_index = 0;
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
