use std::collections::HashMap;
use std::sync::LazyLock;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use tree_sitter::Parser;

use crate::theme::{SyntaxColors, Theme};

// ---------------------------------------------------------------------------
// Language alias normalization (#186)
// ---------------------------------------------------------------------------

/// Alias → canonical language name (only for languages with tree-sitter grammars).
static LANGUAGE_ALIASES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Rust
    m.insert("rs", "rust");
    // JavaScript / TypeScript
    m.insert("js", "javascript");
    m.insert("jsx", "javascript");
    m.insert("ts", "typescript");
    m.insert("tsx", "typescript");
    m.insert("typescriptreact", "typescript");
    m.insert("javascriptreact", "javascript");
    m.insert("mjs", "javascript");
    m.insert("cjs", "javascript");
    // Python
    m.insert("py", "python");
    m.insert("py3", "python");
    m.insert("python3", "python");
    // Go
    m.insert("golang", "go");
    // C / C++
    m.insert("h", "c");
    // Shell
    m.insert("sh", "bash");
    m.insert("shell", "bash");
    m.insert("zsh", "bash");
    m.insert("ksh", "bash");
    // HTML
    m.insert("htm", "html");
    // CSS
    m.insert("scss", "css");
    m.insert("sass", "css");
    m.insert("less", "css");
    // JSON
    m.insert("jsonl", "json");
    m.insert("jsonc", "json");
    m
});

/// Canonical language names that map to tree-sitter grammars we ship.
static TREE_SITTER_LANGUAGES: LazyLock<HashMap<&'static str, tree_sitter::Language>> =
    LazyLock::new(|| {
        let mut m: HashMap<&str, tree_sitter::Language> = HashMap::new();
        m.insert("rust", tree_sitter_rust::LANGUAGE.into());
        m.insert(
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );
        m.insert(
            "javascript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );
        m.insert("python", tree_sitter_python::LANGUAGE.into());
        m.insert("go", tree_sitter_go::LANGUAGE.into());
        m.insert("java", tree_sitter_java::LANGUAGE.into());
        m.insert("c", tree_sitter_c::LANGUAGE.into());
        m.insert("bash", tree_sitter_bash::LANGUAGE.into());
        m.insert("html", tree_sitter_html::LANGUAGE.into());
        m.insert("css", tree_sitter_css::LANGUAGE.into());
        m.insert("json", tree_sitter_json::LANGUAGE.into());
        m
    });

/// Normalize a raw language token from a markdown fence to a canonical name
/// that exists in `TREE_SITTER_LANGUAGES`, or `None`.
fn normalize_language(lang: &str) -> Option<&'static str> {
    let key = lang.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    // Try direct match against canonical names
    for (&canonical, _) in TREE_SITTER_LANGUAGES.iter() {
        if canonical == key {
            return Some(canonical);
        }
    }
    // Try alias lookup
    LANGUAGE_ALIASES.get(key.as_str()).copied()
}

// ---------------------------------------------------------------------------
// Parser cache (#185)
// ---------------------------------------------------------------------------

/// Per-language cached parser. We lock → parse → unlock per call.
static PARSER_CACHE: LazyLock<std::sync::Mutex<HashMap<&'static str, Parser>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Obtain a cached parser for `lang_name`, creating one if needed.
/// Returns a MutexGuard holding the lock; caller should drop ASAP after parsing.
fn cached_parser(
    lang_name: &'static str,
) -> Option<std::sync::MutexGuard<'static, HashMap<&'static str, Parser>>> {
    let lang = TREE_SITTER_LANGUAGES.get(lang_name)?.clone();
    let mut cache = PARSER_CACHE.lock().unwrap();
    if !cache.contains_key(lang_name) {
        let mut parser = Parser::new();
        parser.set_language(&lang).ok()?;
        cache.insert(lang_name, parser);
    }
    Some(cache)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

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

    let mut sorted: Vec<_> = highlights.iter().collect();
    sorted.sort_by_key(|h| h.start);

    for hl in sorted {
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

// ---------------------------------------------------------------------------
// Internal highlighting
// ---------------------------------------------------------------------------

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
    let lang_name = match normalize_language(language) {
        Some(n) => n,
        None => return Vec::new(),
    };

    let tree = {
        let mut cache = match cached_parser(lang_name) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let parser = cache.get_mut(lang_name).unwrap();
        match parser.parse(line, None) {
            Some(t) => t,
            None => return Vec::new(),
        }
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

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_nodes(child, source, highlights);
    }
}

fn is_function_context(node: &tree_sitter::Node) -> bool {
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
    normalize_language(ext)
}
