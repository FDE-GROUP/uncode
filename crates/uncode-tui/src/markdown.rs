use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::mem;

/// 将 Markdown 文本渲染为 ratatui Line 列表
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    if text.is_empty() {
        return vec![Line::from("")];
    }

    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(text, options);

    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_style = Style::default();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => {
                    current_style = current_style.add_modifier(Modifier::BOLD);
                    if level == HeadingLevel::H1 {
                        current_style = current_style.fg(Color::Yellow);
                    }
                }
                Tag::Strong => current_style = current_style.add_modifier(Modifier::BOLD),
                Tag::Emphasis => current_style = current_style.add_modifier(Modifier::ITALIC),
                Tag::CodeBlock(_) => {
                    current_style = Style::default().fg(Color::Cyan);
                }
                Tag::Item => {
                    current_line.push(Span::styled("  • ", Style::default().fg(Color::DarkGray)));
                }
                Tag::List(_) => {}
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph | TagEnd::Heading(_) => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(mem::take(&mut current_line)));
                    }
                    lines.push(Line::from(""));
                    current_style = Style::default();
                }
                TagEnd::Strong | TagEnd::Emphasis => {
                    current_style = Style::default();
                }
                TagEnd::CodeBlock => {
                    current_style = Style::default();
                }
                TagEnd::Item =>
                {
                    #[allow(clippy::collapsible_match)]
                    if !current_line.is_empty() {
                        lines.push(Line::from(mem::take(&mut current_line)));
                    }
                }
                TagEnd::List(_) => {
                    lines.push(Line::from(""));
                }
                _ => {}
            },
            Event::Text(text) => {
                if text.trim().is_empty() && current_line.is_empty() {
                    continue;
                }
                current_line.push(Span::styled(text.into_string(), current_style));
            }
            Event::Code(code) => {
                current_line.push(Span::styled(
                    code.into_string(),
                    Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 30)),
                ));
            }
            Event::SoftBreak => {
                current_line.push(Span::raw(" "));
            }
            Event::HardBreak => {
                lines.push(Line::from(mem::take(&mut current_line)));
            }
            Event::Rule => {
                lines.push(Line::from("───"));
            }
            _ => {}
        }
    }

    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    lines
}
