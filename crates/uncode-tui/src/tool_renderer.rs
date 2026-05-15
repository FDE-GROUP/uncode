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
        vec![Line::from(Span::styled(
            format!("📖 读取 {path}"),
            Style::default().fg(theme.tool_status.running),
        ))]
    }

    fn render_result(&self, result: &str, width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let lines: Vec<&str> = result.lines().collect();
        let line_count = lines.len();
        let mut out = vec![Line::from(Span::styled(
            format!("{line_count} 行"),
            Style::default().fg(theme.ui.footer_text),
        ))];

        let max_lines = (width as usize).min(20);
        for line in lines.iter().take(max_lines) {
            out.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(theme.markdown.code_text),
            )));
        }
        if lines.len() > max_lines {
            out.push(Line::from(Span::styled(
                "...",
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
        vec![Line::from(Span::styled(
            format!("📝 写入 {path}"),
            Style::default().fg(theme.tool_status.await_confirm),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let bytes = result.len();
        let mut out = vec![Line::from(Span::styled(
            format!("写入 {bytes} 字节"),
            Style::default().fg(theme.tool_status.success),
        ))];
        let lines: Vec<&str> = result.lines().take(10).collect();
        for line in lines {
            out.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(theme.markdown.code_text),
            )));
        }
        out
    }
}

struct EditRenderer;

impl ToolRenderer for EditRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let path = extract_path(args);
        vec![Line::from(Span::styled(
            format!("✏️ 编辑 {path}"),
            Style::default().fg(theme.tool_status.await_confirm),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for line in result.lines().take(20) {
            if line.starts_with('+') && !line.starts_with("++") {
                out.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.diff.added_text),
                )));
            } else if line.starts_with('-') && !line.starts_with("--") {
                out.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.diff.removed_text),
                )));
            } else if line.starts_with("@@") {
                out.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.diff.header),
                )));
            } else {
                out.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.diff.context),
                )));
            }
        }
        if result.lines().count() > 20 {
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
        vec![Line::from(Span::styled(
            format!("🔍 搜索 {args}"),
            Style::default().fg(theme.tool_status.running),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let matches: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("找到 {} 处匹配", matches.len()),
            Style::default().fg(theme.tool_status.success),
        ))];
        for line in matches.iter().take(15) {
            out.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(theme.markdown.code_text),
            )));
        }
        if matches.len() > 15 {
            out.push(Line::from(Span::styled(
                format!("... 还有 {} 处", matches.len() - 15),
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
        vec![Line::from(Span::styled(
            format!("$ {cmd}"),
            Style::default()
                .fg(theme.bash.command)
                .add_modifier(Modifier::BOLD),
        ))]
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
        vec![Line::from(Span::styled(
            format!("📂 查找 {args}"),
            Style::default().fg(theme.tool_status.running),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let files: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("找到 {} 个文件", files.len()),
            Style::default().fg(theme.tool_status.success),
        ))];
        for f in files.iter().take(20) {
            out.push(Line::from(Span::styled(
                f.to_string(),
                Style::default().fg(theme.tool_status.running),
            )));
        }
        out
    }
}

struct LsRenderer;

impl ToolRenderer for LsRenderer {
    fn render_call(&self, args: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        vec![Line::from(Span::styled(
            format!("📂 列出 {args}"),
            Style::default().fg(theme.tool_status.running),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let entries: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("{} 个条目", entries.len()),
            Style::default().fg(theme.tool_status.success),
        ))];
        for e in entries.iter().take(20) {
            out.push(Line::from(Span::styled(
                e.to_string(),
                Style::default().fg(theme.markdown.code_text),
            )));
        }
        out
    }
}

// --- Fallback ---

struct FallbackRenderer;

impl ToolRenderer for FallbackRenderer {
    fn render_call(&self, args: &str, _width: u16, _theme: &Theme) -> Vec<Line<'static>> {
        vec![Line::from(args.to_string())]
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
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(path) = val
            .get("path")
            .or_else(|| val.get("file_path"))
            .and_then(|v| v.as_str())
        {
            return path.to_string();
        }
    }
    args.to_string()
}

fn extract_command(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
            return cmd.to_string();
        }
    }
    args.to_string()
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
        assert!(result[0].to_string().contains("3 行"));
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
