//! Message-type-level custom rendering — extensions override built-in message display.

use std::collections::HashMap;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// Trait for custom message rendering by message type.
pub trait MessageRenderer: Send + Sync {
    fn render(&self, content: &str, width: u16, theme: &Theme) -> Vec<Line<'static>>;
}

/// Registry for message-type-level custom renderers.
pub struct MessageRendererRegistry {
    renderers: HashMap<String, Box<dyn MessageRenderer>>,
}

impl MessageRendererRegistry {
    pub fn new() -> Self {
        Self {
            renderers: HashMap::new(),
        }
    }

    pub fn register(&mut self, message_type: String, renderer: Box<dyn MessageRenderer>) {
        self.renderers.insert(message_type, renderer);
    }

    pub fn get(&self, message_type: &str) -> Option<&dyn MessageRenderer> {
        self.renderers.get(message_type).map(|r| r.as_ref())
    }
}

impl Default for MessageRendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Template-based message renderer built from `MessageRenderConfig`.
pub struct TemplateMessageRenderer {
    template: String,
    max_lines: usize,
}

impl TemplateMessageRenderer {
    pub fn new(template: String, max_lines: usize) -> Self {
        Self {
            template,
            max_lines,
        }
    }
}

impl MessageRenderer for TemplateMessageRenderer {
    fn render(&self, content: &str, width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let rendered = self
            .template
            .replace("{text}", content)
            .replace("{content}", content);
        let w = width.saturating_sub(2) as usize;
        let max = if self.max_lines == 0 {
            usize::MAX
        } else {
            self.max_lines
        };
        rendered
            .lines()
            .take(max)
            .map(|l| {
                let text = if l.len() > w {
                    let end = l.floor_char_boundary(w);
                    l[..end].to_string()
                } else {
                    l.to_string()
                };
                Line::from(Span::styled(text, Style::default().fg(theme.ui.agent_text)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let reg = MessageRendererRegistry::new();
        assert!(reg.get("thinking").is_none());
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = MessageRendererRegistry::new();
        reg.register(
            "thinking".into(),
            Box::new(TemplateMessageRenderer::new("💭 {text}".into(), 20)),
        );
        assert!(reg.get("thinking").is_some());
        assert!(reg.get("assistant").is_none());
    }

    #[test]
    fn test_template_renderer() {
        let renderer = TemplateMessageRenderer::new("💭 {text}".into(), 20);
        let theme = Theme::default();
        let lines = renderer.render("hello world", 80, &theme);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().contains("💭 hello world"));
    }

    #[test]
    fn test_template_renderer_max_lines() {
        let renderer = TemplateMessageRenderer::new("{text}".into(), 2);
        let theme = Theme::default();
        let lines = renderer.render("a\nb\nc\nd", 80, &theme);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_template_renderer_unlimited() {
        let renderer = TemplateMessageRenderer::new("{text}".into(), 0);
        let theme = Theme::default();
        let content: String = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let lines = renderer.render(&content, 80, &theme);
        assert_eq!(lines.len(), 100);
    }

    #[test]
    fn test_template_renderer_width_truncation() {
        let renderer = TemplateMessageRenderer::new("{text}".into(), 0);
        let theme = Theme::default();
        let long = "a".repeat(200);
        let lines = renderer.render(&long, 40, &theme);
        assert_eq!(lines.len(), 1);
        // Should be truncated to width-2 = 38 chars
        let rendered = lines[0].to_string();
        assert!(rendered.len() <= 40);
    }

    #[test]
    fn test_template_content_placeholder() {
        let renderer = TemplateMessageRenderer::new("📊 {content}".into(), 0);
        let theme = Theme::default();
        let lines = renderer.render("data", 80, &theme);
        assert!(lines[0].to_string().contains("📊 data"));
    }
}
