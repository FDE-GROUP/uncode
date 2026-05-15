use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::mem;

use crate::theme::Theme;

/// 将 Markdown 文本渲染为 ratatui Line 列表（默认主题）
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    render_markdown_with_theme(text, &Theme::default())
}

/// 将 Markdown 文本渲染为 ratatui Line 列表，使用指定主题
pub fn render_markdown_with_theme(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    if text.is_empty() {
        return vec![Line::from("")];
    }

    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(text, options);

    let mut ctx = RenderContext::new(theme);

    for event in parser {
        match event {
            Event::Start(tag) => ctx.handle_start(tag),
            Event::End(tag) => ctx.handle_end(tag),
            Event::Text(text) => ctx.push_text(&text),
            Event::Code(code) => ctx.push_inline_code(&code),
            Event::SoftBreak => ctx.current_line.push(Span::raw(" ")),
            Event::HardBreak => ctx.flush_line(),
            Event::Rule => ctx.push_rule(),
            Event::Html(html) => ctx.push_text(&html),
            Event::TaskListMarker(checked) => {
                // Replace the last "• " bullet span with a checkbox
                let marker = if checked { "☑ " } else { "☐ " };
                if let Some(pos) = ctx
                    .current_line
                    .iter()
                    .rposition(|s| s.content.as_ref() == "• ")
                {
                    ctx.current_line[pos] = Span::styled(
                        marker,
                        Style::default()
                            .fg(ctx.theme.markdown.link)
                            .add_modifier(Modifier::BOLD),
                    );
                }
            }
            _ => {}
        }
    }

    ctx.finish()
}

struct RenderContext<'a> {
    theme: &'a Theme,
    lines: Vec<Line<'static>>,
    current_line: Vec<Span<'static>>,
    current_style: Style,

    in_code_block: bool,
    code_lang: Option<String>,
    quote_depth: usize,
    table_state: Option<TableState>,
    list_depth: usize,
    link_url: Option<String>,
}

struct TableState {
    rows: Vec<Vec<Vec<Span<'static>>>>,
    current_row: Vec<Vec<Span<'static>>>,
    current_cell: Vec<Span<'static>>,
    is_header: bool,
}

impl TableState {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            current_row: Vec::new(),
            current_cell: Vec::new(),
            is_header: false,
        }
    }

    fn finish_cell(&mut self) {
        let cell = mem::take(&mut self.current_cell);
        self.current_row.push(cell);
    }

    fn finish_row(&mut self) {
        let row = mem::take(&mut self.current_row);
        if !row.is_empty() {
            self.rows.push(row);
        }
    }
}

impl<'a> RenderContext<'a> {
    fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            lines: Vec::new(),
            current_line: Vec::new(),
            current_style: Style::default(),
            in_code_block: false,
            code_lang: None,
            quote_depth: 0,
            table_state: None,
            list_depth: 0,
            link_url: None,
        }
    }

    fn handle_start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                let size_mod = match level {
                    HeadingLevel::H1 => Modifier::BOLD | Modifier::SLOW_BLINK,
                    HeadingLevel::H2 => Modifier::BOLD,
                    _ => Modifier::BOLD,
                };
                self.current_style = Style::default()
                    .fg(self.theme.markdown.heading)
                    .add_modifier(size_mod);
            }
            Tag::Strong => {
                self.current_style = self
                    .current_style
                    .fg(self.theme.markdown.bold)
                    .add_modifier(Modifier::BOLD);
            }
            Tag::Emphasis => {
                self.current_style = self
                    .current_style
                    .fg(self.theme.markdown.italic)
                    .add_modifier(Modifier::ITALIC);
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = true;
                self.code_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => Some(lang.into_string()),
                    _ => None,
                };
                self.current_style = Style::default().fg(self.theme.markdown.code_text);
                self.flush_line();

                // Top border with optional language label
                let label = self.code_lang.as_deref().unwrap_or("");
                let border = if label.is_empty() {
                    " ┌─".to_string()
                } else {
                    format!(" ┌─ {} ", label)
                };
                self.lines.push(Line::from(Span::styled(
                    border,
                    Style::default().fg(self.theme.markdown.code_block_border),
                )));
            }
            Tag::BlockQuote(_) => {
                self.quote_depth += 1;
            }
            Tag::List(_) => {
                self.list_depth += 1;
            }
            Tag::Item => {
                let indent = "  ".repeat(self.list_depth.saturating_sub(1));
                if !indent.is_empty() {
                    self.current_line.push(Span::raw(indent));
                }
                self.current_line.push(Span::styled(
                    "• ",
                    Style::default().fg(self.theme.ui.footer_text),
                ));
            }
            Tag::Link {
                link_type: _,
                dest_url,
                title: _,
                id: _,
            } => {
                // OSC 8 hyperlink: \x1b]8;;url\x1b\\ text \x1b]8;;\x1b\\
                // We store the URL to emit the closing sequence in TagEnd::Link
                let url = dest_url.to_string();
                let osc_open = format!("\x1b]8;;{}\x1b\\", url);
                self.current_line.push(Span::raw(osc_open));
                self.link_url = Some(url);
            }
            Tag::Table(_) => {
                self.table_state = Some(TableState::new());
            }
            Tag::TableHead => {
                if let Some(ref mut ts) = self.table_state {
                    ts.is_header = true;
                }
            }
            Tag::TableRow => {}
            Tag::TableCell => {
                // Cell content starts fresh
            }
            Tag::Strikethrough => {
                self.current_style = self.current_style.add_modifier(Modifier::CROSSED_OUT);
            }
            _ => {}
        }
    }

    fn handle_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => {
                self.flush_line();
                self.lines.push(Line::from(""));
                self.current_style = Style::default();
            }
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {
                self.current_style = Style::default();
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.flush_line();
                // Bottom border
                self.lines.push(Line::from(Span::styled(
                    " └─",
                    Style::default().fg(self.theme.markdown.code_block_border),
                )));
                self.lines.push(Line::from(""));
                self.current_style = Style::default();
            }
            TagEnd::BlockQuote(_) => {
                self.quote_depth = self.quote_depth.saturating_sub(1);
                if self.quote_depth == 0 {
                    self.lines.push(Line::from(""));
                }
            }
            TagEnd::List(_) => {
                self.list_depth = self.list_depth.saturating_sub(1);
                if self.list_depth == 0 {
                    self.lines.push(Line::from(""));
                }
            }
            TagEnd::Item => {
                self.flush_line();
            }
            TagEnd::Link => {
                // Close OSC 8 hyperlink
                self.current_line.push(Span::raw("\x1b]8;;\x1b\\"));
                // Append visible URL in dim style
                if let Some(url) = self.link_url.take() {
                    self.current_line.push(Span::styled(
                        format!(" ({})", url),
                        Style::default()
                            .fg(self.theme.ui.footer_text)
                            .add_modifier(Modifier::DIM),
                    ));
                }
            }
            TagEnd::Table => {
                if let Some(ts) = self.table_state.take() {
                    self.render_table(ts);
                }
            }
            TagEnd::TableHead => {
                if let Some(ref mut ts) = self.table_state {
                    ts.finish_cell();
                    ts.finish_row();
                    ts.is_header = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(ref mut ts) = self.table_state {
                    ts.finish_cell();
                    ts.finish_row();
                }
            }
            TagEnd::TableCell => {
                if let Some(ref mut ts) = self.table_state {
                    ts.finish_cell();
                }
            }
            _ => {}
        }
    }

    fn push_text(&mut self, text: &str) {
        if self.in_code_block {
            for line in text.lines() {
                let highlighted = crate::highlight::highlight_line_with_theme(
                    line,
                    self.code_lang.as_deref().unwrap_or(""),
                    self.theme,
                );
                let mut spans = vec![Span::styled(
                    " │ ",
                    Style::default().fg(self.theme.markdown.code_block_border),
                )];
                spans.extend(highlighted.spans);
                self.lines.push(Line::from(spans));
            }
            return;
        }

        // Inside table cell
        if self.table_state.is_some() {
            if let Some(ref mut ts) = self.table_state {
                ts.current_cell
                    .push(Span::styled(text.to_string(), self.current_style));
            }
            return;
        }

        // Quote prefix
        if self.quote_depth > 0 && self.current_line.is_empty() {
            self.current_line.push(Span::styled(
                "▎ ",
                Style::default().fg(self.theme.markdown.code_block_border),
            ));
        }

        if text.trim().is_empty() && self.current_line.is_empty() {
            return;
        }
        self.current_line
            .push(Span::styled(text.to_string(), self.current_style));
    }

    fn push_inline_code(&mut self, code: &str) {
        self.current_line.push(Span::styled(
            format!(" {} ", code),
            Style::default()
                .fg(self.theme.markdown.code_text)
                .bg(self.theme.markdown.code_bg),
        ));
    }

    fn push_rule(&mut self) {
        self.flush_line();
        self.lines.push(Line::from(Span::styled(
            "─".repeat(40),
            Style::default().fg(self.theme.markdown.code_block_border),
        )));
    }

    fn flush_line(&mut self) {
        if !self.current_line.is_empty() {
            self.lines
                .push(Line::from(mem::take(&mut self.current_line)));
        }
    }

    fn render_table(&mut self, ts: TableState) {
        if ts.rows.is_empty() {
            return;
        }

        let col_count = ts.rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut col_widths = vec![0usize; col_count];

        for row in &ts.rows {
            for (i, cell) in row.iter().enumerate() {
                let text: String = cell.iter().map(|s| s.content.as_ref()).collect();
                let w = unicode_width::UnicodeWidthStr::width(text.as_str());
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(w);
                }
            }
        }

        let border_color = Style::default().fg(self.theme.markdown.code_block_border);

        for (row_idx, row) in ts.rows.iter().enumerate() {
            let mut spans = vec![Span::styled("│", border_color)];
            for (col_idx, cell) in row.iter().enumerate() {
                spans.push(Span::raw(" "));
                let text: String = cell.iter().map(|s| s.content.as_ref()).collect();
                let text_width = unicode_width::UnicodeWidthStr::width(text.as_str());
                let padding = col_widths
                    .get(col_idx)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(text_width);

                let style = if row_idx == 0 {
                    Style::default()
                        .fg(self.theme.markdown.heading)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.markdown.code_text)
                };
                spans.push(Span::styled(text, style));
                spans.push(Span::raw(" ".repeat(padding)));
                spans.push(Span::raw(" "));
                spans.push(Span::styled("│", border_color));
            }
            self.lines.push(Line::from(spans));

            // Separator after header row
            if row_idx == 0 {
                let sep = format!(
                    "├{}┤",
                    col_widths
                        .iter()
                        .map(|w| "─".repeat(*w + 2))
                        .collect::<Vec<_>>()
                        .join("┼")
                );
                self.lines.push(Line::from(Span::styled(sep, border_color)));
            }
        }
        self.lines.push(Line::from(""));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        self.lines
    }
}
