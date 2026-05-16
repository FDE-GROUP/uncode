/// 工具自定义渲染器
///
/// 每个工具有独立的 render_call() 和 render_result() 函数，
/// 用于内联折叠方框中的摘要和展开内容。
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// 已知工具类型 — 静态分发，避免 HashMap + vtable 开销
#[derive(Clone, Copy)]
enum ToolKind {
    Read,
    Write,
    Edit,
    Grep,
    Bash,
    Find,
    Ls,
}

impl ToolKind {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "edit" => Some(Self::Edit),
            "grep" => Some(Self::Grep),
            "bash" => Some(Self::Bash),
            "find" => Some(Self::Find),
            "ls" => Some(Self::Ls),
            _ => None,
        }
    }
}

/// 工具渲染 trait — 所有颜色从 Theme 获取
pub trait ToolRenderer: Send + Sync {
    fn render_call(&self, args: &str, width: u16, theme: &Theme) -> Vec<Line<'static>>;
    fn render_result(&self, result: &str, width: u16, theme: &Theme) -> Vec<Line<'static>>;
}

/// 工具渲染注册表 — 零分配静态分发
pub struct ToolRendererRegistry;

impl ToolRendererRegistry {
    pub fn new() -> Self {
        Self
    }

    pub fn get(&self, tool_name: &str) -> &dyn ToolRenderer {
        match ToolKind::from_name(tool_name) {
            Some(ToolKind::Read) => &STATIC_READ,
            Some(ToolKind::Write) => &STATIC_WRITE,
            Some(ToolKind::Edit) => &STATIC_EDIT,
            Some(ToolKind::Grep) => &STATIC_GREP,
            Some(ToolKind::Bash) => &STATIC_BASH,
            Some(ToolKind::Find) => &STATIC_FIND,
            Some(ToolKind::Ls) => &STATIC_LS,
            None => &STATIC_FALLBACK,
        }
    }
}

impl Default for ToolRendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// --- Per-tool renderers ---

static STATIC_READ: ReadRenderer = ReadRenderer;
static STATIC_WRITE: WriteRenderer = WriteRenderer;
static STATIC_EDIT: EditRenderer = EditRenderer;
static STATIC_GREP: GrepRenderer = GrepRenderer;
static STATIC_BASH: BashRenderer = BashRenderer;
static STATIC_FIND: FindRenderer = FindRenderer;
static STATIC_LS: LsRenderer = LsRenderer;
static STATIC_FALLBACK: FallbackRenderer = FallbackRenderer;

struct ReadRenderer;

impl ToolRenderer for ReadRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let path = extract_path(args);
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(path, Style::default().fg(theme.tool_status.running)),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let lines: Vec<&str> = result.lines().collect();
        let line_count = lines.len();
        let mut out = vec![Line::from(Span::styled(
            format!("{line_count} lines"),
            Style::default().fg(theme.ui.footer_text),
        ))];

        let code_style = Style::default().fg(theme.markdown.code_text);
        out.extend(
            lines
                .iter()
                .take(80)
                .map(|line| Line::from(Span::styled(line.to_string(), code_style))),
        );
        if lines.len() > 80 {
            out.push(Line::from(Span::styled(
                format!("... ({} more lines)", lines.len() - 80),
                Style::default().fg(theme.ui.footer_text),
            )));
        }
        out
    }
}

struct WriteRenderer;

impl ToolRenderer for WriteRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let path = extract_path(args);
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(path, Style::default().fg(theme.tool_status.await_confirm)),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let bytes = result.len();
        let mut out = vec![Line::from(Span::styled(
            format!("{bytes} bytes written"),
            Style::default().fg(theme.tool_status.success),
        ))];
        let code_style = Style::default().fg(theme.markdown.code_text);
        out.extend(
            result
                .lines()
                .take(10)
                .map(|line| Line::from(Span::styled(line.to_string(), code_style))),
        );
        out
    }
}

struct EditRenderer;

impl ToolRenderer for EditRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let path = extract_path(args);
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(path, Style::default().fg(theme.tool_status.await_confirm)),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let all_lines: Vec<&str> = result.lines().collect();
        let mut out: Vec<Line<'static>> = all_lines
            .iter()
            .take(20)
            .map(|line| {
                let style = if line.starts_with('+') && !line.starts_with("++") {
                    theme.diff.added_text
                } else if line.starts_with('-') && !line.starts_with("--") {
                    theme.diff.removed_text
                } else if line.starts_with("@@") {
                    theme.diff.header
                } else {
                    theme.diff.context
                };
                Line::from(Span::styled(line.to_string(), Style::default().fg(style)))
            })
            .collect();
        if all_lines.len() > 20 {
            out.push(Line::from(Span::styled(
                "...",
                Style::default().fg(theme.ui.footer_text),
            )));
        }
        out
    }
}

struct GrepRenderer;

impl ToolRenderer for GrepRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let pattern =
            extract_quoted_value(args, &["\"pattern\""]).unwrap_or_else(|| args.to_string());
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(pattern, Style::default().fg(theme.tool_status.running)),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let matches: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("{} matches found", matches.len()),
            Style::default().fg(theme.tool_status.success),
        ))];
        let code_style = Style::default().fg(theme.markdown.code_text);
        out.extend(
            matches
                .iter()
                .take(50)
                .map(|line| Line::from(Span::styled(line.to_string(), code_style))),
        );
        if matches.len() > 50 {
            out.push(Line::from(Span::styled(
                format!("... {} more", matches.len() - 50),
                Style::default().fg(theme.ui.footer_text),
            )));
        }
        out
    }
}

struct BashRenderer;

impl ToolRenderer for BashRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let cmd = extract_command(args);
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(
                format!("$ {cmd}"),
                Style::default()
                    .fg(theme.bash.command)
                    .add_modifier(Modifier::BOLD),
            ),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        result
            .lines()
            .take(20)
            .map(|l| {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(theme.bash.stdout),
                ))
            })
            .collect()
    }
}

struct FindRenderer;

impl ToolRenderer for FindRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let query = extract_quoted_value(args, &["\"pattern\"", "\"path\""])
            .unwrap_or_else(|| args.to_string());
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(query, Style::default().fg(theme.tool_status.running)),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let files: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("{} files found", files.len()),
            Style::default().fg(theme.tool_status.success),
        ))];
        let file_style = Style::default().fg(theme.tool_status.running);
        out.extend(
            files
                .iter()
                .take(20)
                .map(|f| Line::from(Span::styled(f.to_string(), file_style))),
        );
        out
    }
}

struct LsRenderer;

impl ToolRenderer for LsRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let dir = extract_quoted_value(args, &["\"path\""]).unwrap_or_else(|| args.to_string());
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::styled(dir, Style::default().fg(theme.tool_status.running)),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let entries: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("{} entries", entries.len()),
            Style::default().fg(theme.tool_status.success),
        ))];
        let code_style = Style::default().fg(theme.markdown.code_text);
        out.extend(
            entries
                .iter()
                .take(20)
                .map(|e| Line::from(Span::styled(e.to_string(), code_style))),
        );
        out
    }
}

// --- Fallback ---

struct FallbackRenderer;

impl ToolRenderer for FallbackRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let summary = extract_quoted_value(args, &["\"path\"", "\"file_path\"", "\"command\""])
            .unwrap_or_else(|| args.to_string());
        vec![Line::from(vec![
            Span::styled("└ ", Style::default().fg(theme.ui.footer_text)),
            Span::raw(summary),
        ])]
    }

    fn render_result(&self, result: &str, _width: u16, _theme: &Theme) -> Vec<Line<'static>> {
        result
            .lines()
            .take(10)
            .map(|l| Line::from(l.to_string()))
            .collect()
    }
}

// --- Helpers ---

fn extract_path(args: &str) -> String {
    // Try full JSON parse first
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(path) = val
            .get("path")
            .or_else(|| val.get("file_path"))
            .and_then(|v| v.as_str())
        {
            return path.to_string();
        }
    }
    // Fallback: extract from partial JSON (delta streaming)
    extract_quoted_value(args, &["\"path\"", "\"file_path\""]).unwrap_or_else(|| args.to_string())
}

/// Extract the first non-empty quoted value following any of the given keys.
fn extract_quoted_value(s: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(pos) = s.find(key) {
            let rest = s.get(pos + key.len()..)?;
            let colon = rest.find(':')?;
            let after = rest.get(colon + 1..)?.trim_start();
            if after.starts_with('"') {
                let inner = after.get(1..)?;
                let end = inner.find('"')?;
                let val = &inner[..end];
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

fn extract_command(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
            return cmd.to_string();
        }
    }
    extract_quoted_value(args, &["\"command\""]).unwrap_or_else(|| args.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        Theme::default()
    }

    #[test]
    fn test_registry_has_all_tools() {
        let reg = ToolRendererRegistry::new();
        let theme = test_theme();
        for tool in &["read", "write", "edit", "grep", "bash", "find", "ls"] {
            let lines = reg.get(tool).render_call("test", 80, &theme);
            assert!(!lines.is_empty());
        }
    }

    #[test]
    fn test_fallback_for_unknown_tool() {
        let reg = ToolRendererRegistry::new();
        let theme = test_theme();
        let lines = reg.get("unknown_tool").render_call("args", 80, &theme);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_read_renderer() {
        let r = ReadRenderer;
        let theme = test_theme();
        let call = r.render_call(r#"{"path":"src/main.rs"}"#, 80, &theme);
        assert!(call[0].to_string().contains("src/main.rs"));

        let result = r.render_result("line1\nline2\nline3", 80, &theme);
        assert!(result[0].to_string().contains("3 lines"));
    }

    #[test]
    fn test_edit_renderer_diff_colors() {
        let r = EditRenderer;
        let theme = test_theme();
        let result = r.render_result("@@ -1,3 +1,4 @@\n-old\n+new\n ctx", 80, &theme);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_bash_renderer() {
        let r = BashRenderer;
        let theme = test_theme();
        let call = r.render_call(r#"{"command":"cargo test"}"#, 80, &theme);
        assert!(call[0].to_string().contains("$ cargo test"));
    }

    #[test]
    fn test_extract_path() {
        assert_eq!(extract_path(r#"{"path":"src/main.rs"}"#), "src/main.rs");
        assert_eq!(extract_path(r#"{"file_path":"Cargo.toml"}"#), "Cargo.toml");
        assert_eq!(extract_path("raw-args"), "raw-args");
    }
}
