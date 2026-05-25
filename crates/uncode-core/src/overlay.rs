//! Extension overlay types — declarative overlay configuration for TUI rendering.

use serde::{Deserialize, Serialize};

/// Overlay configuration — how an overlay is positioned and sized.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverlayConfig {
    /// Unique key identifying this overlay instance.
    pub key: String,
    /// Overlay width. Defaults to 60% of screen.
    #[serde(default)]
    pub width: Option<SizeValue>,
    /// Overlay height. Defaults to 50% of screen.
    #[serde(default)]
    pub height: Option<SizeValue>,
    /// Anchor position on screen.
    #[serde(default = "default_anchor")]
    pub anchor: OverlayAnchor,
    /// Whether this overlay captures keyboard input.
    #[serde(default)]
    pub capturing: bool,
}

fn default_anchor() -> OverlayAnchor {
    OverlayAnchor::Center
}

/// Size value — fixed characters or percentage of screen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SizeValue {
    Fixed(u16),
    Percent(u16),
}

/// Anchor position for overlay placement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum OverlayAnchor {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
}

/// Overlay content — text lines with per-line styles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OverlayContent {
    pub lines: Vec<String>,
    #[serde(default)]
    pub styles: Vec<OverlayStyle>,
}

/// Per-line style descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OverlayStyle {
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub bold: bool,
}

/// Actions an extension can perform on overlays.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OverlayAction {
    Show {
        config: OverlayConfig,
        content: OverlayContent,
    },
    Hide {
        key: String,
    },
    Update {
        key: String,
        content: OverlayContent,
    },
}

impl OverlayConfig {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.key.is_empty() {
            return Err("overlay key must not be empty".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_validate_ok() {
        let config = OverlayConfig {
            key: "my_overlay".into(),
            width: Some(SizeValue::Percent(80)),
            height: Some(SizeValue::Fixed(20)),
            anchor: OverlayAnchor::Center,
            capturing: true,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_validate_empty_key() {
        let config = OverlayConfig {
            key: String::new(),
            width: None,
            height: None,
            anchor: OverlayAnchor::Center,
            capturing: false,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn action_roundtrip() {
        let action = OverlayAction::Show {
            config: OverlayConfig {
                key: "test".into(),
                width: None,
                height: None,
                anchor: OverlayAnchor::Center,
                capturing: false,
            },
            content: OverlayContent {
                lines: vec!["Hello".into()],
                styles: vec![OverlayStyle {
                    fg: Some("red".into()),
                    bg: None,
                    bold: true,
                }],
            },
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: OverlayAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }
}
