use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::VecDeque;

const MAX_HISTORY: usize = 100;
const MAX_UNDO: usize = 50;

struct UndoSnapshot {
    buffer: String,
    cursor: usize,
}

pub struct InputEditor {
    buffer: String,
    cursor: usize,
    history: VecDeque<String>,
    history_index: Option<usize>,
    completions: Vec<String>,
    completion_index: usize,
    undo_stack: Vec<UndoSnapshot>,
    redo_stack: Vec<UndoSnapshot>,
    last_input_char: bool,
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
            undo_stack: Vec::with_capacity(MAX_UNDO),
            redo_stack: Vec::new(),
            last_input_char: false,
        }
    }

    fn push_undo(&mut self) {
        if self.undo_stack.len() >= MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(UndoSnapshot {
            buffer: self.buffer.clone(),
            cursor: self.cursor,
        });
        self.redo_stack.clear();
    }

    fn word_boundary_forward(&self) -> usize {
        let chars: Vec<char> = self.buffer[self.cursor..].chars().collect();
        let mut i = 0;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        while i < chars.len() && chars[i] != ' ' {
            i += 1;
        }
        let byte_offset: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
        self.cursor + byte_offset
    }

    fn word_boundary_backward(&self) -> usize {
        let before = &self.buffer[..self.cursor];
        let chars: Vec<char> = before.chars().collect();
        let mut i = chars.len();
        while i > 0 && chars[i - 1] == ' ' {
            i -= 1;
        }
        while i > 0 && chars[i - 1] != ' ' {
            i -= 1;
        }
        let byte_offset: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
        byte_offset
    }

    fn delete_word_forward(&mut self) {
        let end = self.word_boundary_forward();
        if end > self.cursor {
            self.buffer.drain(self.cursor..end);
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> InputAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            // Shift+Enter: insert newline (multiline)
            KeyCode::Enter if shift => {
                self.push_undo();
                self.buffer.insert(self.cursor, '\n');
                self.cursor += 1;
                self.last_input_char = true;
                InputAction::None
            }
            // Enter: submit
            KeyCode::Enter => {
                self.last_input_char = false;
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
                self.last_input_char = false;
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
                self.last_input_char = false;
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
            // Word navigation: Alt+Left / Alt+Right (before plain Left/Right)
            KeyCode::Left if alt => {
                self.last_input_char = false;
                self.cursor = self.word_boundary_backward();
                InputAction::None
            }
            KeyCode::Right if alt => {
                self.last_input_char = false;
                self.cursor = self.word_boundary_forward();
                InputAction::None
            }
            KeyCode::Left => {
                self.last_input_char = false;
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                InputAction::None
            }
            KeyCode::Right => {
                self.last_input_char = false;
                if self.cursor < self.buffer.len() {
                    self.cursor += 1;
                }
                InputAction::None
            }
            KeyCode::Home => {
                self.last_input_char = false;
                self.cursor = 0;
                InputAction::None
            }
            KeyCode::End => {
                self.last_input_char = false;
                self.cursor = self.buffer.len();
                InputAction::None
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.push_undo();
                    let prev = self.prev_char_boundary();
                    self.buffer.drain(prev..self.cursor);
                    self.cursor = prev;
                }
                InputAction::None
            }
            KeyCode::Delete => {
                if self.cursor < self.buffer.len() {
                    self.push_undo();
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
                self.push_undo();
                self.buffer.truncate(self.cursor);
                InputAction::None
            }
            KeyCode::Char('u') if ctrl => {
                self.push_undo();
                self.buffer.drain(..self.cursor);
                self.cursor = 0;
                InputAction::None
            }
            KeyCode::Char('w') if ctrl => {
                self.push_undo();
                self.delete_word_backward();
                InputAction::None
            }
            // Undo: Ctrl+Z
            KeyCode::Char('z') if ctrl => {
                self.last_input_char = false;
                if let Some(snapshot) = self.undo_stack.pop() {
                    self.redo_stack.push(UndoSnapshot {
                        buffer: std::mem::take(&mut self.buffer),
                        cursor: self.cursor,
                    });
                    self.buffer = snapshot.buffer;
                    self.cursor = snapshot.cursor;
                }
                InputAction::None
            }
            // Redo: Ctrl+Y (but not Alt+Y for yank-pop)
            KeyCode::Char('y') if ctrl && !alt => {
                self.last_input_char = false;
                if let Some(snapshot) = self.redo_stack.pop() {
                    self.undo_stack.push(UndoSnapshot {
                        buffer: std::mem::take(&mut self.buffer),
                        cursor: self.cursor,
                    });
                    self.buffer = snapshot.buffer;
                    self.cursor = snapshot.cursor;
                }
                InputAction::None
            }
            // Delete word forward: Alt+D
            KeyCode::Char('d') if alt => {
                self.push_undo();
                self.delete_word_forward();
                InputAction::None
            }
            KeyCode::Char(c) if !ctrl && !alt => {
                if !self.last_input_char {
                    self.push_undo();
                }
                self.buffer.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                self.last_input_char = true;
                InputAction::None
            }
            KeyCode::Tab => {
                self.last_input_char = false;
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

    pub fn render(&self, f: &mut Frame, area: Rect, border_color: Color) {
        let display_text = if self.buffer.is_empty() {
            "> _".to_string()
        } else {
            format!("> {}", self.buffer)
        };

        let content = Paragraph::new(display_text)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .title("输入")
                    .border_style(Style::default().fg(border_color)),
            )
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

    fn alt_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }

    fn shift_enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)
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

    #[test]
    fn test_shift_enter_multiline() {
        let mut editor = InputEditor::new();
        editor.handle_key(key(KeyCode::Char('a')));
        editor.handle_key(shift_enter());
        editor.handle_key(key(KeyCode::Char('b')));
        assert_eq!(editor.buffer(), "a\nb");
    }

    #[test]
    fn test_undo_redo() {
        let mut editor = InputEditor::new();
        editor.handle_key(key(KeyCode::Char('a')));
        editor.handle_key(key(KeyCode::Char('b')));
        editor.handle_key(key(KeyCode::Char('c')));
        // abc → undo should go back to empty (word-merged)
        editor.handle_key(ctrl_key(KeyCode::Char('z')));
        assert_eq!(editor.buffer(), "");
        // redo
        editor.handle_key(ctrl_key(KeyCode::Char('y')));
        assert_eq!(editor.buffer(), "abc");
    }

    #[test]
    fn test_word_navigation() {
        let mut editor = InputEditor::new();
        editor.set_buffer("hello world test".into());
        // cursor at end (16)
        assert_eq!(editor.cursor, 16);
        // Alt+Left: jump back to "test" start
        editor.handle_key(alt_key(KeyCode::Left));
        assert_eq!(editor.cursor, 12);
        // Alt+Left: jump back to "world" start
        editor.handle_key(alt_key(KeyCode::Left));
        assert_eq!(editor.cursor, 6);
    }

    #[test]
    fn test_alt_d_delete_word_forward() {
        let mut editor = InputEditor::new();
        editor.set_buffer("hello world".into());
        // Move cursor to after "hello"
        editor.cursor = 5;
        editor.handle_key(alt_key(KeyCode::Char('d')));
        assert_eq!(editor.buffer(), "hello");
    }
}
