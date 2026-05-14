/// 工具自定义渲染器
///
/// 每个工具有独立的 render_call() 和 render_result() 函数，
/// 用于内联折叠方框中的摘要和展开内容。
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::HashMap;

/// 工具渲染 trait
pub trait ToolRenderer: Send + Sync {
    /// 渲染工具调用摘要（折叠状态）
    fn render_call(&self, args: &str, width: u16) -> Vec<Line<'static>>;
    /// 渲染工具结果（展开状态）
    fn render_result(&self, result: &str, width: u16) -> Vec<Line<'static>>;
}

/// 工具渲染注册表
pub struct ToolRendererRegistry {
    renderers: HashMap<String, Box<dyn ToolRenderer>>,
}

impl ToolRendererRegistry {
    pub fn new() -> Self {
        let mut renderers: HashMap<String, Box<dyn ToolRenderer>> = HashMap::new();
        renderers.insert("read".into(), Box::new(ReadRenderer));
        renderers.insert("write".into(), Box::new(WriteRenderer));
        renderers.insert("edit".into(), Box::new(EditRenderer));
        renderers.insert("grep".into(), Box::new(GrepRenderer));
        renderers.insert("bash".into(), Box::new(BashRenderer));
        renderers.insert("find".into(), Box::new(FindRenderer));
        renderers.insert("ls".into(), Box::new(LsRenderer));
        Self { renderers }
    }

    pub fn get(&self, tool_name: &str) -> &dyn ToolRenderer {
        self.renderers
            .get(tool_name)
            .map(|b| b.as_ref())
            .unwrap_or(&FALLBACK_RENDERER as &dyn ToolRenderer)
    }
}

impl Default for ToolRendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// --- Per-tool renderers ---

struct ReadRenderer;

impl ToolRenderer for ReadRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        let path = extract_path(args);
        vec![Line::from(Span::styled(
            format!("📖 读取 {path}"),
            Style::default().fg(Color::Cyan),
        ))]
    }

    fn render_result(&self, result: &str, width: u16) -> Vec<Line<'static>> {
        let lines: Vec<&str> = result.lines().collect();
        let line_count = lines.len();
        let mut out = vec![Line::from(Span::styled(
            format!("{line_count} 行"),
            Style::default().fg(Color::DarkGray),
        ))];

        let max_lines = (width as usize).min(20);
        for line in lines.iter().take(max_lines) {
            out.push(Line::from(line.to_string()));
        }
        if lines.len() > max_lines {
            out.push(Line::from(Span::styled(
                "...",
                Style::default().fg(Color::DarkGray),
            )));
        }
        out
    }
}

struct WriteRenderer;

impl ToolRenderer for WriteRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        let path = extract_path(args);
        vec![Line::from(Span::styled(
            format!("📝 写入 {path}"),
            Style::default().fg(Color::Yellow),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16) -> Vec<Line<'static>> {
        let bytes = result.len();
        let mut out = vec![Line::from(Span::styled(
            format!("写入 {bytes} 字节"),
            Style::default().fg(Color::Green),
        ))];
        let lines: Vec<&str> = result.lines().take(10).collect();
        for line in lines {
            out.push(Line::from(line.to_string()));
        }
        out
    }
}

struct EditRenderer;

impl ToolRenderer for EditRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        let path = extract_path(args);
        vec![Line::from(Span::styled(
            format!("✏️ 编辑 {path}"),
            Style::default().fg(Color::Yellow),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for line in result.lines().take(20) {
            if line.starts_with('+') && !line.starts_with("++") {
                out.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Green),
                )));
            } else if line.starts_with('-') && !line.starts_with("--") {
                out.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Red),
                )));
            } else if line.starts_with("@@") {
                out.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Cyan),
                )));
            } else {
                out.push(Line::from(line.to_string()));
            }
        }
        if result.lines().count() > 20 {
            out.push(Line::from(Span::styled(
                "...",
                Style::default().fg(Color::DarkGray),
            )));
        }
        out
    }
}

struct GrepRenderer;

impl ToolRenderer for GrepRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        vec![Line::from(Span::styled(
            format!("🔍 搜索 {args}"),
            Style::default().fg(Color::Cyan),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16) -> Vec<Line<'static>> {
        let matches: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("找到 {} 处匹配", matches.len()),
            Style::default().fg(Color::Green),
        ))];
        for line in matches.iter().take(15) {
            out.push(Line::from(line.to_string()));
        }
        if matches.len() > 15 {
            out.push(Line::from(Span::styled(
                format!("... 还有 {} 处", matches.len() - 15),
                Style::default().fg(Color::DarkGray),
            )));
        }
        out
    }
}

struct BashRenderer;

impl ToolRenderer for BashRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        let cmd = extract_command(args);
        vec![Line::from(Span::styled(
            format!("$ {cmd}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16) -> Vec<Line<'static>> {
        result
            .lines()
            .take(20)
            .map(|l| Line::from(l.to_string()))
            .collect()
    }
}

struct FindRenderer;

impl ToolRenderer for FindRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        vec![Line::from(Span::styled(
            format!("📂 查找 {args}"),
            Style::default().fg(Color::Cyan),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16) -> Vec<Line<'static>> {
        let files: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("找到 {} 个文件", files.len()),
            Style::default().fg(Color::Green),
        ))];
        for f in files.iter().take(20) {
            out.push(Line::from(Span::styled(
                f.to_string(),
                Style::default().fg(Color::Cyan),
            )));
        }
        out
    }
}

struct LsRenderer;

impl ToolRenderer for LsRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        vec![Line::from(Span::styled(
            format!("📂 列出 {args}"),
            Style::default().fg(Color::Cyan),
        ))]
    }

    fn render_result(&self, result: &str, _width: u16) -> Vec<Line<'static>> {
        let entries: Vec<&str> = result.lines().collect();
        let mut out = vec![Line::from(Span::styled(
            format!("{} 个条目", entries.len()),
            Style::default().fg(Color::Green),
        ))];
        for e in entries.iter().take(20) {
            out.push(Line::from(e.to_string()));
        }
        out
    }
}

// --- Fallback ---

struct FallbackRenderer;

static FALLBACK_RENDERER: FallbackRenderer = FallbackRenderer;

impl ToolRenderer for FallbackRenderer {
    fn render_call(&self, args: &str, _width: u16) -> Vec<Line<'static>> {
        vec![Line::from(args.to_string())]
    }

    fn render_result(&self, result: &str, _width: u16) -> Vec<Line<'static>> {
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

    #[test]
    fn test_registry_has_all_tools() {
        let reg = ToolRendererRegistry::new();
        for tool in &["read", "write", "edit", "grep", "bash", "find", "ls"] {
            let lines = reg.get(tool).render_call("test", 80);
            assert!(!lines.is_empty());
        }
    }

    #[test]
    fn test_fallback_for_unknown_tool() {
        let reg = ToolRendererRegistry::new();
        let lines = reg.get("unknown_tool").render_call("args", 80);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_read_renderer() {
        let r = ReadRenderer;
        let call = r.render_call(r#"{"path":"src/main.rs"}"#, 80);
        assert!(call[0].to_string().contains("src/main.rs"));

        let result = r.render_result("line1\nline2\nline3", 80);
        assert!(result[0].to_string().contains("3 行"));
    }

    #[test]
    fn test_edit_renderer_diff_colors() {
        let r = EditRenderer;
        let result = r.render_result("@@ -1,3 +1,4 @@\n-old\n+new\n ctx", 80);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_bash_renderer() {
        let r = BashRenderer;
        let call = r.render_call(r#"{"command":"cargo test"}"#, 80);
        assert!(call[0].to_string().contains("$ cargo test"));
    }

    #[test]
    fn test_extract_path() {
        assert_eq!(extract_path(r#"{"path":"src/main.rs"}"#), "src/main.rs");
        assert_eq!(extract_path(r#"{"file_path":"Cargo.toml"}"#), "Cargo.toml");
        assert_eq!(extract_path("raw-args"), "raw-args");
    }
}
