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

    pub fn append(mut self, text: &str) -> Self {
        if !text.is_empty() {
            self.parts.push(text.to_string());
        }
        self
    }

    pub fn build(self) -> String {
        self.parts.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn read_tool_def() -> ToolDefinition {
        ToolDefinition {
            name: "read".into(),
            description: "read file".into(),
            parameters: serde_json::json!({}),
            label: None,
            execution_mode: Default::default(),
        }
    }

    #[test]
    fn test_empty_builder() {
        let s = SystemPromptBuilder::new().build();
        assert_eq!(s, "");
    }

    #[test]
    fn test_base_appends_text() {
        let s = SystemPromptBuilder::new().base("Hello").build();
        assert_eq!(s, "Hello");
    }

    #[test]
    fn test_add_tool_guide() {
        let td = read_tool_def();
        let s = SystemPromptBuilder::new().add_tool_guide(&[td]).build();
        assert!(s.contains("## 可用工具"));
        assert!(s.contains("read"));
        assert!(s.contains("read file"));
    }

    #[test]
    fn test_add_context() {
        let s = SystemPromptBuilder::new().add_context("AGENTS.md").build();
        assert!(s.contains("## 项目上下文"));
        assert!(s.contains("AGENTS.md"));
    }

    #[test]
    fn test_add_context_empty_skips() {
        let s = SystemPromptBuilder::new().add_context("").build();
        assert!(!s.contains("项目上下文"));
    }

    #[test]
    fn test_add_working_dir() {
        let s = SystemPromptBuilder::new()
            .add_working_dir(Path::new("/tmp"))
            .build();
        assert!(s.contains("## 工作目录"));
        assert!(s.contains("/tmp"));
    }

    #[test]
    fn test_add_rules() {
        let s = SystemPromptBuilder::new().add_rules("no unsafe").build();
        assert!(s.contains("## 开发规则"));
        assert!(s.contains("no unsafe"));
    }

    #[test]
    fn test_add_rules_empty_skips() {
        let s = SystemPromptBuilder::new().add_rules("").build();
        assert!(!s.contains("开发规则"));
    }

    #[test]
    fn test_chained_build() {
        let s = SystemPromptBuilder::new()
            .base("a")
            .append("b")
            .append("c")
            .build();
        assert_eq!(s, "a\n\nb\n\nc");
    }

    #[test]
    fn test_append_empty_skips() {
        let s = SystemPromptBuilder::new().append("").build();
        assert_eq!(s, "");
    }
}
