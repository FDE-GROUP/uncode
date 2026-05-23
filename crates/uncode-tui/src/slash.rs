use std::collections::HashMap;

pub type CommandFn = Box<dyn Fn(&str) -> String + Send + Sync>;

pub struct SlashCommands {
    commands: HashMap<String, (String, CommandFn)>,
}

impl SlashCommands {
    pub fn new() -> Self {
        let mut cmds = Self {
            commands: HashMap::new(),
        };
        cmds.register_defaults();
        cmds
    }

    fn register_defaults(&mut self) {
        self.register(
            "help",
            "显示可用命令",
            Box::new(|_| {
                let cmds = [
                    "/help          — 显示此帮助",
                    "/clear         — 清空对话区",
                    "/compact       — 查看上下文使用情况",
                    "/model [name]  — 切换模型（无参数弹出选择器）",
                    "/new           — 创建新会话",
                    "/fork [id]     — 从当前/指定会话创建分支",
                    "/export [fmt]  — 导出会话（html/jsonl）",
                    "/sessions      — 列出历史会话",
                    "/branch        — 显示分支信息",
                    "/name [title]  — 设置/查看会话标题",
                    "/copy          — 复制最后回复到剪贴板",
                    "/usage         — Token 用量统计",
                    "/reload        — 重新加载配置",
                    "/diff          — 显示工作区变更",
                    "/extensions    — 管理扩展 (list|reload|disable|enable)",
                    "/quit          — 退出",
                ];
                cmds.join("\n")
            }),
        );
    }

    pub fn register(&mut self, name: &str, description: &str, handler: CommandFn) {
        self.commands
            .insert(name.to_string(), (description.to_string(), handler));
    }

    pub fn execute(&self, input: &str) -> Option<String> {
        let input = input.trim_start_matches('/');
        let (name, args) = input.split_once(' ').unwrap_or((input, ""));

        self.commands.get(name).map(|(_, handler)| handler(args))
    }

    pub fn names(&self) -> Vec<String> {
        self.commands.keys().cloned().collect()
    }

    pub fn unregister(&mut self, name: &str) -> bool {
        self.commands.remove(name).is_some()
    }
}

impl Default for SlashCommands {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_help() {
        let cmds = SlashCommands::new();
        let result = cmds.execute("/help");
        assert!(result.is_some());
        let help = result.unwrap();
        assert!(help.contains("/help"));
        assert!(help.contains("/quit"));
    }

    #[test]
    fn test_execute_quit() {
        let cmds = SlashCommands::new();
        let result = cmds.execute("/quit");
        // quit is handled by TUI engine directly, not via slash handler
        assert!(result.is_none());
    }

    #[test]
    fn test_execute_unknown() {
        let cmds = SlashCommands::new();
        let result = cmds.execute("/nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_execute_with_args() {
        let mut cmds = SlashCommands::new();
        cmds.register(
            "echo",
            "echo args",
            Box::new(|args| format!("echo: {args}")),
        );
        let result = cmds.execute("/echo hello world");
        assert_eq!(result.unwrap(), "echo: hello world");
    }

    #[test]
    fn test_names() {
        let cmds = SlashCommands::new();
        let names = cmds.names();
        assert!(names.contains(&"help".to_string()));
    }

    #[test]
    fn test_execute_without_slash() {
        let cmds = SlashCommands::new();
        let result = cmds.execute("help");
        assert!(result.is_some());
    }
}
