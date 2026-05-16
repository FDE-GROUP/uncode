use uncode_core::tool::ToolDefinition;

#[derive(Default)]
pub struct SystemPromptBuilder {
    parts: Vec<String>,
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn base(mut self, text: impl Into<String>) -> Self {
        self.parts.push(text.into());
        self
    }

    pub fn add_tool_guide(mut self, tools: &[ToolDefinition]) -> Self {
        if tools.is_empty() {
            return self;
        }
        let mut guide = String::with_capacity(tools.len() * 128);
        guide.push_str("## 可用工具\n\n");
        for t in tools {
            guide.push_str("### ");
            guide.push_str(&t.name);
            guide.push('\n');
            guide.push_str(&t.description);
            guide.push_str("\n\n");
        }
        self.parts.push(guide);
        self
    }

    pub fn add_context(mut self, text: &str) -> Self {
        if !text.is_empty() {
            let mut s = String::with_capacity(16 + text.len());
            s.push_str("## 项目上下文\n\n");
            s.push_str(text);
            self.parts.push(s);
        }
        self
    }

    pub fn add_working_dir(mut self, dir: &std::path::Path) -> Self {
        let path = dir.to_string_lossy();
        let mut s = String::with_capacity(64 + path.len());
        s.push_str("## 工作目录\n\n");
        s.push_str(&path);
        s.push_str("\n\n所有文件路径应相对于此目录，或使用绝对路径。");
        self.parts.push(s);
        self
    }

    pub fn add_skills(mut self, skills: &[(String, String)]) -> Self {
        if skills.is_empty() {
            return self;
        }
        let mut section = String::with_capacity(skills.len() * 64);
        section.push_str("## 可用技能\n\n");
        for (name, desc) in skills {
            section.push_str("- **");
            section.push_str(name);
            section.push_str("**: ");
            section.push_str(desc);
            section.push('\n');
        }
        self.parts.push(section);
        self
    }

    pub fn add_rules(mut self, rules: &str) -> Self {
        if !rules.is_empty() {
            let mut s = String::with_capacity(16 + rules.len());
            s.push_str("## 开发规则\n\n");
            s.push_str(rules);
            self.parts.push(s);
        }
        self
    }

    pub fn build(self) -> String {
        self.parts.join("\n\n")
    }
}
