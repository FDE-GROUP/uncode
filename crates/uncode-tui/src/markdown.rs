use markdown::mdast::{self, Node};
use markdown::to_mdast;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    render_markdown_with_theme(text, &Theme::default(), None)
}

pub fn render_markdown_with_theme(
    text: &str,
    theme: &Theme,
    max_width: Option<usize>,
) -> Vec<Line<'static>> {
    if text.is_empty() {
        return vec![Line::from("")];
    }

    let ast = match to_mdast(text, &markdown::ParseOptions::gfm()) {
        Ok(node) => node,
        Err(_) => return text.lines().map(|l| Line::from(l.to_string())).collect(),
    };

    let mut ctx = RenderContext::new(theme, max_width);
    ctx.render_node(&ast);
    ctx.finish()
}

/// A prefix pushed onto the stack for continuation indentation after wrapping.
struct Prefix {
    text: String,
    width: usize,
}

struct RenderContext<'a> {
    theme: &'a Theme,
    max_width: Option<usize>,
    lines: Vec<Line<'static>>,
    current_line: Vec<Span<'static>>,
    current_width: usize,
    current_style: Style,
    prefix_stack: Vec<Prefix>,
    list_ordered: bool,
    list_counter: u64,
}

impl<'a> RenderContext<'a> {
    fn new(theme: &'a Theme, max_width: Option<usize>) -> Self {
        Self {
            theme,
            max_width,
            lines: Vec::new(),
            current_line: Vec::new(),
            current_width: 0,
            current_style: Style::default(),
            prefix_stack: Vec::new(),
            list_ordered: false,
            list_counter: 1,
        }
    }

    /// Push styled text into the current line, with word-level wrapping.
    fn push_wrapped(&mut self, text: &str, style: Style) {
        if text.is_empty() {
            return;
        }

        let max = match self.max_width {
            Some(w) if w > 0 => w,
            _ => {
                // No wrapping: just push directly
                let w = UnicodeWidthStr::width(text);
                self.current_line
                    .push(Span::styled(text.to_string(), style));
                self.current_width += w;
                return;
            }
        };

        for word in text.split_whitespace() {
            let word_w = UnicodeWidthStr::width(word);
            let need_space = !self.current_line.is_empty();
            let space_w = if need_space { 1 } else { 0 };
            let total = self.current_width + space_w + word_w;

            if total > max && !self.current_line.is_empty() {
                // Wrap current line
                self.flush_line();
                // Add continuation prefixes
                for prefix in &self.prefix_stack {
                    self.current_line.push(Span::raw(prefix.text.clone()));
                    self.current_width += prefix.width;
                }
            } else {
                // Add space before word (if not at line start)
                if need_space {
                    self.current_line.push(Span::styled(" ".to_string(), style));
                    self.current_width += 1;
                }
            }

            self.current_line
                .push(Span::styled(word.to_string(), style));
            self.current_width += word_w;
        }
    }

    fn render_node(&mut self, node: &Node) {
        match node {
            Node::Root(root) => {
                for child in &root.children {
                    self.render_node(child);
                }
            }

            // --- Block containers ---
            Node::Paragraph(para) => {
                for child in &para.children {
                    self.render_inline(child);
                }
                self.flush_line();
                self.lines.push(Line::from(""));
            }

            Node::Heading(heading) => {
                let mod_flag = match heading.depth {
                    1 => Modifier::BOLD | Modifier::SLOW_BLINK,
                    _ => Modifier::BOLD,
                };
                self.current_style = Style::default()
                    .fg(self.theme.markdown.heading)
                    .add_modifier(mod_flag);
                for child in &heading.children {
                    self.render_inline(child);
                }
                self.flush_line();
                self.lines.push(Line::from(""));
                self.current_style = Style::default();
            }

            Node::Blockquote(bq) => {
                let depth = self
                    .prefix_stack
                    .iter()
                    .filter(|p| p.text.contains('▎'))
                    .count();
                self.prefix_stack.push(Prefix {
                    text: "▎ ".to_string(),
                    width: 2,
                });
                // Add blockquote prefix to current line
                self.current_line.push(Span::styled(
                    "▎ ".to_string(),
                    Style::default().fg(self.theme.markdown.code_block_border),
                ));
                self.current_width += 2;
                for child in &bq.children {
                    self.render_node(child);
                }
                self.prefix_stack.pop();
                if depth == 0 {
                    self.lines.push(Line::from(""));
                }
            }

            Node::List(list) => {
                let prev_ordered = self.list_ordered;
                let prev_counter = self.list_counter;
                self.list_ordered = list.ordered;
                self.list_counter = list.start.unwrap_or(1) as u64;
                let indent = if list.ordered { "   " } else { "  " };
                self.prefix_stack.push(Prefix {
                    text: indent.to_string(),
                    width: indent.len(),
                });
                for child in &list.children {
                    self.render_node(child);
                }
                self.prefix_stack.pop();
                self.list_ordered = prev_ordered;
                self.list_counter = prev_counter;
                if self.prefix_stack.is_empty()
                    || !self.prefix_stack.iter().any(|p| p.text.contains('▎'))
                {
                    self.lines.push(Line::from(""));
                }
            }

            Node::ListItem(item) => {
                let marker = if let Some(checked) = item.checked {
                    let m = if checked { "x " } else { "o " };
                    m.to_string()
                } else if self.list_ordered {
                    let m = format!("{}. ", self.list_counter);
                    self.list_counter += 1;
                    m
                } else {
                    "• ".to_string()
                };
                self.current_line.push(Span::styled(
                    marker,
                    Style::default().fg(self.theme.ui.footer_text),
                ));
                for child in &item.children {
                    self.render_node(child);
                }
                self.flush_line();
            }

            // --- Leaf blocks ---
            Node::Code(code) => {
                self.render_code_block(code.lang.as_deref(), &code.value);
            }

            Node::ThematicBreak(_) => {
                self.flush_line();
                self.lines.push(Line::from(""));
            }

            Node::Html(html) => {
                self.push_text(&html.value);
            }

            // --- Table ---
            Node::Table(table) => {
                self.render_table(table);
            }

            Node::Break(_) => {
                self.flush_line();
            }

            _ => {}
        }
    }

    /// Render inline (phrasing) content with current style context
    fn render_inline(&mut self, node: &Node) {
        match node {
            Node::Text(text) => self.push_text(&text.value),
            Node::InlineCode(code) => self.push_inline_code(&code.value),
            Node::Strong(strong) => {
                let prev = self.current_style;
                self.current_style = prev
                    .fg(self.theme.markdown.bold)
                    .add_modifier(Modifier::BOLD);
                for child in &strong.children {
                    self.render_inline(child);
                }
                self.current_style = prev;
            }
            Node::Emphasis(em) => {
                let prev = self.current_style;
                self.current_style = prev
                    .fg(self.theme.markdown.italic)
                    .add_modifier(Modifier::ITALIC);
                for child in &em.children {
                    self.render_inline(child);
                }
                self.current_style = prev;
            }
            Node::Delete(del) => {
                let prev = self.current_style;
                self.current_style = prev.add_modifier(Modifier::CROSSED_OUT);
                for child in &del.children {
                    self.render_inline(child);
                }
                self.current_style = prev;
            }
            Node::Link(link) => {
                // Render link text with underline, then show URL
                let link_style = Style::default()
                    .fg(self.theme.markdown.link)
                    .add_modifier(Modifier::UNDERLINED);
                let prev = self.current_style;
                self.current_style = link_style;
                for child in &link.children {
                    self.render_inline(child);
                }
                self.current_style = prev;
                if !link.url.is_empty() {
                    self.current_line.push(Span::styled(
                        format!(" ({})", link.url),
                        Style::default().fg(self.theme.markdown.link),
                    ));
                }
            }
            Node::Html(html) => self.push_text(&html.value),
            Node::Break(_) => self.flush_line(),
            _ => {
                if let Some(children) = node.children() {
                    for child in children {
                        self.render_inline(child);
                    }
                }
            }
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.trim().is_empty() && self.current_line.is_empty() {
            return;
        }
        self.push_wrapped(text, self.current_style);
    }

    fn push_inline_code(&mut self, code: &str) {
        self.current_line.push(Span::styled(
            format!(" {code} "),
            Style::default().fg(self.theme.markdown.code_text),
        ));
    }

    fn render_code_block(&mut self, lang: Option<&str>, code: &str) {
        self.flush_line();
        let lang_str = lang.unwrap_or("");
        let header = if lang_str.is_empty() {
            String::new()
        } else {
            format!(" {lang_str}")
        };
        if !header.is_empty() {
            self.lines.push(Line::from(Span::styled(
                header,
                Style::default().fg(self.theme.markdown.code_block_border),
            )));
        }

        let code_max = self.max_width;

        for line in code.lines() {
            let expanded = expand_tabs(line);
            let highlighted =
                crate::highlight::highlight_line_with_theme(&expanded, lang_str, self.theme);

            if let Some(max) = code_max {
                let line_w: usize = highlighted
                    .spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                if line_w > max {
                    for wrapped in wrap_spans(&highlighted.spans, max) {
                        self.lines.push(Line::from(wrapped));
                    }
                    continue;
                }
            }

            self.lines.push(Line::from(highlighted.spans));
        }
        self.lines.push(Line::from(""));
    }

    fn render_table(&mut self, table: &mdast::Table) {
        struct CellContent {
            text: String,
            width: usize,
        }

        let mut rows: Vec<Vec<CellContent>> = Vec::new();
        for child in &table.children {
            if let Node::TableRow(tr) = child {
                let mut cells: Vec<CellContent> = Vec::new();
                for cell_node in &tr.children {
                    if let Node::TableCell(cell) = cell_node {
                        let mut spans = Vec::new();
                        for inline in &cell.children {
                            collect_inline_spans(
                                inline,
                                Style::default(),
                                &mut spans,
                                self.theme.markdown.code_text,
                            );
                        }
                        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
                        let width = UnicodeWidthStr::width(text.as_str());
                        cells.push(CellContent { text, width });
                    }
                }
                rows.push(cells);
            }
        }

        if rows.is_empty() {
            return;
        }

        let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut col_widths = vec![0usize; col_count];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.width);
                }
            }
        }

        let border_color = Style::default().fg(self.theme.markdown.code_block_border);
        for (row_idx, row) in rows.iter().enumerate() {
            let mut spans = vec![Span::styled("│", border_color)];
            for (col_idx, cell) in row.iter().enumerate() {
                spans.push(Span::raw(" "));
                let padding = col_widths
                    .get(col_idx)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(cell.width);
                let style = if row_idx == 0 {
                    Style::default()
                        .fg(self.theme.markdown.heading)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.markdown.code_text)
                };
                spans.push(Span::styled(cell.text.clone(), style));
                spans.push(Span::raw(" ".repeat(padding)));
                spans.push(Span::raw(" "));
                spans.push(Span::styled("│", border_color));
            }
            self.lines.push(Line::from(spans));
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

    fn flush_line(&mut self) {
        if !self.current_line.is_empty() {
            self.lines
                .push(Line::from(std::mem::take(&mut self.current_line)));
            self.current_width = 0;
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        self.lines
    }
}

/// Expand tabs to spaces, aligned to 4-column tab stops.
fn expand_tabs(s: &str) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len() + 8);
    let mut col = 0usize;
    for ch in s.chars() {
        if ch == '\t' {
            let spaces = 4 - (col % 4);
            result.push_str(&" ".repeat(spaces));
            col += spaces;
        } else {
            result.push(ch);
            col += UnicodeWidthStr::width(ch.to_string().as_str());
        }
    }
    result
}

/// Hard-wrap a sequence of styled spans at `max` display width.
/// Returns one or more span vectors, each fitting within `max`.
/// Adjacent graphemes with the same style are merged into a single Span.
fn wrap_spans(spans: &[Span<'static>], max: usize) -> Vec<Vec<Span<'static>>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![vec![]];
    let mut current_width = 0usize;
    let mut buf = String::new();
    let mut buf_style = Style::default();

    let flush_buf = |buf: &mut String, style: Style, line: &mut Vec<Span<'static>>| {
        if !buf.is_empty() {
            line.push(Span::styled(std::mem::take(buf), style));
        }
    };

    for span in spans {
        let span_style = span.style;
        let text: &str = &span.content;

        // If style changed, flush the current buffer first
        if span_style != buf_style {
            flush_buf(&mut buf, buf_style, lines.last_mut().unwrap());
            buf_style = span_style;
        }

        for grapheme in UnicodeSegmentation::graphemes(text, true) {
            let gw = UnicodeWidthStr::width(grapheme);
            if current_width + gw > max && !lines.last().unwrap().is_empty() {
                // Flush pending buffer before starting new line
                flush_buf(&mut buf, buf_style, lines.last_mut().unwrap());
                lines.push(vec![]);
                current_width = 0;
            }
            buf.push_str(grapheme);
            current_width += gw;
        }
    }

    // Flush remaining buffer
    flush_buf(&mut buf, buf_style, lines.last_mut().unwrap());

    lines
}

/// Recursively collect inline node text into styled spans (for table cells)
fn collect_inline_spans(
    node: &Node,
    style: Style,
    out: &mut Vec<Span<'static>>,
    code_color: ratatui::style::Color,
) {
    match node {
        Node::Text(text) => {
            out.push(Span::styled(text.value.clone(), style));
        }
        Node::InlineCode(code) => {
            out.push(Span::styled(
                format!(" {} ", code.value),
                Style::default().fg(code_color),
            ));
        }
        Node::Strong(strong) => {
            let s = style.add_modifier(Modifier::BOLD);
            for child in &strong.children {
                collect_inline_spans(child, s, out, code_color);
            }
        }
        Node::Emphasis(em) => {
            let s = style.add_modifier(Modifier::ITALIC);
            for child in &em.children {
                collect_inline_spans(child, s, out, code_color);
            }
        }
        Node::Delete(del) => {
            let s = style.add_modifier(Modifier::CROSSED_OUT);
            for child in &del.children {
                collect_inline_spans(child, s, out, code_color);
            }
        }
        _ => {
            if let Some(children) = node.children() {
                for child in children {
                    collect_inline_spans(child, style, out, code_color);
                }
            }
        }
    }
}
