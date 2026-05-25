use std::sync::LazyLock;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::theme::Theme;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

static THEME_PREFERENCES: &[&str] = &[
    "base16-eighties.dark",
    "Solarized (dark)",
    "base16-ocean.dark",
    "InspiredGitHub",
];

/// Select the best syntect theme using preference chain.
fn select_syntect_theme(theme: &Theme) -> Option<&syntect::highlighting::Theme> {
    // 1. User-specified theme name
    if let Some(ref name) = theme.syntax_theme_name
        && let Some(t) = THEME_SET.themes.get(name)
    {
        return Some(t);
    }
    // 2. Preference chain
    for &name in THEME_PREFERENCES {
        if let Some(t) = THEME_SET.themes.get(name) {
            return Some(t);
        }
    }
    // 3. First available
    THEME_SET.themes.values().next()
}

/// Public API: highlight code with default theme.
pub fn highlight_code(code: &str, language: &str) -> Vec<Line<'static>> {
    highlight_code_with_theme(code, language, &Theme::default())
}

/// Public API: highlight code with custom theme.
pub fn highlight_code_with_theme(code: &str, language: &str, theme: &Theme) -> Vec<Line<'static>> {
    if code.is_empty() {
        return vec![];
    }
    code.lines()
        .map(|line| highlight_line_with_theme(line, language, theme))
        .collect()
}

/// Highlight a single line of code using syntect.
pub fn highlight_line_with_theme(line: &str, language: &str, theme: &Theme) -> Line<'static> {
    if line.is_empty() {
        return Line::from("");
    }

    let syntax = SYNTAX_SET
        .find_syntax_by_token(language)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(language));

    let Some(syntax) = syntax else {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(theme.markdown.code_text),
        ));
    };

    let Some(syntect_theme) = select_syntect_theme(theme) else {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(theme.markdown.code_text),
        ));
    };

    let mut highlighter = HighlightLines::new(syntax, syntect_theme);
    let regions = highlighter.highlight_line(line, &SYNTAX_SET);

    let Ok(regions) = regions else {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(theme.markdown.code_text),
        ));
    };

    let spans: Vec<Span<'static>> = regions
        .iter()
        .map(|(style, text)| Span::styled(text.to_string(), syntect_to_ratatui(*style)))
        .collect();

    if spans.is_empty() {
        Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(theme.markdown.code_text),
        ))
    } else {
        Line::from(spans)
    }
}

/// Convert a syntect Style to a ratatui Style.
fn syntect_to_ratatui(s: syntect::highlighting::Style) -> Style {
    let fg = ratatui::style::Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b);
    let mut style = Style::default().fg(fg);
    if s.font_style.contains(FontStyle::BOLD) {
        style = style.bold();
    }
    if s.font_style.contains(FontStyle::ITALIC) {
        style = style.italic();
    }
    if s.font_style.contains(FontStyle::UNDERLINE) {
        style = style.underlined();
    }
    style
}

/// Detect programming language from file extension using syntect.
pub fn detect_language_from_path(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    let syntax = SYNTAX_SET
        .find_syntax_by_extension(ext)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(path))?;
    Some(syntax.name.clone().leak() as &str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_language_from_path_rs() {
        assert_eq!(detect_language_from_path("file.rs"), Some("Rust"));
    }

    #[test]
    fn detect_language_from_path_py() {
        assert_eq!(detect_language_from_path("file.py"), Some("Python"));
    }

    #[test]
    fn detect_language_from_path_js() {
        assert_eq!(detect_language_from_path("file.js"), Some("JavaScript"));
    }

    #[test]
    fn detect_language_from_path_ts() {
        // syntect's default syntax set may not include TypeScript
        let result = detect_language_from_path("file.ts");
        assert!(result.is_none() || result == Some("TypeScript"));
    }

    #[test]
    fn detect_language_from_full_path() {
        assert_eq!(detect_language_from_path("src/main.rs"), Some("Rust"));
    }

    #[test]
    fn detect_language_unknown_extension() {
        assert_eq!(detect_language_from_path("file.xyz"), None);
    }

    #[test]
    fn detect_language_empty_path() {
        assert_eq!(detect_language_from_path(""), None);
    }

    #[test]
    fn detect_language_no_extension() {
        assert_eq!(detect_language_from_path("Makefile"), None);
    }

    #[test]
    fn detect_language_multiple_dots() {
        // Rust's Path::extension returns "ts" for "test.spec.ts"
        // Whether syntect finds it depends on its bundled syntaxes
        let result = detect_language_from_path("test.spec.ts");
        assert!(result.is_none() || result == Some("TypeScript"));
    }

    #[test]
    fn detect_language_uppercase_extension() {
        // syntect's extension matching appears to be case-insensitive
        assert_eq!(detect_language_from_path("file.RS"), Some("Rust"));
    }

    #[test]
    fn highlight_code_empty_input() {
        let result = highlight_code("", "rust");
        assert!(result.is_empty());
    }

    #[test]
    fn highlight_code_with_theme_empty_input() {
        let result = highlight_code_with_theme("", "rust", &Theme::default());
        assert!(result.is_empty());
    }

    #[test]
    fn highlight_line_with_theme_empty_input() {
        let line = highlight_line_with_theme("", "rust", &Theme::default());
        assert_eq!(line, Line::from(""));
    }

    #[test]
    fn highlight_code_unknown_language_fallback() {
        let result = highlight_code("hello world", "nonexistent_lang_xyz");
        // Should produce one line with plain text styling
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn highlight_code_preserves_line_count() {
        let result = highlight_code("line1\nline2\nline3", "rust");
        assert_eq!(result.len(), 3);
    }
}
