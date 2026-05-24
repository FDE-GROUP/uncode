//! Extension feature-flag registry.
//!
//! Extensions register named flags with default values; other extensions can query them.
//! This is a local DashMap-based store — no host callbacks needed.
//!
//! **Pi:** `pi.registerFlag(name, defaultValue)` / `pi.getFlag(name)`.

use dashmap::DashMap;
use std::sync::Arc;

/// A feature-flag value. Extensions can register bool, string, number, or null flags.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum FlagValue {
    Bool(bool),
    String(String),
    Number(f64),
    Null,
}

impl FlagValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }
}

/// Registry for extension feature flags.
///
/// **Pi:** corresponds to the flag subsystem of Pi's extension API.
pub struct FlagRegistry {
    flags: DashMap<String, FlagValue>,
}

impl FlagRegistry {
    pub fn new() -> Self {
        Self {
            flags: DashMap::new(),
        }
    }

    /// Register a flag with a default value. Overwrites any existing value.
    ///
    /// **Pi:** `pi.registerFlag(name, defaultValue)`.
    pub fn register(&self, name: String, default: FlagValue) {
        self.flags.insert(name, default);
    }

    /// Get the current value of a flag. Returns `None` if unregistered.
    ///
    /// **Pi:** `pi.getFlag(name)`.
    pub fn get(&self, name: &str) -> Option<FlagValue> {
        self.flags.get(name).map(|v| v.value().clone())
    }

    /// Check if a flag is registered.
    pub fn exists(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    /// Remove a flag registration. Returns `true` if it existed.
    pub fn unregister(&self, name: &str) -> bool {
        self.flags.remove(name).is_some()
    }

    /// List all registered flag names.
    pub fn flag_names(&self) -> Vec<String> {
        self.flags.iter().map(|e| e.key().clone()).collect()
    }
}

impl Default for FlagRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared flag registry handle — stored inside `ExtensionApi`.
pub type SharedFlagRegistry = Arc<FlagRegistry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get_bool() {
        let reg = FlagRegistry::new();
        reg.register("dark_mode".into(), FlagValue::Bool(true));
        let val = reg.get("dark_mode").unwrap();
        assert_eq!(val.as_bool(), Some(true));
    }

    #[test]
    fn register_and_get_string() {
        let reg = FlagRegistry::new();
        reg.register("theme".into(), FlagValue::String("monokai".into()));
        let val = reg.get("theme").unwrap();
        assert_eq!(val.as_str(), Some("monokai"));
    }

    #[test]
    fn register_and_get_number() {
        let reg = FlagRegistry::new();
        reg.register("max_retries".into(), FlagValue::Number(3.0));
        let val = reg.get("max_retries").unwrap();
        assert_eq!(val.as_f64(), Some(3.0));
    }

    #[test]
    fn get_unregistered_returns_none() {
        let reg = FlagRegistry::new();
        assert!(reg.get("nope").is_none());
    }

    #[test]
    fn unregister_removes_flag() {
        let reg = FlagRegistry::new();
        reg.register("x".into(), FlagValue::Bool(false));
        assert!(reg.unregister("x"));
        assert!(reg.get("x").is_none());
    }

    #[test]
    fn unregister_nonexistent_returns_false() {
        let reg = FlagRegistry::new();
        assert!(!reg.unregister("nope"));
    }

    #[test]
    fn overwrite_existing() {
        let reg = FlagRegistry::new();
        reg.register("flag".into(), FlagValue::Bool(false));
        reg.register("flag".into(), FlagValue::Bool(true));
        assert_eq!(reg.get("flag").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn flag_names_lists_all() {
        let reg = FlagRegistry::new();
        reg.register("a".into(), FlagValue::Null);
        reg.register("b".into(), FlagValue::Null);
        let mut names = reg.flag_names();
        names.sort();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn null_flag_value() {
        let reg = FlagRegistry::new();
        reg.register("empty".into(), FlagValue::Null);
        assert!(matches!(reg.get("empty"), Some(FlagValue::Null)));
    }
}
