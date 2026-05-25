//! Extension command and shortcut registration types.
//!
//! These types are crossterm-independent so `uncode-extensions` does not
//! depend on the TUI crate. The TUI layer converts `ExtKeyEvent` to
//! `crossterm::KeyEvent` at dispatch time.

/// Reserved command names that extensions cannot override.
pub const RESERVED_COMMAND_NAMES: &[&str] = &[
    "help",
    "clear",
    "compact",
    "model",
    "new",
    "fork",
    "export",
    "sessions",
    "branch",
    "name",
    "copy",
    "usage",
    "reload",
    "diff",
    "extensions",
    "quit",
];

/// Metadata for a slash command registered by an extension.
#[derive(serde::Deserialize)]
pub struct CommandRegistration {
    pub name: String,
    pub description: String,
}

impl CommandRegistration {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("command name cannot be empty".into());
        }
        if self.name.contains(' ') || self.name.contains('\n') {
            return Err(format!(
                "command name cannot contain whitespace: {}",
                self.name
            ));
        }
        if RESERVED_COMMAND_NAMES.contains(&self.name.as_str()) {
            return Err(format!(
                "cannot register command with reserved name: {}",
                self.name
            ));
        }
        if self.description.is_empty() {
            return Err(format!(
                "command description cannot be empty: {}",
                self.name
            ));
        }
        Ok(())
    }
}

/// Key code — crossterm-independent representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum ExtKey {
    Char(char),
    F(u8),
    Enter,
    Escape,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
}

/// Modifier keys — crossterm-independent representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Deserialize)]
pub struct ExtModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// A key event — crossterm-independent representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize)]
pub struct ExtKeyEvent {
    pub key: ExtKey,
    pub modifiers: ExtModifiers,
}

/// Reserved shortcut combinations that extensions cannot override.
pub const RESERVED_SHORTCUTS: &[ExtKeyEvent] = &[
    ExtKeyEvent {
        key: ExtKey::Char('c'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
    ExtKeyEvent {
        key: ExtKey::Char('o'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
    ExtKeyEvent {
        key: ExtKey::Char('t'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
    ExtKeyEvent {
        key: ExtKey::Char('l'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
    ExtKeyEvent {
        key: ExtKey::Char('p'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
    ExtKeyEvent {
        key: ExtKey::Char('r'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
    ExtKeyEvent {
        key: ExtKey::Char('n'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
    ExtKeyEvent {
        key: ExtKey::Char('/'),
        modifiers: ExtModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    },
];

/// Metadata for a keyboard shortcut registered by an extension.
#[derive(serde::Deserialize)]
pub struct ShortcutRegistration {
    pub key: ExtKeyEvent,
    pub description: String,
}

impl ShortcutRegistration {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if RESERVED_SHORTCUTS.contains(&self.key) {
            return Err(format!(
                "cannot register shortcut with reserved key: {:?}",
                self.key
            ));
        }
        if self.description.is_empty() {
            return Err("shortcut description cannot be empty".into());
        }
        Ok(())
    }
}
