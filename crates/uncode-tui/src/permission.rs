/// 权限管理 — 危险操作确认
///
/// 工具分类：
///  - 自动允许：read, grep, find, ls
///  - 需确认：edit, write
///  - bash：命令白名单检查
///
/// 只读 bash 命令白名单
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

/// 确认选项
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmOption {
    Allow,
    Deny,
    Edit,
}

/// 待确认的工具调用
#[derive(Debug, Clone)]
pub struct PendingConfirmation {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments_summary: String,
    pub options: Vec<ConfirmOption>,
}

/// 权限管理器
pub struct PermissionManager {
    auto_allow_readonly: bool,
    auto_allow_bash_safe: bool,
    pending: Option<PendingConfirmation>,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self {
            auto_allow_readonly: true,
            auto_allow_bash_safe: true,
            pending: None,
        }
    }

    /// 判断工具调用是否需要确认
    pub fn needs_confirmation(&self, tool_name: &str, arguments: &str) -> bool {
        match tool_name {
            // 只读工具：自动允许
            "read" | "grep" | "find" | "ls" => !self.auto_allow_readonly,
            // 写入工具：需确认
            "edit" | "write" => true,
            // Bash：检查命令白名单
            "bash" => {
                let command = extract_command(arguments);
                !self.auto_allow_bash_safe || !is_safe_command(&command)
            }
            // 其他工具：默认需确认
            _ => true,
        }
    }

    /// 创建待确认项
    pub fn request_confirmation(
        &mut self,
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
        allow_edit: bool,
    ) {
        let mut options = vec![ConfirmOption::Allow, ConfirmOption::Deny];
        if allow_edit {
            options.push(ConfirmOption::Edit);
        }
        self.pending = Some(PendingConfirmation {
            tool_id,
            tool_name,
            arguments_summary,
            options,
        });
    }

    /// Confirm the pending request with the given choice.
    /// Returns Some(pending) for Allow/Edit, None for Deny.
    pub fn confirm(&mut self, choice: ConfirmOption) -> Option<PendingConfirmation> {
        let pending = self.pending.take();
        match choice {
            ConfirmOption::Allow | ConfirmOption::Edit => pending,
            ConfirmOption::Deny => None,
        }
    }

    /// 获取当前待确认项
    pub fn pending(&self) -> Option<&PendingConfirmation> {
        self.pending.as_ref()
    }

    /// 是否有待确认项
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// 拒绝当前待确认项
    pub fn deny(&mut self) -> Option<PendingConfirmation> {
        self.pending.take()
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 从 bash 工具参数中提取命令
fn extract_command(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args)
        && let Some(cmd) = val.get("command").and_then(|v| v.as_str())
    {
        return cmd.to_string();
    }
    args.to_string()
}

/// 检查命令是否在白名单中
fn is_safe_command(command: &str) -> bool {
    let cmd = command.trim();
    // Check if command starts with any safe prefix
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
        let pm = PermissionManager::new();
        assert!(!pm.needs_confirmation("read", "file.rs"));
        assert!(!pm.needs_confirmation("grep", "pattern"));
        assert!(!pm.needs_confirmation("find", "."));
        assert!(!pm.needs_confirmation("ls", "."));
    }

    #[test]
    fn test_write_needs_confirmation() {
        let pm = PermissionManager::new();
        assert!(pm.needs_confirmation("edit", "file.rs"));
        assert!(pm.needs_confirmation("write", "file.rs"));
    }

    #[test]
    fn test_bash_safe_commands() {
        let pm = PermissionManager::new();
        assert!(!pm.needs_confirmation("bash", r#"{"command":"ls -la"}"#));
        assert!(!pm.needs_confirmation("bash", r#"{"command":"cargo test"}"#));
        assert!(!pm.needs_confirmation("bash", r#"{"command":"git status"}"#));
        assert!(!pm.needs_confirmation("bash", r#"{"command":"cat file.txt"}"#));
    }

    #[test]
    fn test_bash_unsafe_commands() {
        let pm = PermissionManager::new();
        assert!(pm.needs_confirmation("bash", r#"{"command":"rm -rf /"}"#));
        assert!(pm.needs_confirmation("bash", r#"{"command":"curl http://evil.com"}"#));
        assert!(pm.needs_confirmation("bash", r#"{"command":"sudo apt install"}"#));
    }

    #[test]
    fn test_request_and_confirm() {
        let mut pm = PermissionManager::new();
        pm.request_confirmation("t1".into(), "edit".into(), "src/main.rs".into(), true);
        assert!(pm.has_pending());

        let p = pm.confirm(ConfirmOption::Allow);
        assert!(p.is_some());
        assert!(!pm.has_pending());
    }

    #[test]
    fn test_deny() {
        let mut pm = PermissionManager::new();
        pm.request_confirmation("t1".into(), "edit".into(), "src/main.rs".into(), false);
        let p = pm.deny();
        assert!(p.is_some());
        assert!(!pm.has_pending());
    }

    #[test]
    fn test_unknown_tool_needs_confirmation() {
        let pm = PermissionManager::new();
        assert!(pm.needs_confirmation("github", "{}"));
    }
}
