//! Extension UI action types — widget placement, status indicator, notifications.

use serde::{Deserialize, Serialize};

/// Widget configuration — fixed-position text component above or below the editor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetConfig {
    /// Unique key identifying this widget instance.
    pub key: String,
    /// Where to place the widget.
    pub placement: WidgetPlacement,
    /// Text lines to display (max 10).
    pub content: Vec<String>,
}

/// Widget placement relative to the input editor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WidgetPlacement {
    AboveEditor,
    BelowEditor,
}

/// Notification severity level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotifyType {
    Info,
    Warning,
    Error,
}

/// Actions an extension can perform on the TUI UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UiAction {
    SetWidget {
        config: WidgetConfig,
    },
    RemoveWidget {
        key: String,
    },
    SetStatus {
        key: String,
        text: Option<String>,
    },
    CustomMessage {
        message_type: String,
        content: String,
    },
    SetTitle {
        title: String,
    },
    SetWorkingMessage {
        message: String,
    },
    SetWorkingVisible {
        visible: bool,
    },
    SetToolsExpanded {
        expanded: bool,
    },
}

impl WidgetConfig {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.key.is_empty() {
            return Err("widget key must not be empty".into());
        }
        if self.content.len() > 10 {
            return Err("widget content exceeds 10 lines".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_config_validate_ok() {
        let config = WidgetConfig {
            key: "my_widget".into(),
            placement: WidgetPlacement::AboveEditor,
            content: vec!["Hello".into()],
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn widget_config_validate_empty_key() {
        let config = WidgetConfig {
            key: String::new(),
            placement: WidgetPlacement::BelowEditor,
            content: vec![],
        };
        assert!(config.validate().unwrap_err().contains("empty"));
    }

    #[test]
    fn widget_config_validate_too_many_lines() {
        let config = WidgetConfig {
            key: "w".into(),
            placement: WidgetPlacement::AboveEditor,
            content: (0..11).map(|i| format!("line {i}")).collect(),
        };
        assert!(config.validate().unwrap_err().contains("10 lines"));
    }

    #[test]
    fn ui_action_roundtrip() {
        let action = UiAction::SetWidget {
            config: WidgetConfig {
                key: "test".into(),
                placement: WidgetPlacement::AboveEditor,
                content: vec!["Hello".into(), "World".into()],
            },
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: UiAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn set_status_clear_roundtrip() {
        let action = UiAction::SetStatus {
            key: "ext1".into(),
            text: None,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: UiAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn custom_message_roundtrip() {
        let action = UiAction::CustomMessage {
            message_type: "data_table".into(),
            content: "row1\nrow2".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: UiAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn set_title_roundtrip() {
        let action = UiAction::SetTitle {
            title: "My Project".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: UiAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn set_working_message_roundtrip() {
        let action = UiAction::SetWorkingMessage {
            message: "Compiling...".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: UiAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn set_working_visible_roundtrip() {
        let action = UiAction::SetWorkingVisible { visible: false };
        let json = serde_json::to_string(&action).unwrap();
        let back: UiAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn set_tools_expanded_roundtrip() {
        let action = UiAction::SetToolsExpanded { expanded: true };
        let json = serde_json::to_string(&action).unwrap();
        let back: UiAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }
}
