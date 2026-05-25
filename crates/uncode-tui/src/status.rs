//! Status manager — manages extension status text displayed in the footer.

use std::collections::HashMap;

use ratatui::style::{Style, Stylize};
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
                spans.push(" | ".dark_gray());
            }
            spans.push(Span::styled(text.clone(), Style::default().yellow()));
        }
        spans
    }
}

impl Default for StatusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let m = StatusManager::new();
        assert!(m.render_spans().is_empty());
    }

    #[test]
    fn test_set_and_render() {
        let mut m = StatusManager::new();
        m.set("k1".into(), "hello".into());
        let spans = m.render_spans();
        assert!(!spans.is_empty());
        // First span is a space separator
        assert!(spans.len() >= 2);
        assert_eq!(spans[1].content, "hello");
    }

    #[test]
    fn test_clear() {
        let mut m = StatusManager::new();
        m.set("k1".into(), "hello".into());
        m.clear("k1");
        assert!(m.render_spans().is_empty());
    }

    #[test]
    fn test_multiple_statuses() {
        let mut m = StatusManager::new();
        m.set("k1".into(), "first".into());
        m.set("k2".into(), "second".into());
        let spans = m.render_spans();
        // Each status adds at least one span; with separator " | " between them
        assert!(spans.len() >= 3);
    }

    #[test]
    fn test_overwrite() {
        let mut m = StatusManager::new();
        m.set("k1".into(), "old".into());
        m.set("k1".into(), "new".into());
        let spans = m.render_spans();
        assert!(spans.iter().any(|s| s.content == "new"));
    }
}
