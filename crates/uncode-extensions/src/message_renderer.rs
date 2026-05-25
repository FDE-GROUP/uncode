//! Extension message renderer types — declarative custom rendering by message type.

use serde::{Deserialize, Serialize};

/// Configuration for a message-type-level custom renderer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageRenderConfig {
    /// Message type this renderer applies to (e.g., "thinking", "assistant", "custom_data").
    pub message_type: String,
    /// Template with `{text}`, `{content}` placeholders.
    pub render_template: String,
    /// Display style.
    #[serde(default)]
    pub style: MessageRenderStyle,
    /// Maximum lines to render (0 = unlimited).
    #[serde(default)]
    pub result_max_lines: usize,
}

/// Display style for custom message rendering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MessageRenderStyle {
    #[default]
    Inline,
    Collapsible,
    Block,
}

impl MessageRenderConfig {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.message_type.is_empty() {
            return Err("message_type must not be empty".into());
        }
        if self.render_template.is_empty() {
            return Err("render_template must not be empty".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_validate_ok() {
        let config = MessageRenderConfig {
            message_type: "thinking".into(),
            render_template: "💭 {text}".into(),
            style: MessageRenderStyle::Collapsible,
            result_max_lines: 20,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_validate_empty_type() {
        let config = MessageRenderConfig {
            message_type: String::new(),
            render_template: "{text}".into(),
            style: MessageRenderStyle::Inline,
            result_max_lines: 0,
        };
        assert!(config.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn config_validate_empty_template() {
        let config = MessageRenderConfig {
            message_type: "code".into(),
            render_template: String::new(),
            style: MessageRenderStyle::Block,
            result_max_lines: 0,
        };
        assert!(config.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn config_roundtrip() {
        let config = MessageRenderConfig {
            message_type: "custom_data".into(),
            render_template: "📊 {text}".into(),
            style: MessageRenderStyle::Block,
            result_max_lines: 50,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: MessageRenderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }
}
