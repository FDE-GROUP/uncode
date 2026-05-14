use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// 对代码进行关键词语法高亮
pub fn highlight_code(code: &str, language: &str) -> Vec<Line<'static>> {
    if code.is_empty() {
        return vec![];
    }
    let keywords = get_keywords(language);
    code.lines()
        .map(|line| highlight_line(line, &keywords))
        .collect()
}

fn highlight_line(line: &str, keywords: &[String]) -> Line<'static> {
    let mut spans = Vec::new();
    let words: Vec<&str> = line
        .split_inclusive(&[
            ' ', '(', ')', '{', '}', '[', ']', ':', ';', ',', '.', '<', '>', '!', '=', '+', '-',
            '*', '/', '"', '\'',
        ])
        .collect();

    for word in words {
        let trimmed = word.trim_end_matches(&[
            ' ', '(', ')', '{', '}', '[', ']', ':', ';', ',', '.', '<', '>', '!', '=', '+', '-',
            '*', '/',
        ]);
        if keywords.iter().any(|k| k == trimmed) {
            spans.push(Span::styled(
                word.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
        } else if trimmed.starts_with("//") || trimmed.starts_with('#') {
            spans.push(Span::styled(
                word.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ));
        } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
            spans.push(Span::styled(
                word.to_string(),
                Style::default().fg(Color::Green),
            ));
        } else {
            spans.push(Span::raw(word.to_string()));
        }
    }

    Line::from(spans)
}

fn get_keywords(language: &str) -> Vec<String> {
    match language {
        "rust" | "rs" => vec![
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "use", "mod", "async",
            "await", "if", "else", "match", "for", "while", "loop", "return", "self", "Self",
            "where", "type", "const", "static", "ref", "move", "unsafe", "true", "false", "Some",
            "None", "Ok", "Err", "Result", "Option", "Vec", "String", "&str", "bool", "i32", "u32",
            "u64", "usize", "Box", "Arc", "RwLock", "HashMap", "Clone", "Debug", "Default", "Send",
            "Sync", "Copy",
        ],
        "python" | "py" => vec![
            "def", "class", "import", "from", "if", "else", "elif", "for", "while", "return",
            "yield", "with", "as", "try", "except", "finally", "raise", "True", "False", "None",
            "self", "lambda", "pass", "break", "continue", "and", "or", "not", "in", "is",
        ],
        "typescript" | "ts" | "js" => vec![
            "function",
            "const",
            "let",
            "var",
            "class",
            "interface",
            "type",
            "enum",
            "export",
            "import",
            "from",
            "async",
            "await",
            "if",
            "else",
            "for",
            "while",
            "return",
            "throw",
            "try",
            "catch",
            "finally",
            "new",
            "this",
            "super",
            "true",
            "false",
            "null",
            "undefined",
            "typeof",
            "instanceof",
            "Promise",
            "string",
            "number",
            "boolean",
            "void",
            "any",
        ],
        "go" => vec![
            "func",
            "var",
            "const",
            "type",
            "struct",
            "interface",
            "package",
            "import",
            "if",
            "else",
            "for",
            "range",
            "return",
            "go",
            "defer",
            "chan",
            "select",
            "true",
            "false",
            "nil",
            "map",
            "string",
            "int",
            "bool",
            "error",
        ],
        "java" => vec![
            "class",
            "interface",
            "enum",
            "extends",
            "implements",
            "public",
            "private",
            "protected",
            "static",
            "final",
            "void",
            "int",
            "long",
            "boolean",
            "String",
            "if",
            "else",
            "for",
            "while",
            "return",
            "new",
            "this",
            "super",
            "try",
            "catch",
            "throw",
            "throws",
            "true",
            "false",
            "null",
            "import",
            "package",
        ],
        _ => vec![],
    }
    .into_iter()
    .map(String::from)
    .collect()
}

/// 根据文件扩展名检测编程语言
pub fn detect_language_from_path(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    match ext {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("typescript"),
        "py" => Some("python"),
        "go" => Some("go"),
        "java" => Some("java"),
        _ => None,
    }
}
