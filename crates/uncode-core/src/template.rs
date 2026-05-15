use std::collections::HashMap;

/// Prompt 模板定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Template {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub variables: Vec<String>,
    #[serde(default)]
    pub system: String,
    pub prompt: String,
}

/// 模板存储：内置 + 用户自定义
pub struct TemplateStore {
    templates: HashMap<String, Template>,
}

impl TemplateStore {
    /// 加载内置模板 + 用户模板（~/.uncode/templates/）
    pub fn load() -> Self {
        let mut templates = HashMap::new();

        // 内置模板
        for t in builtins() {
            templates.insert(t.name.clone(), t);
        }

        // 用户模板：覆盖同名内置
        if let Some(dir) = dirs::config_dir() {
            let tpl_dir = dir.join("uncode").join("templates");
            if tpl_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&tpl_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                if let Ok(tpl) = toml::from_str::<Template>(&content) {
                                    templates.insert(tpl.name.clone(), tpl);
                                }
                            }
                        }
                    }
                }
            }
        }

        Self { templates }
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    pub fn list(&self) -> Vec<&Template> {
        let mut list: Vec<_> = self.templates.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// 渲染模板，替换 {{variable}} 占位符
    pub fn render(&self, name: &str, vars: &HashMap<String, String>) -> Option<String> {
        let tpl = self.templates.get(name)?;
        let mut result = tpl.prompt.clone();
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }
        Some(result)
    }

    /// 返回模板的系统 prompt（如果有）
    pub fn system_prompt(&self, name: &str) -> Option<&str> {
        self.templates.get(name).map(|t| t.system.as_str())
    }
}

impl Default for TemplateStore {
    fn default() -> Self {
        Self::load()
    }
}

fn builtins() -> Vec<Template> {
    vec![
        Template {
            name: "review".into(),
            description: "代码审查：检查安全性、性能和可维护性".into(),
            variables: vec!["language".into(), "focus".into()],
            system: "你是一位资深代码审查专家。".into(),
            prompt: "请对以下 {{language}} 代码进行审查。\n\n重点关注：{{focus}}\n\n审查维度：\n1. 安全性漏洞（OWASP Top 10）\n2. 性能问题\n3. 可维护性和代码规范\n4. 错误处理完整性\n\n请用中文回复，按严重程度排序。".into(),
        },
        Template {
            name: "refactor".into(),
            description: "重构建议：改善代码结构和可读性".into(),
            variables: vec!["language".into()],
            system: "你是一位重构专家。".into(),
            prompt: "请分析以下 {{language}} 代码，提出重构建议。\n\n关注：\n1. 设计模式改进\n2. 函数拆分和职责单一\n3. 命名和代码组织\n4. 消除重复代码\n\n请给出具体的重构方案和代码示例。".into(),
        },
        Template {
            name: "test".into(),
            description: "生成单元测试".into(),
            variables: vec!["language".into()],
            system: "你是一位测试工程师。".into(),
            prompt: "请为以下 {{language}} 代码生成单元测试。\n\n要求：\n1. 覆盖正常路径和边界条件\n2. 覆盖错误处理路径\n3. 使用该语言的主流测试框架\n4. 每个测试有清晰的描述\n\n请直接输出测试代码。".into(),
        },
        Template {
            name: "explain".into(),
            description: "代码解释：用易懂的方式解释代码逻辑".into(),
            variables: vec![],
            system: String::new(),
            prompt: "请解释以下代码的逻辑。用易懂的中文描述：\n1. 整体功能\n2. 关键算法或设计\n3. 潜在的问题或改进点\n\n用中文回复。".into(),
        },
        Template {
            name: "fix".into(),
            description: "Bug 修复：分析并修复代码中的问题".into(),
            variables: vec!["error".into()],
            system: "你是一位调试专家。".into(),
            prompt: "以下代码存在 bug：{{error}}\n\n请：\n1. 分析根因\n2. 提供修复方案\n3. 解释为什么修复有效\n\n请用中文回复，给出具体的修复代码。".into(),
        },
        Template {
            name: "document".into(),
            description: "生成代码文档".into(),
            variables: vec!["language".into()],
            system: String::new(),
            prompt: "请为以下 {{language}} 代码生成文档。\n\n包含：\n1. 模块/函数概述\n2. 参数说明\n3. 返回值说明\n4. 使用示例\n\n请用中文回复。".into(),
        },
    ]
}

/// 解析 CLI 的 --var 参数 "key=value" 格式
pub fn parse_vars(args: &[String]) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for arg in args {
        if let Some((key, value)) = arg.split_once('=') {
            vars.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_builtin_templates() {
        let store = TemplateStore::load();
        assert!(store.get("review").is_some());
        assert!(store.get("refactor").is_some());
        assert!(store.get("test").is_some());
        assert!(store.get("explain").is_some());
        assert!(store.get("fix").is_some());
        assert!(store.get("document").is_some());
    }

    #[test]
    fn test_list_sorted() {
        let store = TemplateStore::load();
        let list = store.list();
        let names: Vec<&str> = list.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["document", "explain", "fix", "refactor", "review", "test"]
        );
    }

    #[test]
    fn test_render_with_variables() {
        let store = TemplateStore::load();
        let mut vars = HashMap::new();
        vars.insert("language".into(), "Rust".into());
        vars.insert("focus".into(), "error handling".into());
        let result = store.render("review", &vars).unwrap();
        assert!(result.contains("Rust"));
        assert!(result.contains("error handling"));
        assert!(!result.contains("{{language}}"));
    }

    #[test]
    fn test_render_partial_vars() {
        let store = TemplateStore::load();
        let mut vars = HashMap::new();
        vars.insert("language".into(), "Rust".into());
        let result = store.render("review", &vars).unwrap();
        assert!(result.contains("Rust"));
        assert!(result.contains("{{focus}}"));
    }

    #[test]
    fn test_system_prompt() {
        let store = TemplateStore::load();
        assert_eq!(
            store.system_prompt("review"),
            Some("你是一位资深代码审查专家。")
        );
        assert_eq!(store.system_prompt("explain"), Some(""));
        assert_eq!(store.system_prompt("nonexistent"), None);
    }

    #[test]
    fn test_parse_vars() {
        let args = vec![
            "language=rust".to_string(),
            "focus=error handling".to_string(),
        ];
        let vars = parse_vars(&args);
        assert_eq!(vars.get("language"), Some(&"rust".to_string()));
        assert_eq!(vars.get("focus"), Some(&"error handling".to_string()));
    }

    #[test]
    fn test_parse_vars_no_equals() {
        let args = vec!["noequals".to_string()];
        let vars = parse_vars(&args);
        assert!(vars.is_empty());
    }

    #[test]
    fn test_render_nonexistent() {
        let store = TemplateStore::load();
        assert!(store.render("nonexistent", &HashMap::new()).is_none());
    }
}
