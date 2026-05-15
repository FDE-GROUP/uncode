use uncode_core::message::{ContentBlock, Role};
use uncode_core::session::{SessionEntry, SessionHeader};

/// 将会话数据导出为自包含 HTML
pub fn export_html(
    header: &SessionHeader,
    _entries: &[SessionEntry],
    messages: &[(Role, Vec<ContentBlock>)],
) -> String {
    let title = header.title.as_deref().unwrap_or("uncode 会话");
    let date = header.created_at.format("%Y-%m-%d %H:%M UTC").to_string();

    let mut body = String::new();

    // Metadata
    body.push_str(&format!(
        r#"<div class="meta">
<span class="label">会话</span> {title}
<span class="label">日期</span> {date}
<span class="label">模型</span> {model}
<span class="label">会话ID</span> {id}
</div>
"#,
        title = html_escape(title),
        date = date,
        model = html_escape(&header.model),
        id = html_escape(&header.id),
    ));

    for (role, blocks) in messages {
        let class = match role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
            Role::System => "system",
            _ => "other",
        };
        body.push_str(&format!(r#"<div class="msg {class}">"#));
        body.push_str(&format!(
            r#"<div class="msg-header">{}</div>"#,
            match role {
                Role::User => "用户",
                Role::Assistant => "助手",
                Role::Tool => "工具",
                Role::System => "系统",
                _ => "其他",
            }
        ));

        for block in blocks {
            match block {
                ContentBlock::Text { text } => {
                    body.push_str(&format!(
                        r#"<div class="text">{}</div>"#,
                        render_markdown_lite(text)
                    ));
                }
                ContentBlock::Thinking { text } => {
                    body.push_str(&format!(
                        r#"<details><summary>思考过程</summary><div class="thinking">{}</div></details>"#,
                        html_escape(text)
                    ));
                }
                ContentBlock::ToolCall(tc) => {
                    body.push_str(&format!(
                        r#"<details><summary>工具调用: {}</summary><div class="tool-call"><pre><code>{}</code></pre></div></details>"#,
                        html_escape(&tc.name),
                        html_escape(&serde_json::to_string_pretty(&tc.arguments).unwrap_or_default()),
                    ));
                }
                ContentBlock::ToolResult(tr) => {
                    let status = if tr.is_error { "error" } else { "result" };
                    body.push_str(&format!(
                        r#"<details><summary>工具结果 ({status})</summary><div class="tool-result"><pre><code>{}</code></pre></div></details>"#,
                        html_escape(&tr.content),
                    ));
                }
                ContentBlock::Image { .. } => {
                    body.push_str(r#"<div class="text">[图片]</div>"#);
                }
                _ => {}
            }
        }
        body.push_str("</div>\n");
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 900px; margin: 0 auto; padding: 20px; background: #fff; color: #1a1a1a; line-height: 1.6; }}
.meta {{ background: #f6f8fa; padding: 12px 16px; border-radius: 8px; margin-bottom: 24px; font-size: 14px; }}
.meta .label {{ display: inline-block; width: 60px; color: #656d76; font-weight: 600; }}
.msg {{ margin-bottom: 16px; border-radius: 8px; overflow: hidden; }}
.msg.user {{ background: #ddf4ff; }}
.msg.assistant {{ background: #f6f8fa; }}
.msg.tool {{ background: #fff8c5; }}
.msg-header {{ padding: 6px 12px; font-size: 13px; font-weight: 600; color: #656d76; border-bottom: 1px solid #d0d7de; }}
.msg .text {{ padding: 12px; }}
.msg pre {{ background: #1b1f24; color: #e6edf3; padding: 12px; border-radius: 6px; overflow-x: auto; font-size: 13px; }}
.msg code {{ font-family: 'SFMono-Regular', Consolas, monospace; }}
.thinking {{ padding: 12px; color: #656d76; font-style: italic; font-size: 14px; white-space: pre-wrap; }}
.tool-call, .tool-result {{ padding: 8px 12px; }}
details {{ margin: 4px 0; }}
summary {{ cursor: pointer; padding: 6px 12px; font-size: 13px; color: #0969da; }}
summary:hover {{ background: #f6f8fa; }}
.error {{ color: #cf222e; }}
</style>
</head>
<body>
<h1>{title}</h1>
{body}
</body>
</html>"#,
        title = html_escape(title),
        body = body,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 轻量 Markdown → HTML（仅处理代码块和加粗）
fn render_markdown_lite(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_code_block = false;
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if line.starts_with("```") {
            if in_code_block {
                result.push_str("</code></pre>");
                in_code_block = false;
            } else {
                result.push_str("<pre><code>");
                in_code_block = true;
            }
            result.push('\n');
        } else if in_code_block {
            result.push_str(&html_escape(line));
            result.push('\n');
        } else {
            let escaped = html_escape(line);
            // bold
            let rendered = escaped.replace("**", "<strong>").replace("</strong>", "");
            result.push_str(&rendered);
            result.push_str("<br>\n");
        }
    }

    if in_code_block {
        result.push_str("</code></pre>");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_html_basic() {
        let header = SessionHeader::new("test-id".into(), "test-model".into(), "/tmp".into());
        let messages = vec![(
            Role::User,
            vec![ContentBlock::Text {
                text: "Hello".into(),
            }],
        )];
        let html = export_html(&header, &[], &messages);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("test-id"));
        assert!(html.contains("Hello"));
        assert!(html.contains("class=\"msg user\""));
    }

    #[test]
    fn test_export_html_with_thinking() {
        let header = SessionHeader::new("test-id".into(), "test-model".into(), "/tmp".into());
        let messages = vec![(
            Role::Assistant,
            vec![
                ContentBlock::Thinking {
                    text: "thinking...".into(),
                },
                ContentBlock::Text {
                    text: "response".into(),
                },
            ],
        )];
        let html = export_html(&header, &[], &messages);
        assert!(html.contains("<details>"));
        assert!(html.contains("思考过程"));
        assert!(html.contains("thinking..."));
        assert!(html.contains("response"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a&b"), "a&amp;b");
    }

    #[test]
    fn test_render_markdown_lite_code_block() {
        let input = "before\n```rust\nfn main() {}\n```\nafter";
        let rendered = render_markdown_lite(input);
        assert!(rendered.contains("<pre><code>"));
        assert!(rendered.contains("fn main()"));
        assert!(rendered.contains("</code></pre>"));
    }
}
