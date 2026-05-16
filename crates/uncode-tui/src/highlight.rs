use std::sync::LazyLock;

use ratatui::style::{Modifier, Style};
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
    if let Some(ref name) = theme.syntax_theme_name {
        if let Some(t) = THEME_SET.themes.get(name) {
            return Some(t);
        }
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
        style = style.add_modifier(Modifier::BOLD);
    }
    if s.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if s.font_style.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
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
