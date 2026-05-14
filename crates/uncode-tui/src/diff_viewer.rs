use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct DiffViewer {
    visible: bool,
    files: Vec<DiffFile>,
    active_index: usize,
}

#[derive(Debug)]
struct DiffFile {
    path: String,
    hunks: Vec<DiffLine>,
}

#[derive(Debug)]
struct DiffLine {
    kind: DiffKind,
    text: String,
}

#[derive(Debug, PartialEq)]
enum DiffKind {
    Added,
    Removed,
    Context,
    Header,
}

impl DiffViewer {
    pub fn new() -> Self {
        Self {
            visible: false,
            files: Vec::new(),
            active_index: 0,
        }
    }

    pub fn show(&mut self, diff_text: &str) {
        self.files = parse_diff(diff_text);
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
        if self.active_index + 1 < self.files.len() {
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
        let file = match self.files.get(self.active_index) {
            Some(f) => f,
            None => return,
        };

        let title = if self.files.len() > 1 {
            format!(
                "📄 Diff [{}/{}]: {}",
                self.active_index + 1,
                self.files.len(),
                file.path
            )
        } else {
            format!("📄 Diff: {}", file.path)
        };

        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        let max_lines = inner.height as usize;

        let lines: Vec<Line> = file
            .hunks
            .iter()
            .take(max_lines)
            .map(|dl| {
                let (color, prefix) = match dl.kind {
                    DiffKind::Added => (Color::Green, "+ "),
                    DiffKind::Removed => (Color::Red, "- "),
                    DiffKind::Header => (Color::Cyan, "@@ "),
                    DiffKind::Context => (Color::White, "  "),
                };
                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(color)),
                    Span::styled(&dl.text, Style::default().fg(color)),
                ])
            })
            .collect();

        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}

fn parse_diff(text: &str) -> Vec<DiffFile> {
    let mut files = Vec::new();
    let mut current_path = String::new();
    let mut current_hunks = Vec::new();

    for line in text.lines() {
        if line.starts_with("diff --git") {
            if !current_path.is_empty() && !current_hunks.is_empty() {
                files.push(DiffFile {
                    path: current_path.clone(),
                    hunks: std::mem::take(&mut current_hunks),
                });
            }
            current_path = line.to_string();
        } else if line.starts_with("+++ ") || line.starts_with("--- ") {
            current_path = line[4..].to_string();
        } else if line.starts_with("@@") {
            current_hunks.push(DiffLine {
                kind: DiffKind::Header,
                text: line.to_string(),
            });
        } else if let Some(rest) = line.strip_prefix('+') {
            current_hunks.push(DiffLine {
                kind: DiffKind::Added,
                text: rest.to_string(),
            });
        } else if let Some(rest) = line.strip_prefix('-') {
            current_hunks.push(DiffLine {
                kind: DiffKind::Removed,
                text: rest.to_string(),
            });
        } else if !line.is_empty() {
            current_hunks.push(DiffLine {
                kind: DiffKind::Context,
                text: line.to_string(),
            });
        }
    }

    if !current_path.is_empty() && !current_hunks.is_empty() {
        files.push(DiffFile {
            path: current_path,
            hunks: current_hunks,
        });
    }

    files
}

impl Default for DiffViewer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff_single_file() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
 }
+";
        let files = parse_diff(diff);
        assert_eq!(files.len(), 1);
        assert!(files[0].path.contains("main.rs"));
        assert!(files[0].hunks.iter().any(|h| h.kind == DiffKind::Removed));
        assert!(files[0].hunks.iter().any(|h| h.kind == DiffKind::Added));
    }

    #[test]
    fn test_parse_diff_multiple_files() {
        let diff = "\
diff --git a/a.rs b/a.rs
@@ -1 +1 @@
-old
+new
diff --git a/b.rs b/b.rs
@@ -1 +1 @@
-foo
+bar
";
        let files = parse_diff(diff);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_parse_diff_empty_input() {
        let files = parse_diff("");
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_diff_only_context() {
        let diff = "unchanged line\nanother line\n";
        let files = parse_diff(diff);
        assert!(files.is_empty()); // no diff header, no file created
    }

    #[test]
    fn test_parse_diff_no_hunks() {
        let diff = "diff --git a/newfile.rs b/newfile.rs\n--- /dev/null\n+++ b/newfile.rs\n";
        let files = parse_diff(diff);
        // Has header path but no hunks — should be empty
        assert!(files.is_empty());
    }

    #[test]
    fn test_diff_viewer_show_hide() {
        let mut viewer = DiffViewer::new();
        assert!(!viewer.is_visible());

        viewer.show("diff --git a/a.rs b/a.rs\n+added\n");
        assert!(viewer.is_visible());

        viewer.hide();
        assert!(!viewer.is_visible());
    }

    #[test]
    fn test_diff_viewer_navigation() {
        let mut viewer = DiffViewer::new();
        let multi_diff = "\
diff --git a/a.rs b/a.rs
+first
diff --git a/b.rs b/b.rs
+second
diff --git a/c.rs b/c.rs
+third
";
        viewer.show(multi_diff);
        assert_eq!(viewer.files.len(), 3);
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
}
