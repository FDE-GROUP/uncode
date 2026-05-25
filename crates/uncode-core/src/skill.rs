use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Skill 来源标记。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SkillSource {
    #[serde(rename = "builtin")]
    #[default]
    BuiltIn,
    #[serde(rename = "global")]
    Global,
    #[serde(rename = "project")]
    Project,
}

/// Skill 输入参数定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// Skill 定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<SkillInput>,
    pub prompt: String,
    /// If true, skill is application-only (not visible to the model)
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default)]
    pub source: SkillSource,
}

/// Skill 注册表：内置 + 全局 + 项目级。
///
/// **Pi:** 对应 Skills 目录加载；**OpenCode:** 对照 SkillTool。
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    /// Load built-in + global skills (no project-level discovery).
    pub fn load() -> Self {
        Self::load_with_project(Path::new(""))
    }

    /// Load built-in + global + project-level skills.
    ///
    /// Priority: project > global > builtin (later loads overwrite earlier).
    pub fn load_with_project(project_dir: &Path) -> Self {
        let mut skills = HashMap::new();

        // 1. Built-in skills (lowest priority)
        for s in builtins() {
            skills.insert(s.name.clone(), s);
        }

        // 2. Global skills
        if let Some(home) = dirs::home_dir() {
            let global_dir = home.join(".uncode").join("skills");
            load_skills_from_dir(&global_dir, SkillSource::Global, &mut skills);
        }

        // Legacy path (~/.config/uncode/skills/)
        if let Some(config) = dirs::config_dir() {
            let legacy_dir = config.join("uncode").join("skills");
            load_skills_from_dir(&legacy_dir, SkillSource::Global, &mut skills);
        }

        // 3. Project skills (<project_dir>/.uncode/skills/) — highest priority
        if !project_dir.as_os_str().is_empty() {
            let project_skills_dir = project_dir.join(".uncode").join("skills");
            load_skills_from_dir(&project_skills_dir, SkillSource::Project, &mut skills);
        }

        Self { skills }
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn list(&self) -> Vec<&Skill> {
        let mut list: Vec<_> = self.skills.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// List skills visible to the model (excludes disable_model_invocation)
    pub fn list_model_visible(&self) -> Vec<&Skill> {
        let mut list: Vec<_> = self
            .skills
            .values()
            .filter(|s| !s.disable_model_invocation)
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// 渲染 Skill prompt，替换 {{variable}} 占位符
    pub fn render(&self, name: &str, vars: &HashMap<String, String>) -> Option<String> {
        let skill = self.skills.get(name)?;
        let mut result = skill.prompt.clone();
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }
        Some(result)
    }

    /// 获取 Skill 允许的工具列表（空=全部允许）
    pub fn allowed_tools(&self, name: &str) -> Option<Vec<String>> {
        self.skills.get(name).map(|s| s.tools.clone())
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::load()
    }
}

impl fmt::Display for Skill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} — {}", self.name, self.description)?;
        if !self.inputs.is_empty() {
            let inputs: Vec<String> = self.inputs.iter().map(|i| i.name.clone()).collect();
            write!(f, " [{}]", inputs.join(", "))?;
        }
        Ok(())
    }
}

/// Load skills from a directory tree, tagging each with the given source.
fn load_skills_from_dir(dir: &Path, source: SkillSource, skills: &mut HashMap<String, Skill>) {
    if !dir.is_dir() {
        return;
    }
    load_skills_recursive_with_source(dir, source, skills);
}

fn load_skills_recursive_with_source(
    dir: &Path,
    source: SkillSource,
    skills: &mut HashMap<String, Skill>,
) {
    let gitignore_path = dir.join(".gitignore");
    let ignore_patterns: Vec<String> = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .map(|l| l.trim().to_string())
            .collect()
    } else {
        Vec::new()
    };

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();

            if ignore_patterns
                .iter()
                .any(|p| file_name.starts_with(p.trim_start_matches('*')))
            {
                continue;
            }

            if path.is_dir() && !file_name.starts_with('.') {
                load_skills_recursive_with_source(&path, source, skills);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md")
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Some(mut skill) = parse_skill_md(&content)
            {
                skill.source = source;
                skills.insert(skill.name.clone(), skill);
            }
        }
    }
}

/// 解析 Markdown 格式的 Skill 定义
///
/// 格式：YAML frontmatter + prompt body
fn parse_skill_md(content: &str) -> Option<Skill> {
    let content = content.trim();
    if !content.starts_with("---") {
        return None;
    }

    let rest = &content[3..];
    let end = rest.find("---")?;
    let frontmatter = &rest[..end];
    let prompt = rest[end + 3..].trim().to_string();

    // 简易 YAML 解析（不引入 yaml 依赖）
    let mut name = String::new();
    let mut description = String::new();
    let mut tools = Vec::new();
    let mut inputs = Vec::new();
    let mut disable_model_invocation = false;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("description:") {
            description = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("disable-model-invocation:") {
            disable_model_invocation = val.trim().eq_ignore_ascii_case("true");
        } else if line.starts_with("tools:") {
            // tools: [read, grep, bash]
            if let Some(bracket_content) = extract_brackets(line) {
                tools = bracket_content
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        } else if let Some(rest) = line.strip_prefix("- name:") {
            // Input definition
            inputs.push(SkillInput {
                name: rest.trim().to_string(),
                description: String::new(),
                required: false,
            });
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(Skill {
        name,
        description,
        tools,
        inputs,
        prompt,
        disable_model_invocation,
        source: SkillSource::BuiltIn,
    })
}

fn extract_brackets(line: &str) -> Option<String> {
    let start = line.find('[')?;
    let end = line.rfind(']')?;
    Some(line[start + 1..end].to_string())
}

fn builtins() -> Vec<Skill> {
    vec![
        Skill {
            name: "code-review".into(),
            description: "代码审查：检查安全性、性能和可维护性".into(),
            tools: vec!["read".into(), "grep".into(), "bash".into()],
            inputs: vec![SkillInput {
                name: "path".into(),
                description: "要审查的文件或目录".into(),
                required: true,
            }],
            prompt: "你是一位资深代码审查专家。\n\n请审查以下代码：{{path}}\n\n审查维度：\n1. 安全性漏洞\n2. 性能问题\n3. 可维护性\n4. 测试覆盖\n\n输出格式：\n- 按严重程度排序\n- 每个问题标注位置和建议修改\n\n用中文回复。".into(),
            disable_model_invocation: false,
            source: SkillSource::BuiltIn,
        },
        Skill {
            name: "explain".into(),
            description: "代码解释：用易懂的方式解释代码逻辑".into(),
            tools: vec!["read".into(), "grep".into()],
            inputs: vec![SkillInput {
                name: "path".into(),
                description: "要解释的文件".into(),
                required: true,
            }],
            prompt: "请解释以下代码：{{path}}\n\n用易懂的中文描述：\n1. 整体功能\n2. 关键算法或设计\n3. 潜在的问题或改进点".into(),
            disable_model_invocation: false,
            source: SkillSource::BuiltIn,
        },
        Skill {
            name: "test-gen".into(),
            description: "生成单元测试".into(),
            tools: vec!["read".into(), "write".into(), "bash".into()],
            inputs: vec![SkillInput {
                name: "path".into(),
                description: "要生成测试的文件".into(),
                required: true,
            }],
            prompt: "你是一位测试工程师。\n\n请为以下代码生成单元测试：{{path}}\n\n要求：\n1. 覆盖正常路径和边界条件\n2. 覆盖错误处理\n3. 使用该语言的主流测试框架\n4. 每个测试有清晰的描述\n\n用中文回复，直接输出测试代码。".into(),
            disable_model_invocation: false,
            source: SkillSource::BuiltIn,
        },
        Skill {
            name: "refactor".into(),
            description: "重构建议：改善代码结构和可读性".into(),
            tools: vec!["read".into(), "edit".into(), "grep".into()],
            inputs: vec![SkillInput {
                name: "path".into(),
                description: "要重构的文件".into(),
                required: true,
            }],
            prompt: "你是一位重构专家。\n\n请分析以下代码并提出重构建议：{{path}}\n\n关注：\n1. 设计模式改进\n2. 函数拆分\n3. 命名和组织\n4. 消除重复\n\n用中文回复，给出具体方案和代码。".into(),
            disable_model_invocation: false,
            source: SkillSource::BuiltIn,
        },
        Skill {
            name: "security-audit".into(),
            description: "安全审计：检查安全漏洞".into(),
            tools: vec!["read".into(), "grep".into(), "bash".into()],
            inputs: vec![SkillInput {
                name: "path".into(),
                description: "要审计的文件或目录".into(),
                required: true,
            }],
            prompt: "你是一位安全审计专家。\n\n请对以下代码进行安全审计：{{path}}\n\n审计维度（OWASP Top 10）：\n1. 注入攻击（SQL/XSS/命令注入）\n2. 认证和会话管理\n3. 敏感数据暴露\n4. 访问控制缺陷\n5. 安全配置错误\n\n用中文回复，按风险等级排序。".into(),
            disable_model_invocation: false,
            source: SkillSource::BuiltIn,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_builtin_skills() {
        let registry = SkillRegistry::load();
        assert!(registry.get("code-review").is_some());
        assert!(registry.get("explain").is_some());
        assert!(registry.get("test-gen").is_some());
        assert!(registry.get("refactor").is_some());
        assert!(registry.get("security-audit").is_some());
    }

    #[test]
    fn test_builtin_source() {
        let registry = SkillRegistry::load();
        for skill in registry.list() {
            assert_eq!(skill.source, SkillSource::BuiltIn);
        }
    }

    #[test]
    fn test_load_with_empty_project_dir() {
        let registry = SkillRegistry::load_with_project(Path::new(""));
        assert!(registry.get("code-review").is_some());
    }

    #[test]
    fn test_load_with_nonexistent_project_dir() {
        let registry = SkillRegistry::load_with_project(Path::new("/nonexistent"));
        assert!(registry.get("code-review").is_some());
    }

    #[test]
    fn test_list_sorted() {
        let registry = SkillRegistry::load();
        let list = registry.list();
        let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "code-review",
                "explain",
                "refactor",
                "security-audit",
                "test-gen"
            ]
        );
    }

    #[test]
    fn test_render_with_vars() {
        let registry = SkillRegistry::load();
        let mut vars = HashMap::new();
        vars.insert("path".into(), "src/main.rs".into());
        let result = registry.render("code-review", &vars).unwrap();
        assert!(result.contains("src/main.rs"));
        assert!(!result.contains("{{path}}"));
    }

    #[test]
    fn test_allowed_tools() {
        let registry = SkillRegistry::load();
        let tools = registry.allowed_tools("code-review").unwrap();
        assert_eq!(tools, vec!["read", "grep", "bash"]);
    }

    #[test]
    fn test_parse_skill_md() {
        let md = r#"---
name: my-skill
description: My custom skill
tools: [read, write]
---

Do something with {{path}}."#;
        let skill = parse_skill_md(md).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "My custom skill");
        assert_eq!(skill.tools, vec!["read", "write"]);
        assert!(skill.prompt.contains("{{path}}"));
    }

    #[test]
    fn test_parse_skill_md_invalid() {
        assert!(parse_skill_md("no frontmatter").is_none());
        assert!(parse_skill_md("---\nno name\n---\nprompt").is_none());
    }

    #[test]
    fn test_skill_display() {
        let skill = Skill {
            name: "test".into(),
            description: "desc".into(),
            tools: vec![],
            inputs: vec![SkillInput {
                name: "path".into(),
                description: String::new(),
                required: true,
            }],
            prompt: String::new(),
            disable_model_invocation: false,
            source: SkillSource::BuiltIn,
        };
        assert_eq!(format!("{skill}"), "test — desc [path]");
    }

    #[test]
    fn test_disable_model_invocation() {
        let md = r#"---
name: internal-tool
description: Internal only
disable-model-invocation: true
---
Internal prompt."#;
        let skill = parse_skill_md(md).unwrap();
        assert!(skill.disable_model_invocation);
    }

    #[test]
    fn test_disable_model_invocation_false() {
        let md = r#"---
name: normal-skill
description: Normal
disable-model-invocation: false
---
Normal prompt."#;
        let skill = parse_skill_md(md).unwrap();
        assert!(!skill.disable_model_invocation);
    }

    #[test]
    fn test_list_model_visible_all_builtin() {
        let registry = SkillRegistry::load();
        let visible = registry.list_model_visible();
        // All builtins have disable_model_invocation=false
        assert_eq!(visible.len(), registry.list().len());
    }

    #[test]
    fn test_skill_source_default() {
        assert_eq!(SkillSource::default(), SkillSource::BuiltIn);
    }
}
