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

    /// 渲染模板，支持 Shell 风格位置参数：
    /// - `$1`, `$2`, ... → 位置参数
    /// - `$@` / `$ARGUMENTS` → 全部参数
    /// - `${@:N}` → 从第 N 个参数开始
    /// - `${@:N:L}` → 从第 N 个参数开始，取 L 个
    pub fn render_with_positional_args(&self, name: &str, args: &[&str]) -> Option<String> {
        let tpl = self.templates.get(name)?;
        let mut result = tpl.prompt.clone();

        // Replace $1, $2, ... positional args
        for (i, arg) in args.iter().enumerate() {
            let idx = i + 1; // 1-indexed
            result = result.replace(&format!("${idx}"), arg);
        }

        // Replace $@ and $ARGUMENTS with all args joined
        let all_args = args.join(" ");
        result = result.replace("$@", &all_args);
        result = result.replace("$ARGUMENTS", &all_args);

        // Replace ${@:N} and ${@:N:L}
        result = replace_positional_slices(&result, args);

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

/// Replace `${@:N}` and `${@:N:L}` patterns with sliced args (1-indexed).
fn replace_positional_slices(template: &str, args: &[&str]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut i = 0;
    let bytes = template.as_bytes();

    while i < bytes.len() {
        if template[i..].starts_with("${@:") {
            let rest = &template[i + 4..];
            if let Some(close) = rest.find('}') {
                let inner = &rest[..close];
                let slice = parse_slice_args(inner, args);
                result.push_str(&slice);
                i += 4 + close + 1; // skip ${@:...}
                continue;
            }
        }
        // Copy one char (handle multi-byte UTF-8)
        let Some(c) = template[i..].chars().next() else {
            break;
        };
        result.push(c);
        i += c.len_utf8();
    }

    result
}

/// Parse `N` or `N:L` from slice pattern, return joined args.
fn parse_slice_args(inner: &str, args: &[&str]) -> String {
    let (start_str, limit_str) = if let Some((s, l)) = inner.split_once(':') {
        (s, Some(l))
    } else {
        (inner, None)
    };

    let start: usize = start_str
        .trim()
        .parse::<usize>()
        .unwrap_or(1)
        .saturating_sub(1);
    let limit: Option<usize> = limit_str.and_then(|l| l.trim().parse().ok());

    let slice: Vec<&str> = if let Some(limit) = limit {
        args.iter().skip(start).take(limit).copied().collect()
    } else {
        args.iter().skip(start).copied().collect()
    };
    slice.join(" ")
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

    // ── render_with_positional_args tests ──

    #[test]
    fn test_positional_args_dollar_1() {
        let result = replace_positional_slices(
            "file is $1, done",
            &["src/main.rs".as_ref(), "yes".as_ref()],
        );
        // $1 replacement happens in render_with_positional_args, not in replace_positional_slices
        // replace_positional_slices only handles ${@:N} patterns
        assert_eq!(result, "file is $1, done");
    }

    #[test]
    fn test_positional_args_slice_from() {
        let args = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let result = replace_positional_slices(
            "tail: ${@:2}",
            &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );
        assert_eq!(result, "tail: b c");
    }

    #[test]
    fn test_positional_args_slice_with_limit() {
        let args = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let result = replace_positional_slices(
            "mid: ${@:2:2}",
            &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );
        assert_eq!(result, "mid: b c");
    }

    #[test]
    fn test_positional_args_all() {
        let args = vec!["x".to_string(), "y".to_string()];
        let result = replace_positional_slices(
            "all: ${@:1}",
            &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );
        assert_eq!(result, "all: x y");
    }

    #[test]
    fn test_positional_args_no_match() {
        let result = replace_positional_slices("no patterns here", &[]);
        assert_eq!(result, "no patterns here");
    }

    #[test]
    fn test_render_with_positional_args_full() {
        let store = TemplateStore::load();
        // explain template has no $N/$@ placeholders, but the function should still return content
        let result = store
            .render_with_positional_args("explain", &["src/main.rs"])
            .unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_with_positional_args_dollar_at() {
        let store = TemplateStore::load();
        // explain template has no $@, function returns content as-is
        let result = store
            .render_with_positional_args("explain", &["arg1", "arg2", "arg3"])
            .unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_with_positional_args_nonexistent() {
        let store = TemplateStore::load();
        assert!(
            store
                .render_with_positional_args("nonexistent", &["a"])
                .is_none()
        );
    }
}
