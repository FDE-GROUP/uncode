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

struct DiffFile {
    path: String,
    hunks: Vec<DiffLine>,
}

struct DiffLine {
    kind: DiffKind,
    text: String,
}

#[derive(PartialEq)]
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
        } else if line.starts_with('+') {
            current_hunks.push(DiffLine {
                kind: DiffKind::Added,
                text: line[1..].to_string(),
            });
        } else if line.starts_with('-') {
            current_hunks.push(DiffLine {
                kind: DiffKind::Removed,
                text: line[1..].to_string(),
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
