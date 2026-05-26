use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use uncode_core::diff::{DiffLine, Patch};

pub struct DiffViewer {
    visible: bool,
    patches: Vec<Patch>,
    active_index: usize,
}

impl DiffViewer {
    pub fn new() -> Self {
        Self {
            visible: false,
            patches: Vec::new(),
            active_index: 0,
        }
    }

    pub fn show(&mut self, patch: Patch) {
        self.patches = vec![patch];
        self.visible = true;
        self.active_index = 0;
    }

    pub fn show_multi(&mut self, patches: Vec<Patch>) {
        self.patches = patches;
        self.visible = true;
        self.active_index = 0;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn next_file(&mut self) {
        if self.active_index + 1 < self.patches.len() {
            self.active_index += 1;
        }
    }

    pub fn prev_file(&mut self) {
        if self.active_index > 0 {
            self.active_index -= 1;
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }
        let patch = match self.patches.get(self.active_index) {
            Some(p) => p,
            None => return,
        };

        let title = if self.patches.len() > 1 {
            format!(
                " Diff [{}/{}]: {}",
                self.active_index + 1,
                self.patches.len(),
                patch.path
            )
        } else {
            format!(" Diff: {}", patch.path)
        };

        let block = Block::bordered().title(title);
        let inner = block.inner(area);
        let max_lines = inner.height as usize;

        let lines: Vec<Line> = patch
            .hunks
            .iter()
            .flat_map(|hunk| {
                let header = Line::from(Span::styled(
                    format!(
                        "@@ -{},{} +{},{} @@",
                        hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
                    ),
                    Style::default().cyan(),
                ));
                let content_lines = hunk.lines.iter().map(|dl| {
                    let (color, prefix) = match dl {
                        DiffLine::Added { .. } => (Color::Green, "+ "),
                        DiffLine::Removed { .. } => (Color::Red, "- "),
                        DiffLine::Context { .. } => (Color::White, "  "),
                    };
                    Line::from(vec![
                        Span::styled(prefix, Style::default().fg(color)),
                        Span::styled(dl.text().to_string(), Style::default().fg(color)),
                    ])
                });
                std::iter::once(header).chain(content_lines)
            })
            .take(max_lines)
            .collect();

        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}

impl Default for DiffViewer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use uncode_core::diff::Hunk;

    #[test]
    fn test_render_shows_diff() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let patch = Patch {
            path: "test.rs".into(),
            hunks: vec![Hunk {
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 1,
                lines: vec![DiffLine::Context {
                    text: "hello".into(),
                    old_line: 1,
                    new_line: 1,
                }],
            }],
            old_bytes: 6,
            new_bytes: 6,
        };
        let mut viewer = DiffViewer::new();
        viewer.show(patch);
        terminal
            .draw(|f| {
                viewer.render(f, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("hello"));
    }

    #[test]
    fn test_render_hidden_is_empty() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut viewer = DiffViewer::new();
        viewer.hide();
        terminal
            .draw(|f| {
                viewer.render(f, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.chars().all(|c| c == ' '));
    }

    fn make_patch(old: &str, new: &str, path: &str) -> Patch {
        Patch::compute(old, new, path)
    }

    #[test]
    fn test_diff_viewer_single_file() {
        let patch = make_patch("old\n", "new\n", "main.rs");
        assert!(!patch.is_empty());
        assert_eq!(patch.hunks.len(), 1);
        assert_eq!(patch.stats().removed, 1);
        assert_eq!(patch.stats().added, 1);
    }

    #[test]
    fn test_diff_viewer_show_hide() {
        let mut viewer = DiffViewer::new();
        assert!(!viewer.is_visible());

        let patch = make_patch("a\n", "b\n", "f.txt");
        viewer.show(patch);
        assert!(viewer.is_visible());

        viewer.hide();
        assert!(!viewer.is_visible());
    }

    #[test]
    fn test_diff_viewer_multi_file_navigation() {
        let mut viewer = DiffViewer::new();
        let p1 = make_patch("a\n", "b\n", "a.rs");
        let p2 = make_patch("c\n", "d\n", "b.rs");
        let p3 = make_patch("e\n", "f\n", "c.rs");
        viewer.show_multi(vec![p1, p2, p3]);
        assert_eq!(viewer.patches.len(), 3);
        assert_eq!(viewer.active_index, 0);

        viewer.next_file();
        assert_eq!(viewer.active_index, 1);

        viewer.next_file();
        assert_eq!(viewer.active_index, 2);

        viewer.next_file(); // at end, should not overflow
        assert_eq!(viewer.active_index, 2);

        viewer.prev_file();
        assert_eq!(viewer.active_index, 1);

        viewer.prev_file();
        assert_eq!(viewer.active_index, 0);

        viewer.prev_file(); // at start
        assert_eq!(viewer.active_index, 0);
    }

    #[test]
    fn test_diff_viewer_empty_patch() {
        let patch = make_patch("same\n", "same\n", "f.txt");
        assert!(patch.is_empty());
        let mut viewer = DiffViewer::new();
        viewer.show(patch);
        assert!(viewer.is_visible());
        // Empty patch has no hunks, render should be a no-op visually
    }

    #[test]
    fn test_diff_viewer_multiple_hunks() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\nP\n";
        let patch = make_patch(old, new, "multi.txt");
        assert!(
            patch.hunks.len() >= 2,
            "expected >= 2 hunks, got {}",
            patch.hunks.len()
        );
    }
}
