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
                    "/issues pull   — 拉取 GitHub Issues 列表",
                    "/think simple  — 切换到自然语言提炼视图",
                    "/think full    — 恢复完整技术视图",
                    "/simple        — 切换到简化双面板视图",
                    "/full          — 恢复到完整四面板视图",
                    "/unlock        — 解锁面板布局",
                    "/quit          — 退出",
                ];
                cmds.join("\n")
            }),
        );

        self.register("quit", "退出 uncode", Box::new(|_| "退出中...".into()));
    }

    pub fn register(&mut self, name: &str, description: &str, handler: CommandFn) {
        self.commands
            .insert(name.to_string(), (description.to_string(), handler));
    }

    pub fn execute(&self, input: &str) -> Option<String> {
        let input = input.trim_start_matches('/');
        let (name, args) = match input.split_once(' ') {
            Some((n, a)) => (n, a),
            None => (input, ""),
        };

        self.commands.get(name).map(|(_, handler)| handler(args))
    }

    pub fn names(&self) -> Vec<String> {
        self.commands.keys().cloned().collect()
    }
}

impl Default for SlashCommands {
    fn default() -> Self {
        Self::new()
    }
}
