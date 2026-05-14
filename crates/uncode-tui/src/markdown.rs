use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    if text.is_empty() {
        return vec![Line::from("")];
    }

    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(text, options);

    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_style = Style::default();
    let mut in_code_block = false;

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
                    in_code_block = true;
                    current_style = Style::default().fg(Color::Cyan);
                }
                Tag::Item => {
                    current_line.push(Span::styled("  • ", Style::default().fg(Color::DarkGray)));
                }
                Tag::List(_) => {}
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    lines.push(Line::from(""));
                    current_style = Style::default();
                }
                TagEnd::Heading(_) => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    lines.push(Line::from(""));
                    current_style = Style::default();
                }
                TagEnd::Strong | TagEnd::Emphasis => {
                    current_style = Style::default();
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    current_style = Style::default();
                }
                TagEnd::Item => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
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
                current_line.push(Span::styled(text.to_string(), current_style));
            }
            Event::Code(code) => {
                current_line.push(Span::styled(
                    code.to_string(),
                    Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 30)),
                ));
            }
            Event::SoftBreak => {
                current_line.push(Span::raw(" "));
            }
            Event::HardBreak => {
                lines.push(Line::from(current_line.clone()));
                current_line.clear();
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
