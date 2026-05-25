//! Widget manager — manages extension widgets placed above/below the input editor.

use std::collections::HashMap;

use ratatui::{Frame, layout::Rect, style::Style, text::Line, widgets::Paragraph};

use uncode_core::ui_action::{WidgetConfig, WidgetPlacement};

struct WidgetInstance {
    placement: WidgetPlacement,
    content: Vec<String>,
}

pub struct WidgetManager {
    widgets: HashMap<String, WidgetInstance>,
    order: Vec<String>,
}

impl WidgetManager {
    pub fn new() -> Self {
        Self {
            widgets: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn set_widget(&mut self, config: WidgetConfig) {
        let key = config.key.clone();
        let existed = self.widgets.contains_key(&key);
        self.widgets.insert(
            key.clone(),
            WidgetInstance {
                placement: config.placement,
                content: config.content,
            },
        );
        if !existed {
            self.order.push(key);
        }
    }

    pub fn remove_widget(&mut self, key: &str) {
        if self.widgets.remove(key).is_some() {
            self.order.retain(|k| k != key);
        }
    }

    /// Total line count for widgets at the given placement.
    pub fn lines_for(&self, placement: WidgetPlacement) -> u16 {
        self.order
            .iter()
            .filter_map(|k| {
                let w = self.widgets.get(k)?;
                if std::mem::discriminant(&w.placement) == std::mem::discriminant(&placement) {
                    Some(w.content.len() as u16)
                } else {
                    None
                }
            })
            .sum()
    }

    pub fn render(&self, f: &mut Frame, area: Rect, placement: WidgetPlacement) {
        if area.height == 0 {
            return;
        }
        let mut y = 0u16;
        for key in &self.order {
            let w = match self.widgets.get(key) {
                Some(w) => w,
                None => continue,
            };
            if std::mem::discriminant(&w.placement) != std::mem::discriminant(&placement) {
                continue;
            }
            let h = w.content.len() as u16;
            if h == 0 || y + h > area.height {
                continue;
            }
            let rect = Rect::new(area.x, area.y + y, area.width, h);
            let lines: Vec<Line> = w
                .content
                .iter()
                .map(|l| {
                    Line::from(ratatui::text::Span::styled(
                        l.clone(),
                        Style::default().bold(),
                    ))
                })
                .collect();
            let paragraph = Paragraph::new(lines);
            f.render_widget(paragraph, rect);
            y += h;
        }
    }
}

impl Default for WidgetManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_core::ui_action::{WidgetConfig, WidgetPlacement};

    fn placement_above() -> WidgetPlacement {
        WidgetPlacement::AboveEditor
    }
    fn placement_below() -> WidgetPlacement {
        WidgetPlacement::BelowEditor
    }

    #[test]
    fn test_new_empty() {
        let m = WidgetManager::new();
        assert_eq!(m.lines_for(placement_above()), 0);
        assert_eq!(m.lines_for(placement_below()), 0);
    }

    #[test]
    fn test_set_widget_adds_order() {
        let mut m = WidgetManager::new();
        m.set_widget(WidgetConfig {
            key: "w1".into(),
            placement: placement_above(),
            content: vec!["line1".into()],
        });
        assert_eq!(m.lines_for(placement_above()), 1);
        assert_eq!(m.lines_for(placement_below()), 0);
    }

    #[test]
    fn test_set_widget_does_not_duplicate_order() {
        let mut m = WidgetManager::new();
        m.set_widget(WidgetConfig {
            key: "w1".into(),
            placement: placement_above(),
            content: vec!["a".into()],
        });
        m.set_widget(WidgetConfig {
            key: "w1".into(),
            placement: placement_above(),
            content: vec!["a".into(), "b".into()],
        });
        assert_eq!(m.lines_for(placement_above()), 2);
    }

    #[test]
    fn test_remove_widget() {
        let mut m = WidgetManager::new();
        m.set_widget(WidgetConfig {
            key: "w1".into(),
            placement: placement_above(),
            content: vec!["x".into()],
        });
        m.remove_widget("w1");
        assert_eq!(m.lines_for(placement_above()), 0);
    }

    #[test]
    fn test_remove_widget_nonexistent() {
        let mut m = WidgetManager::new();
        m.remove_widget("nonexistent"); // should not panic
    }

    #[test]
    fn test_lines_for_both_placements() {
        let mut m = WidgetManager::new();
        m.set_widget(WidgetConfig {
            key: "above1".into(),
            placement: placement_above(),
            content: vec!["a".into(), "b".into()],
        });
        m.set_widget(WidgetConfig {
            key: "below1".into(),
            placement: placement_below(),
            content: vec!["c".into()],
        });
        assert_eq!(m.lines_for(placement_above()), 2);
        assert_eq!(m.lines_for(placement_below()), 1);
    }
}
