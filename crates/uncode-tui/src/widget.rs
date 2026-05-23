//! Widget manager — manages extension widgets placed above/below the input editor.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::Paragraph,
};

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
                        Style::default().add_modifier(Modifier::BOLD),
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
