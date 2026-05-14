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
        let mut guide = String::from("## 可用工具\n\n");
        for tool in tools {
            guide.push_str(&format!("### {}\n{}\n\n", tool.name, tool.description));
        }
        self.parts.push(guide);
        self
    }

    pub fn add_context(mut self, text: &str) -> Self {
        if !text.is_empty() {
            self.parts.push(format!("## 项目上下文\n\n{text}"));
        }
        self
    }

    pub fn add_skills(mut self, skills: &[(String, String)]) -> Self {
        if skills.is_empty() {
            return self;
        }
        let mut section = String::from("## 可用技能\n\n");
        for (name, desc) in skills {
            section.push_str(&format!("- **{name}**: {desc}\n"));
        }
        self.parts.push(section);
        self
    }

    pub fn add_rules(mut self, rules: &str) -> Self {
        if !rules.is_empty() {
            self.parts.push(format!("## 开发规则\n\n{rules}"));
        }
        self
    }

    pub fn build(self) -> String {
        self.parts.join("\n\n")
    }
}
