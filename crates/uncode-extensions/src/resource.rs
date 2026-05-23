//! Resource Discovery — extension dynamic resource path injection.
//!
//! Extensions register additional paths that the LLM agent's file tools can access
//! beyond the project directory sandbox.

use serde::{Deserialize, Serialize};

/// A resource path registration from an extension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourcePathConfig {
    /// Absolute path or path relative to CWD.
    pub path: String,
    /// Human-readable description of the resource (helps the LLM understand context).
    pub description: String,
}

impl ResourcePathConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.path.is_empty() {
            return Err("resource path cannot be empty".into());
        }
        if self.description.is_empty() {
            return Err("resource description cannot be empty".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_config_validate_ok() {
        let config = ResourcePathConfig {
            path: "/usr/share/templates".into(),
            description: "Shared template library".into(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_resource_config_validate_empty_path() {
        let config = ResourcePathConfig {
            path: "".into(),
            description: "desc".into(),
        };
        assert!(config.validate().unwrap_err().contains("path"));
    }

    #[test]
    fn test_resource_config_validate_empty_description() {
        let config = ResourcePathConfig {
            path: "/some/path".into(),
            description: "".into(),
        };
        assert!(config.validate().unwrap_err().contains("description"));
    }

    #[test]
    fn test_resource_config_roundtrip() {
        let config = ResourcePathConfig {
            path: "~/.config/app".into(),
            description: "App config".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ResourcePathConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }
}
