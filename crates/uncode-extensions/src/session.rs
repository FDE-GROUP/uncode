//! Session tree operations — extension fork/navigate/switch/set_name.
//!
//! Extensions can manipulate the session tree: fork branches, navigate to
//! historical nodes, switch sessions, and set display names.
//! Mirrors Pi's `ctx.fork()`, `ctx.navigateTree()`, `ctx.switchSession()`.

use serde::{Deserialize, Serialize};

/// Action an extension requests on the session tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SessionAction {
    /// Fork a new branch from the specified entry.
    Fork { entry_id: String },
    /// Navigate the session tree to the specified entry.
    Navigate { entry_id: String },
    /// Switch to a different session entirely.
    Switch { session_id: String },
    /// Set the display name of the current session.
    SetName { name: String },
}

impl SessionAction {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Fork { entry_id } if entry_id.is_empty() => {
                Err("session fork: entry_id cannot be empty".into())
            }
            Self::Navigate { entry_id } if entry_id.is_empty() => {
                Err("session navigate: entry_id cannot be empty".into())
            }
            Self::Switch { session_id } if session_id.is_empty() => {
                Err("session switch: session_id cannot be empty".into())
            }
            Self::SetName { name } if name.is_empty() => {
                Err("session set_name: name cannot be empty".into())
            }
            _ => Ok(()),
        }
    }
}

/// Response from a session action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum SessionResponse {
    /// Returned by Fork — contains the new session ID.
    Forked { session_id: String },
    /// Generic success.
    Ok,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_fork_validate_ok() {
        let action = SessionAction::Fork {
            entry_id: "abc123".into(),
        };
        assert!(action.validate().is_ok());
    }

    #[test]
    fn test_action_fork_validate_empty() {
        let action = SessionAction::Fork {
            entry_id: "".into(),
        };
        assert!(action.validate().unwrap_err().contains("entry_id"));
    }

    #[test]
    fn test_action_navigate_validate_empty() {
        let action = SessionAction::Navigate {
            entry_id: "".into(),
        };
        assert!(action.validate().unwrap_err().contains("entry_id"));
    }

    #[test]
    fn test_action_switch_validate_empty() {
        let action = SessionAction::Switch {
            session_id: "".into(),
        };
        assert!(action.validate().unwrap_err().contains("session_id"));
    }

    #[test]
    fn test_action_set_name_validate_empty() {
        let action = SessionAction::SetName { name: "".into() };
        assert!(action.validate().unwrap_err().contains("name"));
    }

    #[test]
    fn test_response_roundtrip() {
        let resp = SessionResponse::Forked {
            session_id: "new-session".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: SessionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }
}
