//! Overlay manager — manages multiple extension overlays with z-ordering.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
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
                    s = s.add_modifier(Modifier::BOLD);
                }
            }
            Line::from(ratatui::text::Span::styled(line.clone(), s))
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", config.key))
                .title_alignment(Alignment::Center),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, rect);
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
