//! Overlay manager — manages multiple extension overlays with z-ordering.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};

use uncode_core::overlay::{OverlayAnchor, OverlayConfig, OverlayContent, SizeValue};

struct OverlayInstance {
    config: OverlayConfig,
    content: OverlayContent,
    visible: bool,
}

pub struct OverlayManager {
    overlays: HashMap<String, OverlayInstance>,
    z_order: Vec<String>,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            overlays: HashMap::new(),
            z_order: Vec::new(),
        }
    }

    pub fn show(&mut self, config: OverlayConfig, content: OverlayContent) {
        let key = config.key.clone();
        let existed = self.overlays.contains_key(&key);
        self.overlays.insert(
            key.clone(),
            OverlayInstance {
                config,
                content,
                visible: true,
            },
        );
        if !existed {
            self.z_order.push(key);
        }
    }

    pub fn hide(&mut self, key: &str) {
        if let Some(inst) = self.overlays.get_mut(key) {
            inst.visible = false;
        }
    }

    pub fn update(&mut self, key: &str, content: OverlayContent) {
        if let Some(inst) = self.overlays.get_mut(key) {
            inst.content = content;
        }
    }

    pub fn has_visible(&self) -> bool {
        self.z_order
            .iter()
            .any(|k| self.overlays.get(k).map_or(false, |o| o.visible))
    }

    /// Whether the topmost visible overlay captures keyboard input.
    pub fn top_capturing(&self) -> bool {
        let top_key = self
            .z_order
            .iter()
            .rev()
            .find(|k| self.overlays.get(*k).map_or(false, |o| o.visible));
        top_key
            .and_then(|k| self.overlays.get(k))
            .map_or(false, |o| o.config.capturing)
    }

    /// Handle Escape key — dismiss the topmost overlay. Returns true if handled.
    pub fn handle_escape(&mut self) -> bool {
        let top_key = self
            .z_order
            .iter()
            .rev()
            .find(|k| self.overlays.get(*k).map_or(false, |o| o.visible))
            .cloned();
        if let Some(key) = top_key {
            self.hide(&key);
            true
        } else {
            false
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        for key in &self.z_order {
            if let Some(inst) = self.overlays.get(key) {
                if inst.visible {
                    render_overlay(f, area, &inst.config, &inst.content);
                }
            }
        }
    }
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

fn compute_rect(config: &OverlayConfig, area: Rect) -> Rect {
    let w = match config.width {
        Some(SizeValue::Fixed(v)) => v.min(area.width),
        Some(SizeValue::Percent(p)) => area.width * p.min(100) / 100,
        None => area.width * 60 / 100,
    };
    let h = match config.height {
        Some(SizeValue::Fixed(v)) => v.min(area.height),
        Some(SizeValue::Percent(p)) => area.height * p.min(100) / 100,
        None => area.height * 50 / 100,
    };
    let (x, y) = match config.anchor {
        OverlayAnchor::Center => (
            area.x + (area.width.saturating_sub(w)) / 2,
            area.y + (area.height.saturating_sub(h)) / 2,
        ),
        OverlayAnchor::TopLeft => (area.x, area.y),
        OverlayAnchor::TopRight => (area.x + area.width.saturating_sub(w), area.y),
        OverlayAnchor::BottomLeft => (area.x, area.y + area.height.saturating_sub(h)),
        OverlayAnchor::BottomRight => (
            area.x + area.width.saturating_sub(w),
            area.y + area.height.saturating_sub(h),
        ),
        OverlayAnchor::TopCenter => (area.x + (area.width.saturating_sub(w)) / 2, area.y),
        OverlayAnchor::BottomCenter => (
            area.x + (area.width.saturating_sub(w)) / 2,
            area.y + area.height.saturating_sub(h),
        ),
    };
    Rect::new(x, y, w, h)
}

fn render_overlay(f: &mut Frame, area: Rect, config: &OverlayConfig, content: &OverlayContent) {
    let rect = compute_rect(config, area);
    f.render_widget(Clear, rect);

    let lines: Vec<Line> = content
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let style = content.styles.get(i);
            let mut s = Style::default();
            if let Some(st) = style {
                if let Some(ref fg) = st.fg {
                    s = s.fg(parse_color(fg));
                }
                if let Some(ref bg) = st.bg {
                    s = s.bg(parse_color(bg));
                }
                if st.bold {
                    s = s.bold();
                }
            }
            Line::from(ratatui::text::Span::styled(line.clone(), s))
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(format!(" {} ", config.key))
                .title_alignment(Alignment::Center),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(key: &str) -> OverlayConfig {
        OverlayConfig {
            key: key.to_string(),
            width: None,
            height: None,
            anchor: OverlayAnchor::Center,
            capturing: false,
        }
    }

    fn make_content() -> OverlayContent {
        OverlayContent::default()
    }

    #[test]
    fn new_starts_empty() {
        let mgr = OverlayManager::new();
        assert!(!mgr.has_visible());
    }

    #[test]
    fn show_adds_overlay() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("test"), make_content());
        assert!(mgr.has_visible());
    }

    #[test]
    fn hide_removes_overlay() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("test"), make_content());
        mgr.hide("test");
        assert!(!mgr.has_visible());
    }

    #[test]
    fn hide_nonexistent_is_noop() {
        let mut mgr = OverlayManager::new();
        mgr.hide("nonexistent");
        assert!(!mgr.has_visible());
    }

    #[test]
    fn update_existing_overlay() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("test"), make_content());
        let new_content = OverlayContent {
            lines: vec!["updated".to_string()],
            ..Default::default()
        };
        mgr.update("test", new_content);
        assert!(mgr.has_visible());
    }

    #[test]
    fn update_nonexistent_is_noop() {
        let mut mgr = OverlayManager::new();
        mgr.update("nonexistent", make_content());
        assert!(!mgr.has_visible());
    }

    #[test]
    fn has_visible_false_when_all_hidden() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("a"), make_content());
        mgr.show(make_config("b"), make_content());
        mgr.hide("a");
        mgr.hide("b");
        assert!(!mgr.has_visible());
    }

    #[test]
    fn has_visible_true_when_at_least_one_visible() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("a"), make_content());
        mgr.show(make_config("b"), make_content());
        mgr.hide("a");
        assert!(mgr.has_visible());
    }

    #[test]
    fn top_capturing_false_when_no_visible() {
        let mgr = OverlayManager::new();
        assert!(!mgr.top_capturing());
    }

    #[test]
    fn top_capturing_false_when_not_capturing() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("test"), make_content());
        assert!(!mgr.top_capturing());
    }

    #[test]
    fn top_capturing_true_when_capturing() {
        let mut mgr = OverlayManager::new();
        let mut cfg = make_config("test");
        cfg.capturing = true;
        mgr.show(cfg, make_content());
        assert!(mgr.top_capturing());
    }

    #[test]
    fn top_capturing_only_checks_visible() {
        let mut mgr = OverlayManager::new();
        let mut cfg = make_config("capture");
        cfg.capturing = true;
        mgr.show(cfg, make_content());
        mgr.show(make_config("nocapture"), make_content());
        // "nocapture" is on top, not capturing
        assert!(!mgr.top_capturing());
        mgr.hide("nocapture");
        // "capture" is now on top, capturing
        assert!(mgr.top_capturing());
    }

    #[test]
    fn handle_escape_hides_topmost() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("a"), make_content());
        mgr.show(make_config("b"), make_content());
        assert!(mgr.handle_escape());
        assert!(mgr.has_visible());
        assert!(mgr.handle_escape());
        assert!(!mgr.has_visible());
    }

    #[test]
    fn handle_escape_returns_false_when_none_visible() {
        let mut mgr = OverlayManager::new();
        assert!(!mgr.handle_escape());
    }

    #[test]
    fn multiple_overlays_hide_one() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("a"), make_content());
        mgr.show(make_config("b"), make_content());
        mgr.hide("a");
        assert!(mgr.has_visible());
    }

    #[test]
    fn show_same_key_replaces() {
        let mut mgr = OverlayManager::new();
        mgr.show(make_config("test"), make_content());
        mgr.show(make_config("test"), make_content());
        assert!(mgr.has_visible());
        mgr.hide("test");
        assert!(!mgr.has_visible());
    }

    #[test]
    fn default_impl_same_as_new() {
        let mgr1 = OverlayManager::new();
        let mgr2 = OverlayManager::default();
        assert_eq!(mgr1.has_visible(), mgr2.has_visible());
    }
}

fn parse_color(name: &str) -> Color {
    match name.to_lowercase().as_str() {
        "red" => Color::Red,
        "green" => Color::Green,
        "blue" => Color::Blue,
        "yellow" => Color::Yellow,
        "cyan" => Color::Cyan,
        "magenta" => Color::Magenta,
        "white" => Color::White,
        "black" => Color::Black,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "dark_grey" => Color::DarkGray,
        "lightred" | "light_red" => Color::LightRed,
        "lightgreen" | "light_green" => Color::LightGreen,
        "lightblue" | "light_blue" => Color::LightBlue,
        "lightyellow" | "light_yellow" => Color::LightYellow,
        "lightcyan" | "light_cyan" => Color::LightCyan,
        "lightmagenta" | "light_magenta" => Color::LightMagenta,
        _ => Color::Reset,
    }
}
