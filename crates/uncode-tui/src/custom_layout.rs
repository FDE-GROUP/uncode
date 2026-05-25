//! Custom header / footer / working indicator types for TUI-side rendering.
//!
//! Converts extension-side `HeaderConfig` / `FooterConfig` / `WorkingIndicatorConfig`
//! into ratatui-native types.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use uncode_extensions::header_footer::{
    FooterConfig, HeaderConfig, LineSpan, WorkingIndicatorConfig,
};

/// Resolved header ready for TUI rendering.
pub struct CustomHeader {
    pub lines: Vec<Line<'static>>,
}

impl CustomHeader {
    pub fn from_config(config: &HeaderConfig) -> Self {
        Self {
            lines: config
                .lines
                .iter()
                .map(|l| convert_line(&l.spans))
                .collect(),
        }
    }

    pub fn line_count(&self) -> u16 {
        self.lines.len() as u16
    }
}

/// Resolved footer ready for TUI rendering.
pub struct CustomFooter {
    pub lines: Vec<Line<'static>>,
    pub show_git_branch: bool,
    pub show_model: bool,
    pub show_extension_statuses: bool,
}

impl CustomFooter {
    pub fn from_config(config: &FooterConfig) -> Self {
        Self {
            lines: config
                .lines
                .iter()
                .map(|l| convert_line(&l.spans))
                .collect(),
            show_git_branch: config.show_git_branch,
            show_model: config.show_model,
            show_extension_statuses: config.show_extension_statuses,
        }
    }
}

/// Resolved working indicator.
pub struct CustomIndicator {
    pub frames: Vec<String>,
    /// Divisor for tick-to-frame mapping: `frame_index = (tick / tick_divisor) % frames.len()`.
    pub tick_divisor: u64,
}

impl CustomIndicator {
    pub fn from_config(config: &WorkingIndicatorConfig) -> Self {
        // poll interval is ~50ms; tick_divisor = interval_ms / 50
        let tick_divisor = (config.interval_ms / 50).max(1);
        Self {
            frames: config.frames.clone(),
            tick_divisor,
        }
    }

    /// Get the frame string for the current tick.
    pub fn frame_at(&self, tick: u64) -> &str {
        let idx = (tick / self.tick_divisor) as usize % self.frames.len();
        &self.frames[idx]
    }
}

fn convert_line(spans: &[LineSpan]) -> Line<'static> {
    Line::from(
        spans
            .iter()
            .map(|s| {
                let mut style = Style::default();
                if let Some(fg) = &s.fg {
                    if let Some(c) = parse_color(fg) {
                        style = style.fg(c);
                    }
                }
                if let Some(bg) = &s.bg {
                    if let Some(c) = parse_color(bg) {
                        style = style.bg(c);
                    }
                }
                if s.bold {
                    style = style.bold();
                }
                Span::styled(s.text.clone(), style)
            })
            .collect::<Vec<_>>(),
    )
}

/// Parse a color string: CSS name or hex (#rrggbb).
fn parse_color(s: &str) -> Option<Color> {
    if let Some(hex) = s.strip_prefix('#') {
        parse_hex_color(hex)
    } else {
        parse_named_color(s)
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

fn parse_named_color(name: &str) -> Option<Color> {
    match name.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "darkgray" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "lightgray" => Some(Color::Gray),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_extensions::header_footer::{HeaderConfig, HeaderFooterLine};

    fn make_span(text: &str, fg: Option<&str>, bg: Option<&str>, bold: bool) -> LineSpan {
        LineSpan {
            text: text.into(),
            fg: fg.map(|s| s.into()),
            bg: bg.map(|s| s.into()),
            bold,
        }
    }

    #[test]
    fn test_custom_header_from_config() {
        let config = HeaderConfig {
            lines: vec![HeaderFooterLine {
                spans: vec![
                    make_span("hello ", None, None, false),
                    make_span("world", Some("cyan"), None, true),
                ],
            }],
        };
        let header = CustomHeader::from_config(&config);
        assert_eq!(header.lines.len(), 1);
        assert_eq!(header.line_count(), 1);
    }

    #[test]
    fn test_custom_footer_from_config() {
        let config = FooterConfig {
            lines: vec![HeaderFooterLine {
                spans: vec![make_span("status", Some("#ff6600"), None, false)],
            }],
            show_git_branch: false,
            show_model: true,
            show_extension_statuses: true,
        };
        let footer = CustomFooter::from_config(&config);
        assert_eq!(footer.lines.len(), 1);
        assert!(!footer.show_git_branch);
        assert!(footer.show_model);
    }

    #[test]
    fn test_custom_indicator_from_config() {
        let config = WorkingIndicatorConfig {
            frames: vec!["⠋".into(), "⠙".into(), "⠹".into()],
            interval_ms: 100,
        };
        let indicator = CustomIndicator::from_config(&config);
        assert_eq!(indicator.frames.len(), 3);
        assert_eq!(indicator.tick_divisor, 2); // 100 / 50 = 2
        // Tick 0 → frame 0, tick 2 → frame 1, tick 4 → frame 0
        assert_eq!(indicator.frame_at(0), "⠋");
        assert_eq!(indicator.frame_at(2), "⠙");
        assert_eq!(indicator.frame_at(4), "⠹");
        assert_eq!(indicator.frame_at(6), "⠋"); // wraps
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_color("#ff6600"), Some(Color::Rgb(255, 102, 0)));
        assert_eq!(parse_color("#000000"), Some(Color::Rgb(0, 0, 0)));
    }

    #[test]
    fn test_parse_named_color() {
        assert_eq!(parse_color("red"), Some(Color::Red));
        assert_eq!(parse_color("CYAN"), Some(Color::Cyan));
        assert_eq!(parse_color("darkgray"), Some(Color::DarkGray));
        assert_eq!(parse_color("unknown"), None);
    }

    #[test]
    fn test_parse_invalid_hex() {
        assert_eq!(parse_color("#xyz"), None);
        assert_eq!(parse_color("#12345"), None);
    }
}
