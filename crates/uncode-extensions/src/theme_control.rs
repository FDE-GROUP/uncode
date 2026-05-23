//! Theme control and thinking label customization — extension-side config types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Valid thinking level keys.
const VALID_LEVELS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh"];

/// Theme switch request — extension changes the active TUI theme by name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeControlConfig {
    /// Theme name: built-in ("dark", "light", "monokai", "solarized") or path to JSON theme file.
    pub theme_name: String,
}

impl ThemeControlConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.theme_name.is_empty() {
            return Err("theme_name cannot be empty".into());
        }
        Ok(())
    }
}

/// Thinking label customization — overrides default level labels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThinkingLabelConfig {
    /// Map from level key to custom label. Only valid keys accepted.
    /// Example: `{"high": "深度思考", "off": "关"}`.
    pub labels: HashMap<String, String>,
}

impl ThinkingLabelConfig {
    pub fn validate(&self) -> Result<(), String> {
        for key in self.labels.keys() {
            if !VALID_LEVELS.contains(&key.as_str()) {
                return Err(format!(
                    "invalid thinking level key '{}'; valid keys: {}",
                    key,
                    VALID_LEVELS.join(", ")
                ));
            }
        }
        for (key, value) in &self.labels {
            if value.is_empty() {
                return Err(format!("label for '{key}' cannot be empty"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_config_validate_ok() {
        let config = ThemeControlConfig {
            theme_name: "monokai".into(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_theme_config_validate_empty() {
        let config = ThemeControlConfig {
            theme_name: "".into(),
        };
        assert!(config.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn test_thinking_label_validate_ok() {
        let mut labels = HashMap::new();
        labels.insert("high".into(), "深度".into());
        labels.insert("off".into(), "关".into());
        let config = ThinkingLabelConfig { labels };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_thinking_label_validate_invalid_key() {
        let mut labels = HashMap::new();
        labels.insert("unknown".into(), "test".into());
        let config = ThinkingLabelConfig { labels };
        assert!(config.validate().unwrap_err().contains("invalid"));
    }

    #[test]
    fn test_thinking_label_validate_empty_value() {
        let mut labels = HashMap::new();
        labels.insert("high".into(), "".into());
        let config = ThinkingLabelConfig { labels };
        assert!(config.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn test_thinking_label_roundtrip() {
        let mut labels = HashMap::new();
        labels.insert("medium".into(), "中".into());
        let config = ThinkingLabelConfig { labels };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ThinkingLabelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }
}
