//! Tool permission policy shared by TUI and `PermissionGate` hooks.

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

/// 检查命令是否在白名单中
pub fn is_safe_command(command: &str) -> bool {
    let cmd = command.trim();
    SAFE_COMMANDS.iter().any(|safe| {
        cmd == *safe
            || cmd.starts_with(&format!("{safe} "))
            || cmd.starts_with(&format!("{safe}\t"))
    })
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
}
