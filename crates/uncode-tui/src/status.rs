//! Status manager — manages extension status text displayed in the footer.

use std::collections::HashMap;

use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub struct StatusManager {
    statuses: HashMap<String, String>,
}

impl StatusManager {
    pub fn new() -> Self {
        Self {
            statuses: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: String, text: String) {
        self.statuses.insert(key, text);
    }

    pub fn clear(&mut self, key: &str) {
        self.statuses.remove(key);
    }

    pub fn render_spans(&self) -> Vec<Span<'static>> {
        if self.statuses.is_empty() {
            return vec![];
        }
        let mut spans = vec![Span::styled(" ", Style::default())];
        for (i, (_key, text)) in self.statuses.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::styled(
                text.clone(),
                Style::default().fg(Color::Yellow),
            ));
        }
        spans
    }
}

impl Default for StatusManager {
    fn default() -> Self {
        Self::new()
    }
}
