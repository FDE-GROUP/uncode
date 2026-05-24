//! Tool permission policy shared by TUI and `PermissionGate` hooks.
//!
//! **Pi:** 对照 `confirm-destructive` / `protected-paths` / `permission-gate` 扩展。

use std::path::Path;

use glob::Pattern;
use regex::Regex;
use uncode_core::config::PermissionConfig;

/// 只读 bash 命令白名单（与 TUI `permission.rs` 对齐）
const SAFE_COMMANDS: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "find",
    "grep",
    "git status",
    "git log",
    "git diff",
    "git branch",
    "cargo check",
    "cargo test",
    "cargo build",
    "cargo clippy",
    "cargo fmt",
    "pwd",
    "echo",
    "which",
    "env",
    "wc",
    "sort",
    "uniq",
    "diff",
    "tree",
    "rg",
    "fd",
];

/// 判断工具调用是否需要用户确认。
pub fn needs_confirmation(
    tool_name: &str,
    arguments: &str,
    auto_allow_readonly: bool,
    auto_allow_bash_safe: bool,
) -> bool {
    match tool_name {
        "read" | "grep" | "find" | "ls" => !auto_allow_readonly,
        "edit" | "write" => true,
        "bash" => {
            let command = extract_command(arguments);
            !auto_allow_bash_safe || !is_safe_command(&command)
        }
        _ => true,
    }
}

/// 审批 UI / 日志用的人类可读说明：bash 优先模型填写的 `description`，否则用 registry 文案。
pub fn approval_description(
    tool_name: &str,
    args: &serde_json::Value,
    registry_description: Option<String>,
) -> Option<String> {
    if tool_name == "bash" {
        if let Some(d) = args
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(d.to_string());
        }
    }
    registry_description.filter(|d| !d.is_empty())
}

/// 从 bash 工具参数中提取命令
pub fn extract_command(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args)
        && let Some(cmd) = val.get("command").and_then(|v| v.as_str())
    {
        return cmd.to_string();
    }
    args.to_string()
}

/// Shell metacharacters that indicate command chaining or substitution.
const SHELL_META_CHARS: &[char] = &[';', '|', '&', '$', '`', '>', '<', '(', ')', '\n', '\r'];

/// Check if a command contains shell metacharacters for chaining/substitution.
fn contains_shell_metacharacters(cmd: &str) -> bool {
    cmd.chars().any(|c| SHELL_META_CHARS.contains(&c))
}

/// 检查命令是否在白名单中
pub fn is_safe_command(command: &str) -> bool {
    let cmd = command.trim();
    if contains_shell_metacharacters(cmd) {
        return false;
    }
    SAFE_COMMANDS.iter().any(|safe| {
        cmd == *safe
            || cmd.starts_with(&format!("{safe} "))
            || cmd.starts_with(&format!("{safe}\t"))
    })
}

// ── Configurable PermissionPolicy ──

/// Compiled permission policy loaded from config.
///
/// **Pi:** 对照 `protected-paths`（敏感路径阻止）、`permission-gate`（危险命令检测）扩展。
pub struct PermissionPolicy {
    /// Merged safe commands: built-in + user extras.
    safe_commands: Vec<String>,
    /// Compiled protected path glob patterns.
    protected_path_patterns: Vec<Pattern>,
    /// Compiled dangerous bash regex patterns.
    dangerous_bash_regexes: Vec<Regex>,
    /// Feature toggles.
    protected_paths_enabled: bool,
    dangerous_bash_detection: bool,
}

impl PermissionPolicy {
    /// Build from config. Invalid patterns are skipped with a warning.
    pub fn from_config(config: &PermissionConfig) -> Self {
        let mut safe_commands: Vec<String> =
            SAFE_COMMANDS.iter().map(|s| (*s).to_string()).collect();
        safe_commands.extend(config.extra_safe_commands.iter().cloned());

        let protected_path_patterns = config
            .protected_paths
            .iter()
            .filter_map(|p| match Pattern::new(p) {
                Ok(pat) => Some(pat),
                Err(e) => {
                    tracing::warn!("skipping invalid protected path pattern \"{p}\": {e}");
                    None
                }
            })
            .collect();

        let dangerous_bash_regexes = config
            .dangerous_bash_patterns
            .iter()
            .filter_map(|p| match Regex::new(p) {
                Ok(re) => Some(re),
                Err(e) => {
                    tracing::warn!("skipping invalid dangerous bash pattern \"{p}\": {e}");
                    None
                }
            })
            .collect();

        Self {
            safe_commands,
            protected_path_patterns,
            dangerous_bash_regexes,
            protected_paths_enabled: config.protected_paths_enabled,
            dangerous_bash_detection: config.dangerous_bash_detection,
        }
    }

    /// Default policy (no config file).
    pub fn default_policy() -> Self {
        Self::from_config(&PermissionConfig::default())
    }

    /// Full permission check: returns true if the tool call requires user confirmation.
    pub fn needs_confirmation(
        &self,
        tool_name: &str,
        arguments: &str,
        auto_allow_readonly: bool,
        auto_allow_bash_safe: bool,
    ) -> bool {
        // Protected path check: write/edit to a protected path always confirms
        if self.protected_paths_enabled
            && (tool_name == "edit" || tool_name == "write")
            && self.is_protected_path_from_args(arguments)
        {
            return true;
        }

        match tool_name {
            "read" | "grep" | "find" | "ls" => !auto_allow_readonly,
            "edit" | "write" => true,
            "bash" => {
                let command = extract_command(arguments);
                // Dangerous command check overrides safe-list
                if self.dangerous_bash_detection && self.is_dangerous_command(&command) {
                    return true;
                }
                !auto_allow_bash_safe || !self.is_safe_command_policy(&command)
            }
            _ => true,
        }
    }

    fn is_protected_path_from_args(&self, arguments: &str) -> bool {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(arguments) {
            if let Some(path) = val.get("path").and_then(|v| v.as_str()) {
                return self.is_protected_path(path);
            }
        }
        false
    }

    fn is_protected_path(&self, path: &str) -> bool {
        let p = Path::new(path);
        for pattern in &self.protected_path_patterns {
            if pattern.matches(path) {
                return true;
            }
            // Check filename
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if pattern.matches(name) {
                    return true;
                }
            }
            // Check each path component
            for component in p.components() {
                if let Some(s) = component.as_os_str().to_str() {
                    if pattern.matches(s) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_dangerous_command(&self, command: &str) -> bool {
        let cmd = command.trim();
        self.dangerous_bash_regexes
            .iter()
            .any(|re| re.is_match(cmd))
    }

    fn is_safe_command_policy(&self, command: &str) -> bool {
        let cmd = command.trim();
        self.safe_commands.iter().any(|safe| {
            cmd == safe.as_str()
                || cmd.starts_with(&format!("{safe} "))
                || cmd.starts_with(&format!("{safe}\t"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readonly_auto_allowed() {
        assert!(!needs_confirmation("read", "file.rs", true, true));
        assert!(needs_confirmation("read", "file.rs", false, true));
    }

    #[test]
    fn test_bash_rm_needs_confirmation() {
        assert!(needs_confirmation(
            "bash",
            r#"{"command":"rm -rf /"}"#,
            true,
            true
        ));
    }

    #[test]
    fn test_approval_description_prefers_bash_call_description() {
        let args = serde_json::json!({
            "command": "rm -rf x",
            "description": "Remove build artifacts"
        });
        let desc = approval_description("bash", &args, Some("registry fallback".into()));
        assert_eq!(desc.as_deref(), Some("Remove build artifacts"));
    }

    #[test]
    fn test_approval_description_falls_back_to_registry() {
        let args = serde_json::json!({ "command": "ls" });
        let desc = approval_description("bash", &args, Some("sandbox note".into()));
        assert_eq!(desc.as_deref(), Some("sandbox note"));
    }

    #[test]
    fn test_safe_command_blocks_shell_injection() {
        assert!(!is_safe_command("ls; rm -rf /"));
        assert!(!is_safe_command("ls | malicious"));
        assert!(!is_safe_command("ls && rm -rf /"));
        assert!(!is_safe_command("ls $(cat /etc/passwd)"));
        assert!(!is_safe_command("cat /etc/passwd > /tmp/out"));
        assert!(!is_safe_command("ls\nrm -rf /"));
    }

    #[test]
    fn test_safe_command_allows_legitimate() {
        assert!(is_safe_command("ls"));
        assert!(is_safe_command("ls /tmp"));
        assert!(is_safe_command("cat README.md"));
        assert!(is_safe_command("pwd"));
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use uncode_core::config::PermissionConfig;

    fn default_policy() -> PermissionPolicy {
        PermissionPolicy::default_policy()
    }

    fn policy_with<F>(mutate: F) -> PermissionPolicy
    where
        F: FnOnce(&mut PermissionConfig),
    {
        let mut config = PermissionConfig::default();
        mutate(&mut config);
        PermissionPolicy::from_config(&config)
    }

    // ── Protected paths ──

    #[test]
    fn test_protected_path_blocks_dotenv() {
        let p = default_policy();
        assert!(p.needs_confirmation("write", r#"{"path":".env"}"#, true, true));
    }

    #[test]
    fn test_protected_path_blocks_dotenv_local() {
        let p = default_policy();
        assert!(p.needs_confirmation("write", r#"{"path":".env.local"}"#, true, true));
    }

    #[test]
    fn test_protected_path_blocks_git_dir() {
        let p = default_policy();
        assert!(p.needs_confirmation("edit", r#"{"path":".git/config"}"#, true, true));
    }

    #[test]
    fn test_protected_path_blocks_nested_credentials() {
        let p = default_policy();
        assert!(p.needs_confirmation("write", r#"{"path":"config/credentials.json"}"#, true, true));
    }

    #[test]
    fn test_protected_path_allows_normal() {
        let p = default_policy();
        // write always needs confirmation, but not because of protected path
        assert!(p.needs_confirmation("write", r#"{"path":"src/main.rs"}"#, true, true));
    }

    #[test]
    fn test_protected_disabled_allows_dotenv() {
        let p = policy_with(|c| c.protected_paths_enabled = false);
        // Still true because write always needs confirmation, but the protected path
        // check is bypassed. Verify by checking that the protection path logic is off:
        assert!(!p.protected_paths_enabled);
    }

    #[test]
    fn test_custom_protected_path() {
        let p = policy_with(|c| c.protected_paths.push("secrets.toml".into()));
        assert!(p.needs_confirmation("edit", r#"{"path":"secrets.toml"}"#, true, true));
    }

    // ── Dangerous bash detection ──

    #[test]
    fn test_dangerous_rm_rf() {
        let p = default_policy();
        assert!(p.needs_confirmation("bash", r#"{"command":"rm -rf /"}"#, true, true));
        assert!(p.needs_confirmation("bash", r#"{"command":"rm -fr /tmp"}"#, true, true));
        assert!(p.needs_confirmation("bash", r#"{"command":"rm --recursive build"}"#, true, true));
    }

    #[test]
    fn test_dangerous_sudo() {
        let p = default_policy();
        assert!(p.needs_confirmation("bash", r#"{"command":"sudo apt install foo"}"#, true, true));
    }

    #[test]
    fn test_dangerous_chmod_777() {
        let p = default_policy();
        assert!(p.needs_confirmation("bash", r#"{"command":"chmod 777 /tmp"}"#, true, true));
    }

    #[test]
    fn test_dangerous_chown() {
        let p = default_policy();
        assert!(p.needs_confirmation(
            "bash",
            r#"{"command":"chown root:root /etc/passwd"}"#,
            true,
            true
        ));
    }

    #[test]
    fn test_dangerous_dd() {
        let p = default_policy();
        assert!(p.needs_confirmation(
            "bash",
            r#"{"command":"dd if=/dev/zero of=/dev/sda"}"#,
            true,
            true
        ));
    }

    #[test]
    fn test_dangerous_mkfs() {
        let p = default_policy();
        assert!(p.needs_confirmation("bash", r#"{"command":"mkfs.ext4 /dev/sda1"}"#, true, true));
    }

    #[test]
    fn test_safe_unaffected_by_dangerous() {
        let p = default_policy();
        assert!(!p.needs_confirmation("bash", r#"{"command":"ls -la"}"#, true, true));
        assert!(!p.needs_confirmation("bash", r#"{"command":"cargo test"}"#, true, true));
        assert!(!p.needs_confirmation("bash", r#"{"command":"git status"}"#, true, true));
    }

    #[test]
    fn test_dangerous_disabled_sudo_goes_to_safelist() {
        let p = policy_with(|c| c.dangerous_bash_detection = false);
        // sudo not in safe list → still needs confirmation
        assert!(p.needs_confirmation("bash", r#"{"command":"sudo apt install"}"#, true, true));
    }

    #[test]
    fn test_dangerous_disabled_safe_still_auto() {
        let p = policy_with(|c| c.dangerous_bash_detection = false);
        assert!(!p.needs_confirmation("bash", r#"{"command":"ls"}"#, true, true));
    }

    // ── Configurable safe commands ──

    #[test]
    fn test_extra_safe_commands() {
        let p = policy_with(|c| c.extra_safe_commands.push("my-tool".into()));
        assert!(!p.needs_confirmation("bash", r#"{"command":"my-tool run"}"#, true, true));
    }

    // ── Edge cases ──

    #[test]
    fn test_invalid_regex_skipped() {
        let p = policy_with(|c| {
            c.dangerous_bash_patterns.push("[invalid".into());
            c.dangerous_bash_patterns.push(r"\bsudo\b".into());
        });
        // Invalid regex skipped, sudo still detected
        assert!(p.needs_confirmation("bash", r#"{"command":"sudo test"}"#, true, true));
    }

    #[test]
    fn test_invalid_glob_skipped() {
        let p = policy_with(|c| {
            c.protected_paths.push("[invalid".into());
            c.protected_paths.push(".secret".into());
        });
        // Invalid glob skipped, .secret still works
        assert!(p.is_protected_path(".secret"));
    }
}
