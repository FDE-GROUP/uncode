/// 权限管理 — 危险操作确认（UI 状态；阻塞逻辑在 `uncode_agent::PermissionGate`）。
///
/// 策略见 `uncode_agent::tool_permission`。

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
    /// Approval hint: bash uses model `description` when set, else registry tool description.
    pub tool_description: Option<String>,
    pub options: Vec<ConfirmOption>,
}

/// 工具执行前的人工确认（TUI 专有能力）。
///
/// **Pi:** Pi 哲学为「无内置权限弹窗」；uncode TUI 在 L3 层显式确认，非 Pi 机制复刻。
/// **OpenCode:** 对照工具审批/沙箱交互，无 API 兼容。
#[derive(Debug)]
pub struct PermissionManager {
    auto_allow_readonly: bool,
    auto_allow_bash_safe: bool,
    policy: Option<std::sync::Arc<uncode_agent::tool_permission::PermissionPolicy>>,
    pending: Option<PendingConfirmation>,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self {
            auto_allow_readonly: true,
            auto_allow_bash_safe: true,
            policy: None,
            pending: None,
        }
    }

    /// Set the configurable permission policy.
    pub fn set_policy(
        &mut self,
        policy: std::sync::Arc<uncode_agent::tool_permission::PermissionPolicy>,
    ) {
        self.policy = Some(policy);
    }

    /// 判断工具调用是否需要确认
    pub fn needs_confirmation(&self, tool_name: &str, arguments: &str) -> bool {
        if let Some(ref policy) = self.policy {
            policy.needs_confirmation(
                tool_name,
                arguments,
                self.auto_allow_readonly,
                self.auto_allow_bash_safe,
            )
        } else {
            uncode_agent::tool_permission::needs_confirmation(
                tool_name,
                arguments,
                self.auto_allow_readonly,
                self.auto_allow_bash_safe,
                None,
            )
        }
    }

    /// 创建待确认项
    pub fn request_confirmation(
        &mut self,
        tool_id: String,
        tool_name: String,
        arguments_summary: String,
        tool_description: Option<String>,
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
            tool_description,
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
        pm.request_confirmation("t1".into(), "edit".into(), "src/main.rs".into(), None, true);
        assert!(pm.has_pending());

        let p = pm.confirm(ConfirmOption::Allow);
        assert!(p.is_some());
        assert!(!pm.has_pending());
    }

    #[test]
    fn test_deny() {
        let mut pm = PermissionManager::new();
        pm.request_confirmation(
            "t1".into(),
            "edit".into(),
            "src/main.rs".into(),
            None,
            false,
        );
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
