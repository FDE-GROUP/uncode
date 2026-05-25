use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use unicode_width::UnicodeWidthStr;

pub struct CodeDetailView {
    visible: bool,
    fullscreen: bool,
    content: Vec<CodeLine>,
    title: String,
}

#[derive(Clone)]
struct CodeLine {
    text: String,
    line_no: usize,
    kind: LineKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Normal,
    Added,
    Removed,
    Header,
}

impl CodeDetailView {
    pub fn new() -> Self {
        Self {
            visible: false,
            fullscreen: false,
            content: Vec::new(),
            title: String::new(),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
    }

    pub fn show_file(&mut self, path: &str, content: &str) {
        self.title = path.to_string();
        let lang = crate::highlight::detect_language_from_path(path);
        let highlighted = crate::highlight::highlight_code(content, lang.unwrap_or(""));

        self.content = highlighted
            .iter()
            .enumerate()
            .map(|(i, line)| CodeLine {
                text: line.spans.iter().map(|s| s.content.clone()).collect(),
                line_no: i + 1,
                kind: LineKind::Normal,
            })
            .collect();
        self.visible = true;
    }

    pub fn show_diff(&mut self, title: &str, diff_text: &str) {
        self.title = title.to_string();
        self.content = diff_text
            .lines()
            .enumerate()
            .map(|(i, line)| {
                let kind = if line.starts_with('+') && !line.starts_with("+++") {
                    LineKind::Added
                } else if line.starts_with('-') && !line.starts_with("---") {
                    LineKind::Removed
                } else if line.starts_with("@@") {
                    LineKind::Header
                } else {
                    LineKind::Normal
                };
                CodeLine {
                    text: line.to_string(),
                    line_no: i + 1,
                    kind,
                }
            })
            .collect();
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.content.clear();
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let area = if self.fullscreen { f.area() } else { area };

        let block_title = if self.fullscreen {
            format!("📄 {} [全屏]", self.title)
        } else {
            format!("📄 代码: {}", self.title)
        };

        let block = Block::bordered().title(block_title);

        let inner = block.inner(area);
        let max_lines = inner.height as usize;
        let max_width = inner.width as usize;

        let lines: Vec<Line> = self
            .content
            .iter()
            .take(max_lines)
            .map(|cl| {
                let line_no = Span::styled(
                    format!("{:>4} ", cl.line_no),
                    Style::default().dark_gray(),
                );

                let (fg, bg) = match cl.kind {
                    LineKind::Added => (Color::Green, Color::Rgb(0, 40, 0)),
                    LineKind::Removed => (Color::Red, Color::Rgb(40, 0, 0)),
                    LineKind::Header => (Color::Cyan, Color::Reset),
                    LineKind::Normal => (Color::White, Color::Reset),
                };

                let display_text = if cl.text.width() > max_width.saturating_sub(6) {
                    &cl.text[..cl.text.len().min(max_width.saturating_sub(6))]
                } else {
                    &cl.text
                };

                let content_span =
                    Span::styled(display_text.to_string(), Style::default().fg(fg).bg(bg));

                Line::from(vec![line_no, content_span])
            })
            .collect();

        let paragraph = Paragraph::new(lines).block(block);
        f.render_widget(paragraph, area);
    }
}

impl Default for CodeDetailView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_not_visible() {
        let v = CodeDetailView::new();
        assert!(!v.is_visible());
        assert!(!v.fullscreen);
    }

    #[test]
    fn test_toggle() {
        let mut v = CodeDetailView::new();
        v.toggle();
        assert!(v.is_visible());
        v.toggle();
        assert!(!v.is_visible());
    }

    #[test]
    fn test_toggle_fullscreen() {
        let mut v = CodeDetailView::new();
        v.toggle_fullscreen();
        assert!(v.fullscreen);
        v.toggle_fullscreen();
        assert!(!v.fullscreen);
    }

    #[test]
    fn test_hide() {
        let mut v = CodeDetailView::new();
        v.toggle();
        v.hide();
        assert!(!v.is_visible());
    }

    #[test]
    fn test_show_diff_parses_added_lines() {
        let mut v = CodeDetailView::new();
        v.show_diff("test.diff", "+added\n-removed\n@@ -1,1 +1,1 @@\n normal");
        assert!(v.is_visible());
        assert_eq!(v.content.len(), 4);
        assert_eq!(v.content[0].kind, LineKind::Added);
        assert_eq!(v.content[1].kind, LineKind::Removed);
        assert_eq!(v.content[2].kind, LineKind::Header);
        assert_eq!(v.content[3].kind, LineKind::Normal);
    }

    #[test]
    fn test_show_diff_skips_plus_plus_plus() {
        let mut v = CodeDetailView::new();
        v.show_diff("test.diff", "+++ b/new.rs\n+a");
        assert!(v.is_visible());
        // +++ is Normal, +a is Added
        assert_eq!(v.content[0].kind, LineKind::Normal);
        assert_eq!(v.content[1].kind, LineKind::Added);
    }

    #[test]
    fn test_show_diff_empty() {
        let mut v = CodeDetailView::new();
        v.show_diff("empty.diff", "");
        assert!(v.is_visible());
        assert!(v.content.is_empty());
    }

    #[test]
    fn test_show_diff_title() {
        let mut v = CodeDetailView::new();
        v.show_diff("my.diff", "+x");
        assert_eq!(v.title, "my.diff");
    }
}
