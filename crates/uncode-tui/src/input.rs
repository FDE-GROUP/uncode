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
            completions: Vec::new(),
            completion_index: 0,
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> InputAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
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
                    None => return InputAction::None,
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
                    let prev = self.prev_char_boundary();
                    self.buffer.drain(prev..self.cursor);
                    self.cursor = prev;
                }
                InputAction::None
            }
            KeyCode::Delete => {
                if self.cursor < self.buffer.len() {
                    let next = self.next_char_boundary();
                    self.buffer.drain(self.cursor..next);
                }
                InputAction::None
            }
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
                InputAction::None
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = self.buffer.len();
                InputAction::None
            }
            KeyCode::Char('k') if ctrl => {
                self.buffer.truncate(self.cursor);
                InputAction::None
            }
            KeyCode::Char('u') if ctrl => {
                self.buffer.drain(..self.cursor);
                self.cursor = 0;
                InputAction::None
            }
            KeyCode::Char('w') if ctrl => {
                self.delete_word_backward();
                InputAction::None
            }
            KeyCode::Char(c) => {
                self.buffer.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                InputAction::None
            }
            KeyCode::Tab => {
                if !self.completions.is_empty() {
                    let idx = self.completion_index % self.completions.len();
                    self.completion_index = (idx + 1) % self.completions.len();
                    let selected = &self.completions[idx];
                    if let Some(pos) = self.buffer.rfind(' ') {
                        self.buffer.truncate(pos + 1);
                        self.buffer.push_str(selected);
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

    fn delete_word_backward(&mut self) {
        while self.cursor > 0 && self.last_char() == Some(' ') {
            let prev = self.prev_char_boundary();
            self.buffer.drain(prev..self.cursor);
            self.cursor = prev;
        }
        while self.cursor > 0 && self.last_char() != Some(' ') {
            let prev = self.prev_char_boundary();
            self.buffer.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    fn last_char(&self) -> Option<char> {
        if self.cursor == 0 {
            return None;
        }
        self.buffer[..self.cursor].chars().last()
    }

    fn prev_char_boundary(&self) -> usize {
        let mut idx = self.cursor.saturating_sub(1);
        while idx > 0 && !self.buffer.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    fn next_char_boundary(&self) -> usize {
        let mut idx = self.cursor + 1;
        while idx < self.buffer.len() && !self.buffer.is_char_boundary(idx) {
            idx += 1;
        }
        idx
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn test_utf8_insert() {
        let mut editor = InputEditor::new();
        editor.handle_key(key(KeyCode::Char('你')));
        editor.handle_key(key(KeyCode::Char('好')));
        assert_eq!(editor.buffer(), "你好");
    }

    #[test]
    fn test_utf8_backspace() {
        let mut editor = InputEditor::new();
        editor.set_buffer("你好".into());
        editor.handle_key(key(KeyCode::Backspace));
        assert_eq!(editor.buffer(), "你");
    }

    #[test]
    fn test_delete_word_cjk() {
        let mut editor = InputEditor::new();
        editor.set_buffer("你好 世界".into());
        editor.handle_key(ctrl_key(KeyCode::Char('w')));
        assert_eq!(editor.buffer(), "你好 ");
    }

    #[test]
    fn test_bare_w_inserts_char() {
        let mut editor = InputEditor::new();
        editor.handle_key(key(KeyCode::Char('w')));
        assert_eq!(editor.buffer(), "w");
    }

    #[test]
    fn test_submit_and_history() {
        let mut editor = InputEditor::new();
        editor.handle_key(key(KeyCode::Char('h')));
        editor.handle_key(key(KeyCode::Char('i')));
        let action = editor.handle_key(key(KeyCode::Enter));
        assert_eq!(action, InputAction::Submit("hi".into()));
    }
}
