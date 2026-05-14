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
        let guide = tools
            .iter()
            .map(|t| format!("### {}\n{}\n\n", t.name, t.description))
            .collect::<String>();
        self.parts.push(format!("## 可用工具\n\n{guide}"));
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
        let section = skills
            .iter()
            .map(|(name, desc)| format!("- **{name}**: {desc}\n"))
            .collect::<String>();
        self.parts.push(format!("## 可用技能\n\n{section}"));
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
