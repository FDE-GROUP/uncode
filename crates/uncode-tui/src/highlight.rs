use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use tree_sitter::Parser;

use crate::theme::{SyntaxColors, Theme};

/// 对代码进行语法高亮（tree-sitter AST 级别）
pub fn highlight_code(code: &str, language: &str) -> Vec<Line<'static>> {
    highlight_code_with_theme(code, language, &Theme::default())
}

/// 对代码进行语法高亮，使用指定主题色
pub fn highlight_code_with_theme(code: &str, language: &str, theme: &Theme) -> Vec<Line<'static>> {
    if code.is_empty() {
        return vec![];
    }
    code.lines()
        .map(|line| highlight_line_with_theme(line, language, theme))
        .collect()
}

/// 高亮单行代码（带主题色）
pub fn highlight_line_with_theme(line: &str, language: &str, theme: &Theme) -> Line<'static> {
    if line.is_empty() {
        return Line::from("");
    }

    let highlights = collect_highlights(line, language);
    if highlights.is_empty() {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(theme.markdown.code_text),
        ));
    }

    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let mut pos = 0usize;
    let colors = &theme.syntax;

    // Sort highlights by start position
    let mut sorted: Vec<_> = highlights.iter().collect();
    sorted.sort_by_key(|h| h.start);

    for hl in sorted {
        // Gap before this highlight
        if pos < hl.start {
            let text = String::from_utf8_lossy(&bytes[pos..hl.start]).to_string();
            spans.push(Span::styled(
                text,
                Style::default().fg(theme.markdown.code_text),
            ));
        }
        let text = String::from_utf8_lossy(&bytes[hl.start..hl.end.min(line.len())]).to_string();
        let style = highlight_style(&hl.kind, colors);
        spans.push(Span::styled(text, style));
        pos = hl.end.min(line.len());
    }

    // Remaining text
    if pos < line.len() {
        let text = String::from_utf8_lossy(&bytes[pos..]).to_string();
        spans.push(Span::styled(
            text,
            Style::default().fg(theme.markdown.code_text),
        ));
    }

    if spans.is_empty() {
        Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(theme.markdown.code_text),
        ))
    } else {
        Line::from(spans)
    }
}

fn highlight_style(kind: &HighlightKind, colors: &SyntaxColors) -> Style {
    match kind {
        HighlightKind::Keyword => Style::default()
            .fg(colors.keyword)
            .add_modifier(Modifier::BOLD),
        HighlightKind::String => Style::default().fg(colors.string),
        HighlightKind::Comment => Style::default()
            .fg(colors.comment)
            .add_modifier(Modifier::ITALIC),
        HighlightKind::Number => Style::default().fg(colors.number),
        HighlightKind::Type => Style::default().fg(colors.type_name),
        HighlightKind::Function => Style::default().fg(colors.function_name),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HighlightKind {
    Keyword,
    String,
    Comment,
    Number,
    Type,
    Function,
}

struct Highlight {
    start: usize,
    end: usize,
    kind: HighlightKind,
}

fn collect_highlights(line: &str, language: &str) -> Vec<Highlight> {
    let lang = match language {
        "rust" | "rs" => tree_sitter_rust::LANGUAGE,
        "typescript" | "ts" | "tsx" | "js" | "jsx" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        "python" | "py" => tree_sitter_python::LANGUAGE,
        "go" => tree_sitter_go::LANGUAGE,
        "java" => tree_sitter_java::LANGUAGE,
        "c" | "h" => tree_sitter_c::LANGUAGE,
        "bash" | "sh" => tree_sitter_bash::LANGUAGE,
        "html" => tree_sitter_html::LANGUAGE,
        "css" => tree_sitter_css::LANGUAGE,
        "json" => tree_sitter_json::LANGUAGE,
        _ => return Vec::new(),
    };

    let mut parser = Parser::new();
    if parser.set_language(&lang.into()).is_err() {
        return Vec::new();
    }

    let tree = match parser.parse(line, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut highlights = Vec::new();
    collect_nodes(tree.root_node(), line, &mut highlights);
    highlights
}

fn collect_nodes(node: tree_sitter::Node, source: &str, highlights: &mut Vec<Highlight>) {
    let kind = node.kind();

    let hl_kind = match kind {
        // Keywords
        "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "use" | "mod"
        | "async" | "await" | "if" | "else" | "match" | "for" | "while" | "loop" | "return"
        | "self" | "Self" | "where" | "type" | "const" | "static" | "ref" | "move" | "unsafe"
        | "true" | "false" | "def" | "class" | "import" | "from" | "elif" | "yield" | "with"
        | "as" | "try" | "except" | "finally" | "raise" | "lambda" | "pass" | "break"
        | "continue" | "and" | "or" | "not" | "in" | "is" | "None" | "function" | "var"
        | "interface" | "export" | "new" | "this" | "super" | "throw" | "catch" | "typeof"
        | "instanceof" | "null" | "undefined" | "void" | "func" | "package" | "go" | "defer"
        | "chan" | "select" | "map" | "range" | "extends" | "implements" | "private"
        | "protected" | "throws" => Some(HighlightKind::Keyword),

        // Strings
        "string" | "string_literal" | "raw_string_literal" | "char_literal" => {
            Some(HighlightKind::String)
        }
        // Comments
        "comment" | "line_comment" | "block_comment" | "doc_comment" => {
            Some(HighlightKind::Comment)
        }
        // Numbers
        "integer_literal" | "float_literal" | "number" | "number_literal" => {
            Some(HighlightKind::Number)
        }
        // Types
        "type_identifier" | "primitive_type" | "generic_type" | "scoped_type_identifier" => {
            Some(HighlightKind::Type)
        }
        // Functions
        "function_item" | "call_expression" | "identifier" if is_function_context(&node) => {
            Some(HighlightKind::Function)
        }
        _ => None,
    };

    if let Some(kind) = hl_kind {
        let start = node.byte_range().start;
        let end = node.byte_range().end;
        if start < source.len() && end <= source.len() && end > start {
            highlights.push(Highlight { start, end, kind });
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_nodes(child, source, highlights);
    }
}

fn is_function_context(node: &tree_sitter::Node) -> bool {
    // Check if this identifier node is used as a function call or definition
    if let Some(parent) = node.parent() {
        let parent_kind = parent.kind();
        matches!(
            parent_kind,
            "function_item"
                | "call_expression"
                | "function_signature"
                | "method_definition"
                | "function_definition"
                | "arrow_function"
                | "function_declaration"
        )
    } else {
        false
    }
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
        "c" | "h" => Some("c"),
        "sh" | "bash" => Some("bash"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "json" => Some("json"),
        _ => None,
    }
}
