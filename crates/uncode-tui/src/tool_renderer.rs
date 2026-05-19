/// 工具自定义渲染器
///
/// 每个工具有独立的 render_call() 和 render_result() 函数，
/// render_call 返回内联显示文字（嵌入 header 行），render_result 返回展开结果行。
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

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
    WebFetch,
    WebSearch,
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
            "web_fetch" => Some(Self::WebFetch),
            "web_search" => Some(Self::WebSearch),
            _ => None,
        }
    }
}

/// 工具渲染 trait — 所有颜色从 Theme 获取
pub trait ToolRenderer: Send + Sync {
    fn render_call(&self, args: &str, workdir: &str) -> String;
    fn render_result(
        &self,
        args: &str,
        result: &str,
        width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>>;
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
            Some(ToolKind::WebFetch) => &STATIC_WEB_FETCH,
            Some(ToolKind::WebSearch) => &STATIC_WEB_SEARCH,
            None => &STATIC_FALLBACK,
        }
    }
}

impl Default for ToolRendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// --- 各工具渲染器 ---

static STATIC_READ: ReadRenderer = ReadRenderer;
static STATIC_WRITE: WriteRenderer = WriteRenderer;
static STATIC_EDIT: EditRenderer = EditRenderer;
static STATIC_GREP: GrepRenderer = GrepRenderer;
static STATIC_BASH: BashRenderer = BashRenderer;
static STATIC_FIND: FindRenderer = FindRenderer;
static STATIC_LS: LsRenderer = LsRenderer;
static STATIC_WEB_FETCH: WebFetchRenderer = WebFetchRenderer;
static STATIC_WEB_SEARCH: WebSearchRenderer = WebSearchRenderer;
static STATIC_FALLBACK: FallbackRenderer = FallbackRenderer;

struct ReadRenderer;

/// Read: `→ 相对路径 [limit=X, offset=Y]`，对标 opencode `→ Read path [extras]`
impl ToolRenderer for ReadRenderer {
    fn render_call(&self, args: &str, workdir: &str) -> String {
        let path = relative_path(extract_path(args), workdir);
        let extra = format_extra_args(args, &["filePath", "file_path", "path"]);
        if extra.is_empty() {
            format!("→ {path}")
        } else {
            format!("→ {path} {extra}")
        }
    }

    fn render_result(
        &self,
        args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let lines: Vec<&str> = result.lines().collect();
        let line_count = lines.len();
        let mut out = vec![Line::from(Span::styled(
            format!("{line_count} lines"),
            Style::default().fg(theme.ui.footer_text),
        ))];

        let path = extract_path(args);
        let ext = file_extension(&path);
        let highlighted = highlight_code(result, &ext, theme);
        if highlighted.is_empty() {
            let code_style = Style::default().fg(theme.markdown.code_text);
            out.extend(
                lines
                    .iter()
                    .take(80)
                    .map(|line| Line::from(Span::styled(line.to_string(), code_style))),
            );
        } else {
            out.extend(highlighted.into_iter().take(80));
        }
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

/// Write: 仅显示相对路径，展开后展示统一 diff
impl ToolRenderer for WriteRenderer {
    fn render_call(&self, args: &str, workdir: &str) -> String {
        relative_path(extract_path(args), workdir)
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let all_lines: Vec<&str> = result.lines().collect();
        let mut out: Vec<Line<'static>> = all_lines
            .iter()
            .take(50)
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
        if all_lines.len() > 50 {
            out.push(Line::from(Span::styled(
                "...",
                Style::default().fg(theme.ui.footer_text),
            )));
        }
        out
    }
}

struct EditRenderer;

/// Edit: 相对路径 + 展开后显示 `┃` 行号 diff（对标 opencode `← Edit path`）
impl ToolRenderer for EditRenderer {
    fn render_call(&self, args: &str, workdir: &str) -> String {
        let path = extract_path(args);
        if path.is_empty() || path == args {
            return String::new();
        }
        relative_path(path, workdir)
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let all_lines: Vec<&str> = result.lines().collect();
        let mut added = 0usize;
        let mut removed = 0usize;

        for line in &all_lines {
            if line.starts_with('+') && !line.starts_with("++") {
                added += 1;
            } else if line.starts_with('-') && !line.starts_with("--") {
                removed += 1;
            }
        }

        let mut out = Vec::new();

        let summary = if added == 1 && removed == 1 {
            "1 addition, 1 deletion".to_string()
        } else if added == 0 && removed > 0 {
            format!("{removed} deletions")
        } else if removed == 0 && added > 0 {
            format!("{added} additions")
        } else {
            format!("{added} additions, {removed} deletions")
        };
        out.push(Line::from(Span::styled(
            summary,
            Style::default().fg(theme.ui.footer_text),
        )));

        // Parse @@ hunks and render with line numbers
        let prefix = "  ┃";
        let prefix_style = Style::default().fg(theme.ui.footer_text);
        let max_show = 20;
        let mut shown = 0;
        let mut old_line: Option<u64> = None;
        let mut new_line: Option<u64> = None;
        let mut skipped = 0usize;

        for line in &all_lines {
            if line.starts_with("---") || line.starts_with("+++") {
                skipped += 1;
                continue;
            }
            if line.starts_with("@@") {
                (old_line, new_line) = parse_hunk_header(line);
                skipped += 1;
                continue;
            }
            if shown >= max_show {
                break;
            }

            let (content, style, num, indicator) =
                if line.starts_with('+') && !line.starts_with("++") {
                    let n = new_line.unwrap_or(0);
                    new_line = Some(n + 1);
                    (&line[1..], theme.diff.added_text, n, "+")
                } else if line.starts_with('-') && !line.starts_with("--") {
                    let n = old_line.unwrap_or(0);
                    old_line = Some(n + 1);
                    (&line[1..], theme.diff.removed_text, n, "-")
                } else {
                    let n = new_line.unwrap_or(0);
                    old_line = old_line.map(|o| o + 1);
                    new_line = Some(n + 1);
                    (
                        if let Some(stripped) = line.strip_prefix(' ') {
                            stripped
                        } else {
                            line
                        },
                        theme.diff.context,
                        n,
                        " ",
                    )
                };
            shown += 1;

            let indicator_style = if indicator == "+" {
                theme.diff.added_text
            } else if indicator == "-" {
                theme.diff.removed_text
            } else {
                theme.diff.context
            };

            out.push(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(
                    format!(" {:>5} ", num),
                    Style::default().fg(theme.ui.footer_text),
                ),
                Span::styled(indicator, Style::default().fg(indicator_style)),
                Span::styled(content.to_string(), Style::default().fg(style)),
            ]));
        }

        let remaining = all_lines.len().saturating_sub(shown + skipped);
        if remaining > 0 {
            out.push(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(
                    format!("  \u{2026} {remaining} more lines"),
                    Style::default().fg(theme.ui.footer_text),
                ),
            ]));
        }

        out
    }
}

struct GrepRenderer;

/// Grep: `"pattern" [in dir]`，对标 opencode `✱ Grep "pattern" in dir/`
impl ToolRenderer for GrepRenderer {
    fn render_call(&self, args: &str, workdir: &str) -> String {
        let pattern = extract_json_field(args, "pattern");
        let path = extract_json_field(args, "path");
        let dir = relative_path(path, workdir);
        if pattern.is_empty() {
            extract_quoted_value(args, &["\"pattern\""]).unwrap_or_default()
        } else if dir.is_empty() {
            format!("\"{pattern}\"")
        } else {
            format!("\"{pattern}\" in {dir}")
        }
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
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

/// Bash: `# 描述\n$ 命令`（双行），对标 opencode `# desc\n$ cmd`
/// 无描述时默认 "Shell"，workdir 非空且不在描述中时追加 "in {dir}"
impl ToolRenderer for BashRenderer {
    fn render_call(&self, args: &str, workdir: &str) -> String {
        let cmd = extract_command(args);
        let desc = extract_json_field(args, "description");
        let wd = extract_json_field(args, "workdir");
        let title = if desc.is_empty() {
            "Shell".to_string()
        } else {
            desc.clone()
        };
        let dir = if wd.is_empty() || wd == "." {
            String::new()
        } else {
            relative_path(&wd, workdir)
        };
        let title = if dir.is_empty() || title.contains(&dir) {
            title
        } else {
            format!("{title} in {dir}")
        };
        if cmd.is_empty() {
            format!("# {title}")
        } else {
            format!("# {title}\n$ {cmd}")
        }
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
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

/// Find: `"pattern" [in path]`，对标 opencode `✱ Glob "pattern" in dir/`
impl ToolRenderer for FindRenderer {
    fn render_call(&self, args: &str, workdir: &str) -> String {
        let pattern = extract_json_field(args, "pattern");
        let path = relative_path(extract_path(args), workdir);
        if pattern.is_empty() {
            path
        } else if path.is_empty() {
            format!("\"{pattern}\"")
        } else {
            format!("\"{pattern}\" in {path}")
        }
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
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

/// Ls: 相对目录路径，对标 opencode `→ List dir/`
impl ToolRenderer for LsRenderer {
    fn render_call(&self, args: &str, workdir: &str) -> String {
        let dir = extract_json_field(args, "path");
        if dir.is_empty() {
            extract_quoted_value(args, &["\"path\""]).unwrap_or_default()
        } else {
            relative_path(dir, workdir)
        }
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
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

struct WebFetchRenderer;

/// WebFetch: `GET <url>`，对标 opencode `% WebFetch url`
impl ToolRenderer for WebFetchRenderer {
    fn render_call(&self, args: &str, _workdir: &str) -> String {
        let url = extract_url(args);
        format!("GET {url}")
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let lines: Vec<&str> = result.lines().collect();
        let code_style = Style::default().fg(theme.markdown.code_text);
        let mut out: Vec<Line<'static>> = lines
            .iter()
            .take(30)
            .map(|l| Line::from(Span::styled(l.to_string(), code_style)))
            .collect();
        if lines.len() > 30 {
            out.push(Line::from(Span::styled(
                format!("... ({} more lines)", lines.len() - 30),
                Style::default().fg(theme.ui.footer_text),
            )));
        }
        out
    }
}

struct WebSearchRenderer;

/// WebSearch: 仅显示查询文本，对标 opencode `◈ provider "query"`
impl ToolRenderer for WebSearchRenderer {
    fn render_call(&self, args: &str, _workdir: &str) -> String {
        extract_quoted_value(args, &["\"query\""]).unwrap_or_else(|| args.to_string())
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        result
            .lines()
            .take(20)
            .map(|l| {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(theme.markdown.code_text),
                ))
            })
            .collect()
    }
}

// --- 回退渲染器（未知工具）---

struct FallbackRenderer;

impl ToolRenderer for FallbackRenderer {
    fn render_call(&self, args: &str, _workdir: &str) -> String {
        let extra = format_extra_args(args, &[]);
        if extra.is_empty() {
            extract_quoted_value(args, &["\"path\"", "\"file_path\"", "\"command\""])
                .unwrap_or_else(|| truncate_for_display(args))
        } else {
            extra
        }
    }

    fn render_result(
        &self,
        _args: &str,
        result: &str,
        _width: u16,
        _theme: &Theme,
    ) -> Vec<Line<'static>> {
        result
            .lines()
            .take(10)
            .map(|l| Line::from(l.to_string()))
            .collect()
    }
}

// --- 语法高亮 ---

/// 从文件路径提取扩展名，用于确定语法
fn file_extension(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 使用 syntect 对代码进行语法高亮，返回带颜色的 Line
fn highlight_code(code: &str, ext: &str, _theme: &Theme) -> Vec<Line<'static>> {
    if ext.is_empty() || code.is_empty() {
        return Vec::new();
    }

    use std::sync::OnceLock;
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    let ss = SS.get_or_init(SyntaxSet::load_defaults_newlines);
    let ts = TS.get_or_init(ThemeSet::load_defaults);

    let syntax = ss
        .find_syntax_by_extension(ext)
        .or_else(|| ss.find_syntax_by_first_line(code));

    let syntax = match syntax {
        Some(s) => s,
        None => return Vec::new(),
    };

    let syntect_theme = &ts.themes["base16-ocean.dark"];
    let mut h = HighlightLines::new(syntax, syntect_theme);

    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(syntect::highlighting::Style, &str)> =
            h.highlight_line(line, ss).unwrap_or_default();
        let spans: Vec<Span> = ranges
            .into_iter()
            .map(|(style, text)| {
                let text = text.trim_end_matches('\n').to_string();
                Span::styled(
                    text,
                    Style::default()
                        .fg(syntect_color_to_ratatui(style.foreground))
                        .add_modifier(
                            if style
                                .font_style
                                .contains(syntect::highlighting::FontStyle::BOLD)
                            {
                                ratatui::style::Modifier::BOLD
                            } else {
                                ratatui::style::Modifier::empty()
                            },
                        ),
                )
            })
            .collect();
        if !spans.is_empty() {
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(" "));
        }
    }
    lines
}

fn syntect_color_to_ratatui(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

// --- 辅助函数 ---

/// 从 tool args JSON 中提取主字段以外的标量参数，格式化为 `[key=val, ...]`
/// 对标 opencode 的 info() 函数：跳过主字段（如 filePath），其余标量参数用 `[k=v, ...]` 展示
/// 同时检查 `arguments` 嵌套和 `function.arguments` 嵌套（兼容 OpenAI/Anthropic 格式）
fn format_extra_args(args: &str, skip_keys: &[&str]) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        let obj = if let Some(inner) = val
            .get("arguments")
            .or_else(|| val.get("function").and_then(|f| f.get("arguments")))
            .and_then(|v| v.as_object())
        {
            inner
        } else if let Some(outer) = val.as_object() {
            outer
        } else {
            return String::new();
        };
        let pairs: Vec<String> = obj
            .iter()
            .filter(|(k, v)| {
                if skip_keys.contains(&k.as_str()) {
                    return false;
                }
                v.is_string() || v.is_number() || v.is_boolean()
            })
            .map(|(k, v)| format!("{k}={}", val_to_string(v)))
            .collect();
        if pairs.is_empty() {
            return String::new();
        }
        return format!("[{}]", pairs.join(", "));
    }
    String::new()
}

/// JSON Value 转展示字符串
fn val_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// 将绝对路径转为工作目录下的相对路径
fn relative_path(abs: impl Into<String>, workdir: &str) -> String {
    let abs = abs.into();
    if abs.is_empty() || workdir.is_empty() {
        return abs;
    }
    let wd = if workdir.starts_with("~/") {
        std::env::var("HOME")
            .map(|home| format!("{}/{}", home, &workdir[2..]))
            .unwrap_or_else(|_| workdir.to_string())
    } else {
        workdir.to_string()
    };
    if abs.starts_with(&wd) {
        let rest = &abs[wd.len()..];
        rest.trim_start_matches('/').to_string()
    } else {
        abs
    }
}

/// 从 JSON args 中提取指定字段的值（字符串/数字/布尔）
/// 同时检查顶层、`arguments` 嵌套、以及 `function.arguments` 嵌套（兼容 OpenAI/Anthropic 格式）
fn extract_json_field(args: &str, field: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        let wrappers: [Option<&serde_json::Value>; 3] = [
            Some(&val),
            val.get("arguments"),
            val.get("function").and_then(|f| f.get("arguments")),
        ];
        for wrapper in wrappers.into_iter().flatten() {
            if let Some(obj) = wrapper.as_object()
                && let Some(v) = obj.get(field)
            {
                return match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => String::new(),
                };
            }
        }
    }
    String::new()
}

/// 截断过长文本用于显示
fn truncate_for_display(s: &str) -> String {
    if s.len() > 60 {
        let end = s.floor_char_boundary(59);
        format!("{}…", &s[..end])
    } else {
        s.to_string()
    }
}

/// 从工具参数中提取路径（支持 `filePath` / `file_path` / `path` 三种字段名）
fn extract_path(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        let wrappers: [Option<&serde_json::Value>; 3] = [
            Some(&val),
            val.get("arguments"),
            val.get("function").and_then(|f| f.get("arguments")),
        ];
        for wrapper in wrappers.into_iter().flatten() {
            if let Some(obj) = wrapper.as_object()
                && let Some(path) = obj
                    .get("filePath")
                    .or_else(|| obj.get("file_path"))
                    .or_else(|| obj.get("path"))
                    .and_then(|v| v.as_str())
            {
                return path.to_string();
            }
        }
        return String::new();
    }
    extract_quoted_value(args, &["\"filePath\"", "\"file_path\"", "\"path\""])
        .unwrap_or_else(|| args.to_string())
}

/// 从不完整 JSON（流式 delta）中提取指定 key 后面的第一个非空引用值
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

/// 从工具参数中提取命令文本（同时检查 `arguments` 嵌套）
fn extract_command(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        let wrappers: [Option<&serde_json::Value>; 3] = [
            Some(&val),
            val.get("arguments"),
            val.get("function").and_then(|f| f.get("arguments")),
        ];
        for wrapper in wrappers.into_iter().flatten() {
            if let Some(obj) = wrapper.as_object()
                && let Some(cmd) = obj.get("command").and_then(|v| v.as_str())
            {
                return cmd.to_string();
            }
        }
    }
    extract_quoted_value(args, &["\"command\""]).unwrap_or_else(|| args.to_string())
}

/// 从工具参数中提取 URL（同时检查 `arguments` 嵌套）
fn extract_url(args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        let wrappers: [Option<&serde_json::Value>; 3] = [
            Some(&val),
            val.get("arguments"),
            val.get("function").and_then(|f| f.get("arguments")),
        ];
        for wrapper in wrappers.into_iter().flatten() {
            if let Some(obj) = wrapper.as_object()
                && let Some(url) = obj.get("url").and_then(|v| v.as_str())
            {
                return url.to_string();
            }
        }
    }
    extract_quoted_value(args, &["\"url\""]).unwrap_or_else(|| args.to_string())
}

/// 解析 unified diff hunk 头 `@@ -旧起始,旧行数 +新起始,新行数 @@`，提取新旧起始行号
fn parse_hunk_header(line: &str) -> (Option<u64>, Option<u64>) {
    if let Some(rest) = line.strip_prefix("@@ -")
        && let Some((old_part, rest)) = rest.split_once('+')
    {
        let old_start = old_part
            .split(',')
            .next()
            .and_then(|s| s.trim().parse().ok());
        let new_start = rest
            .split(' ')
            .next()
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse().ok());
        return (old_start, new_start);
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_all_tools() {
        let reg = ToolRendererRegistry::new();
        let test_args =
            r#"{"path":"test","pattern":"test","command":"test","url":"test","query":"test"}"#;
        for tool in &[
            "read",
            "write",
            "edit",
            "grep",
            "bash",
            "find",
            "ls",
            "web_fetch",
            "web_search",
        ] {
            let text = reg.get(tool).render_call(test_args, "");
            assert!(!text.is_empty(), "tool {tool} returned empty");
        }
        // edit with empty args returns empty
        let edit_text = reg.get("edit").render_call("{}", "");
        assert!(edit_text.is_empty());
    }

    #[test]
    fn test_fallback_for_unknown_tool() {
        let reg = ToolRendererRegistry::new();
        let text = reg.get("unknown_tool").render_call("args", "");
        assert!(!text.is_empty());
    }

    #[test]
    fn test_read_renderer() {
        let r = ReadRenderer;
        let call = r.render_call(
            r#"{"filePath":"src/main.rs","limit":50}"#,
            "/home/user/project",
        );
        assert!(call.contains("src/main.rs"));
        assert!(call.contains("[limit=50]"));

        let theme = Theme::default();
        let result = r.render_result("{}", "line1\nline2\nline3", 80, &theme);
        assert!(result[0].to_string().contains("3 lines"));
    }

    #[test]
    fn test_read_renderer_relative_path() {
        let r = ReadRenderer;
        let call = r.render_call(
            r#"{"filePath":"/home/user/project/src/main.rs"}"#,
            "/home/user/project",
        );
        assert_eq!(call, "→ src/main.rs");
    }

    #[test]
    fn test_edit_renderer_diff_colors() {
        let r = EditRenderer;
        let theme = Theme::default();
        let result = r.render_result(
            r#"{"filePath":"test.rs"}"#,
            "@@ -1,3 +1,4 @@\n-old\n+new\n ctx",
            80,
            &theme,
        );
        // Summary + 3 diff lines
        assert!(result.len() >= 3);
        // First line should be summary
        assert!(result[0].to_string().contains("addition"));
    }

    #[test]
    fn test_bash_renderer() {
        let r = BashRenderer;
        let call = r.render_call(r#"{"description":"Run tests","command":"cargo test"}"#, "");
        assert!(call.contains("# Run tests"));
        assert!(call.contains("$ cargo test"));
    }

    #[test]
    fn test_bash_renderer_no_description() {
        let r = BashRenderer;
        let call = r.render_call(r#"{"command":"ls"}"#, "");
        assert_eq!(call, "# Shell\n$ ls");
    }

    #[test]
    fn test_extract_json_field() {
        let args = r#"{"pattern":"fn main","path":"src","count":42,"flag":true}"#;
        assert_eq!(extract_json_field(args, "pattern"), "fn main");
        assert_eq!(extract_json_field(args, "path"), "src");
        assert_eq!(extract_json_field(args, "count"), "42");
        assert_eq!(extract_json_field(args, "flag"), "true");
        assert_eq!(extract_json_field(args, "missing"), "");
        assert_eq!(extract_json_field("not json", "any"), "");
    }

    #[test]
    fn test_extract_path() {
        assert_eq!(extract_path(r#"{"path":"src/main.rs"}"#), "src/main.rs");
        assert_eq!(extract_path(r#"{"file_path":"Cargo.toml"}"#), "Cargo.toml");
        assert_eq!(extract_path(r#"{"filePath":"lib.rs"}"#), "lib.rs");
        assert_eq!(extract_path(r#"{}"#), "");
        assert_eq!(extract_path("raw-args"), "raw-args");
    }

    #[test]
    fn test_extract_path_nested_arguments() {
        assert_eq!(
            extract_path(r#"{"arguments":{"filePath":"src/main.rs"}}"#),
            "src/main.rs"
        );
        assert_eq!(
            extract_path(r#"{"function":{"arguments":{"path":"lib.rs"}}}"#),
            "lib.rs"
        );
    }

    #[test]
    fn test_edit_render_call_relative_path() {
        let r = EditRenderer;
        assert_eq!(
            r.render_call(
                r#"{"path":"/home/user/project/src/main.rs"}"#,
                "/home/user/project"
            ),
            "src/main.rs"
        );
    }

    #[test]
    fn test_edit_render_call_empty() {
        let r = EditRenderer;
        assert_eq!(r.render_call("{}", ""), "");
    }

    #[test]
    fn test_parse_hunk_header() {
        assert_eq!(parse_hunk_header("@@ -1,3 +1,4 @@"), (Some(1), Some(1)));
        assert_eq!(parse_hunk_header("@@ -5 +5 @@"), (Some(5), Some(5)));
        assert_eq!(parse_hunk_header("not a hunk"), (None, None));
    }

    #[test]
    fn test_edit_result_shows_line_numbers() {
        let r = EditRenderer;
        let theme = Theme::default();
        let result = r.render_result(
            r#"{"filePath":"test.rs"}"#,
            "@@ -1,2 +1,2 @@\n-old\n+new\n ctx",
            80,
            &theme,
        );
        let combined: String = result.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("┃"), "should use ┃ prefix: {combined}");
        assert!(
            combined.contains('1'),
            "should show line number 1: {combined}"
        );
        assert!(
            combined.contains('2'),
            "should show line number 2: {combined}"
        );
    }

    #[test]
    fn test_bash_render_call_with_workdir() {
        let r = BashRenderer;
        let call = r.render_call(
            r#"{"description":"List files","command":"ls","workdir":"src"}"#,
            "/home/user/project",
        );
        assert!(call.contains("# List files in src"));
        assert!(call.contains("$ ls"));
    }

    #[test]
    fn test_highlight_code_rust() {
        let theme = Theme::default();
        let lines = highlight_code("fn main() {\n    println!(\"hi\");\n}\n", "rs", &theme);
        assert!(!lines.is_empty(), "should produce highlighted lines");
        // At least some spans should have color (not all default)
        let combined: String = lines.iter().map(|l| l.to_string()).collect();
        assert!(combined.contains("fn"), "should contain source text");
    }

    #[test]
    fn test_highlight_code_empty() {
        let theme = Theme::default();
        assert!(highlight_code("", "rs", &theme).is_empty());
        assert!(highlight_code("code", "", &theme).is_empty());
    }

    #[test]
    fn test_highlight_code_unknown_ext() {
        let theme = Theme::default();
        let lines = highlight_code("plain text", "xyz", &theme);
        assert!(lines.is_empty(), "unknown ext should fall back to plain");
    }

    #[test]
    fn test_grep_renderer() {
        let r = GrepRenderer;
        let call = r.render_call(
            r#"{"pattern":"fn main","path":"/home/user/project/src"}"#,
            "/home/user/project",
        );
        assert!(call.contains("\"fn main\""));
        assert!(call.contains("src"));
    }

    #[test]
    fn test_format_extra_args() {
        let args = r#"{"filePath":"a.rs","limit":50,"offset":100}"#;
        let extra = format_extra_args(args, &["filePath"]);
        assert!(extra.contains("limit=50"));
        assert!(extra.contains("offset=100"));
    }

    #[test]
    fn test_relative_path() {
        assert_eq!(
            relative_path("/home/u/proj/src/main.rs", "/home/u/proj"),
            "src/main.rs"
        );
        assert_eq!(
            relative_path("/other/path/file.rs", "/home/u/proj"),
            "/other/path/file.rs"
        );
        assert_eq!(relative_path("", "/home/u/proj"), "");
    }
}
