//! Header / Footer / Working Indicator — extension-side configuration types.
//!
//! Extensions call `set_header()`, `set_footer()`, `set_working_indicator()` to replace
//! the built-in TUI header, footer, and working indicator. Passing `None` restores defaults.

use serde::{Deserialize, Serialize};

/// Maximum number of lines allowed in header or footer.
const MAX_LINES: usize = 4;

/// Minimum frames for a working indicator animation.
const MIN_FRAMES: usize = 2;
/// Maximum frames for a working indicator animation.
const MAX_FRAMES: usize = 20;
/// Minimum animation interval in milliseconds.
const MIN_INTERVAL_MS: u64 = 80;
/// Maximum animation interval in milliseconds.
const MAX_INTERVAL_MS: u64 = 1000;

/// Custom header configuration.
///
/// When set, the TUI renders these lines above the chat area.
/// Up to 4 lines. Each line contains styled spans.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeaderConfig {
    pub lines: Vec<HeaderFooterLine>,
}

impl HeaderConfig {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.lines.is_empty() {
            return Err("header lines cannot be empty".into());
        }
        if self.lines.len() > MAX_LINES {
            return Err(format!(
                "header cannot exceed {MAX_LINES} lines, got {}",
                self.lines.len()
            ));
        }
        for (i, line) in self.lines.iter().enumerate() {
            line.validate(i)?;
        }
        Ok(())
    }
}

/// Custom footer configuration.
///
/// When set, the TUI replaces built-in footer lines with these.
/// Boolean flags control whether built-in info (git branch, model, extension statuses)
/// is appended after custom lines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FooterConfig {
    pub lines: Vec<HeaderFooterLine>,
    #[serde(default = "default_true")]
    pub show_git_branch: bool,
    #[serde(default = "default_true")]
    pub show_model: bool,
    #[serde(default = "default_true")]
    pub show_extension_statuses: bool,
}

fn default_true() -> bool {
    true
}

impl FooterConfig {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.lines.is_empty() {
            return Err("footer lines cannot be empty".into());
        }
        if self.lines.len() > MAX_LINES {
            return Err(format!(
                "footer cannot exceed {MAX_LINES} lines, got {}",
                self.lines.len()
            ));
        }
        for (i, line) in self.lines.iter().enumerate() {
            line.validate(i)?;
        }
        Ok(())
    }
}

/// A single line in a header or footer, consisting of styled spans.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeaderFooterLine {
    pub spans: Vec<LineSpan>,
}

impl HeaderFooterLine {
    fn validate(&self, line_index: usize) -> Result<(), String> {
        if self.spans.is_empty() {
            return Err(format!("line {line_index} has no spans"));
        }
        for (j, span) in self.spans.iter().enumerate() {
            span.validate(line_index, j)?;
        }
        Ok(())
    }
}

/// A styled text span within a header or footer line.
///
/// Colors accept CSS-style names ("red", "cyan") or hex strings ("#ff6600").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineSpan {
    pub text: String,
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub bold: bool,
}

impl LineSpan {
    fn validate(&self, line_index: usize, span_index: usize) -> Result<(), String> {
        if self.text.is_empty() {
            return Err(format!("span [{line_index}][{span_index}] has empty text"));
        }
        Ok(())
    }
}

/// Custom working indicator (spinner) configuration.
///
/// When set, replaces the built-in ●/○ animation with custom frames
/// displayed at the specified interval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkingIndicatorConfig {
    /// Animation frames, e.g. `["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"]`.
    pub frames: Vec<String>,
    /// Milliseconds between frame transitions (80–1000).
    pub interval_ms: u64,
}

impl WorkingIndicatorConfig {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.frames.len() < MIN_FRAMES {
            return Err(format!(
                "working indicator needs at least {MIN_FRAMES} frames, got {}",
                self.frames.len()
            ));
        }
        if self.frames.len() > MAX_FRAMES {
            return Err(format!(
                "working indicator cannot exceed {MAX_FRAMES} frames, got {}",
                self.frames.len()
            ));
        }
        if self.frames.iter().any(|f| f.is_empty()) {
            return Err("working indicator frames must not be empty strings".into());
        }
        if self.interval_ms < MIN_INTERVAL_MS {
            return Err(format!(
                "interval_ms must be >= {MIN_INTERVAL_MS}, got {}",
                self.interval_ms
            ));
        }
        if self.interval_ms > MAX_INTERVAL_MS {
            return Err(format!(
                "interval_ms must be <= {MAX_INTERVAL_MS}, got {}",
                self.interval_ms
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_span(text: &str) -> LineSpan {
        LineSpan {
            text: text.into(),
            fg: None,
            bg: None,
            bold: false,
        }
    }

    fn make_line(texts: &[&str]) -> HeaderFooterLine {
        HeaderFooterLine {
            spans: texts.iter().map(|t| make_span(t)).collect(),
        }
    }

    #[test]
    fn test_header_config_validate_ok() {
        let config = HeaderConfig {
            lines: vec![make_line(&["uncode", " v1.0"])],
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_header_config_validate_empty() {
        let config = HeaderConfig { lines: vec![] };
        assert!(config.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn test_header_config_validate_too_many_lines() {
        let config = HeaderConfig {
            lines: vec![
                make_line(&["a"]),
                make_line(&["b"]),
                make_line(&["c"]),
                make_line(&["d"]),
                make_line(&["e"]),
            ],
        };
        assert!(config.validate().unwrap_err().contains("4 lines"));
    }

    #[test]
    fn test_header_config_validate_empty_span() {
        let config = HeaderConfig {
            lines: vec![HeaderFooterLine {
                spans: vec![make_span("")],
            }],
        };
        assert!(config.validate().unwrap_err().contains("empty text"));
    }

    #[test]
    fn test_footer_config_validate_ok() {
        let config = FooterConfig {
            lines: vec![make_line(&["status: ok"])],
            show_git_branch: true,
            show_model: false,
            show_extension_statuses: true,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_footer_config_validate_empty() {
        let config = FooterConfig {
            lines: vec![],
            show_git_branch: true,
            show_model: true,
            show_extension_statuses: true,
        };
        assert!(config.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn test_indicator_config_validate_ok() {
        let config = WorkingIndicatorConfig {
            frames: vec!["⠋".into(), "⠙".into(), "⠹".into()],
            interval_ms: 120,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_indicator_config_validate_too_few_frames() {
        let config = WorkingIndicatorConfig {
            frames: vec!["●".into()],
            interval_ms: 100,
        };
        assert!(config.validate().unwrap_err().contains("at least 2"));
    }

    #[test]
    fn test_indicator_config_validate_too_many_frames() {
        let config = WorkingIndicatorConfig {
            frames: (0..25).map(|i| format!("f{i}")).collect(),
            interval_ms: 100,
        };
        assert!(config.validate().unwrap_err().contains("exceed 20"));
    }

    #[test]
    fn test_indicator_config_validate_empty_frame() {
        let config = WorkingIndicatorConfig {
            frames: vec!["a".into(), "".into()],
            interval_ms: 100,
        };
        assert!(config.validate().unwrap_err().contains("empty strings"));
    }

    #[test]
    fn test_indicator_config_validate_interval_too_low() {
        let config = WorkingIndicatorConfig {
            frames: vec!["a".into(), "b".into()],
            interval_ms: 10,
        };
        assert!(config.validate().unwrap_err().contains(">= 80"));
    }

    #[test]
    fn test_indicator_config_validate_interval_too_high() {
        let config = WorkingIndicatorConfig {
            frames: vec!["a".into(), "b".into()],
            interval_ms: 5000,
        };
        assert!(config.validate().unwrap_err().contains("<= 1000"));
    }

    #[test]
    fn test_header_roundtrip() {
        let config = HeaderConfig {
            lines: vec![HeaderFooterLine {
                spans: vec![LineSpan {
                    text: "hello".into(),
                    fg: Some("cyan".into()),
                    bg: None,
                    bold: true,
                }],
            }],
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: HeaderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_footer_roundtrip() {
        let config = FooterConfig {
            lines: vec![make_line(&["test"])],
            show_git_branch: false,
            show_model: true,
            show_extension_statuses: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: FooterConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_indicator_roundtrip() {
        let config = WorkingIndicatorConfig {
            frames: vec!["●".into(), "○".into()],
            interval_ms: 200,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: WorkingIndicatorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }
}
