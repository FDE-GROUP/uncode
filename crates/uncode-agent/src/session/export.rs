use std::fmt::Write;

use uncode_core::message::{ContentBlock, Role};
use uncode_core::session::{SessionEntry, SessionHeader};

fn role_css_class(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
        Role::System => "system",
        _ => "other",
    }
}

fn role_display(role: Role) -> &'static str {
    match role {
        Role::User => "用户",
        Role::Assistant => "助手",
        Role::Tool => "工具",
        Role::System => "系统",
        _ => "其他",
    }
}

fn write_message_div(body: &mut String, role: Role, content: impl FnOnce(&mut String)) {
    let _ = write!(
        body,
        r#"<div class="msg {}"><div class="msg-header">{}</div>"#,
        role_css_class(role),
        role_display(role)
    );
    content(body);
    body.push_str("</div>\n");
}

/// 将会话数据导出为自包含 HTML
pub fn export_html(
    header: &SessionHeader,
    entries: &[SessionEntry],
    messages: &[(Role, Vec<ContentBlock>)],
) -> String {
    let title = header.title.as_deref().unwrap_or("uncode 会话");
    let date = header.created_at.format("%Y-%m-%d %H:%M UTC").to_string();

    let mut body = String::new();

    let _ = write!(
        body,
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
    );

    for entry in entries {
        match entry {
            SessionEntry::Message(me) => {
                write_message_div(&mut body, me.role, |b| {
                    for block in &me.content {
                        render_block(b, block);
                    }
                });
            }
            SessionEntry::Compaction(ce) => {
                let _ = write!(
                    body,
                    r#"<div class="msg system"><div class="msg-header">上下文压缩</div><div class="text"><em>{}</em></div></div>"#,
                    html_escape(&ce.summary)
                );
            }
            SessionEntry::BranchSummary(bs) => {
                let _ = write!(
                    body,
                    r#"<div class="msg system"><div class="msg-header">分支摘要</div><div class="text"><em>{}</em></div></div>"#,
                    html_escape(&bs.summary)
                );
            }
            SessionEntry::ModelChange(mc) => {
                let _ = write!(
                    body,
                    r#"<div class="msg system"><div class="msg-header">模型切换</div><div class="text">→ {}</div></div>"#,
                    html_escape(&mc.model_id)
                );
            }
            SessionEntry::ThinkingLevelChange(tl) => {
                let _ = write!(
                    body,
                    r#"<div class="msg system"><div class="msg-header">思考等级切换</div><div class="text">→ {:?}</div></div>"#,
                    tl.thinking_level
                );
            }
            SessionEntry::DecisionAudit(da) => {
                let _ = write!(
                    body,
                    r#"<div class="msg system"><div class="msg-header">决策审计</div><div class="text"><em>{}: {}</em></div></div>"#,
                    html_escape(&da.tool_name),
                    html_escape(da.reason.as_deref().unwrap_or("-"))
                );
            }
            _ => {}
        }
    }

    if entries.is_empty() && !messages.is_empty() {
        for (role, blocks) in messages {
            write_message_div(&mut body, *role, |b| {
                for block in blocks {
                    render_block(b, block);
                }
            });
        }
    }

    let mut html = String::new();
    let _ = write!(
        html,
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
    );
    html
}

fn render_block(body: &mut String, block: &ContentBlock) {
    match block {
        ContentBlock::Text { text } => {
            let _ = write!(
                body,
                r#"<div class="text">{}</div>"#,
                render_markdown_lite(text)
            );
        }
        ContentBlock::Thinking { text } => {
            let _ = write!(
                body,
                r#"<details><summary>思考过程</summary><div class="thinking">{}</div></details>"#,
                html_escape(text)
            );
        }
        ContentBlock::ToolCall(tc) => {
            let args = serde_json::to_string_pretty(&tc.arguments).unwrap_or_default();
            let _ = write!(
                body,
                r#"<details><summary>工具调用: {}</summary><div class="tool-call"><pre><code>{}</code></pre></div></details>"#,
                html_escape(&tc.name),
                html_escape(&args),
            );
        }
        ContentBlock::ToolResult(tr) => {
            let status = if tr.is_error { "error" } else { "result" };
            let _ = write!(
                body,
                r#"<details><summary>工具结果 ({status})</summary><div class="tool-result"><pre><code>{}</code></pre></div></details>"#,
                html_escape(&tr.content),
            );
        }
        ContentBlock::Image { .. } => {
            body.push_str(r#"<div class="text">[图片]</div>"#);
        }
        _ => {}
    }
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

    for line in text.lines() {
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

    #[test]
    fn test_export_html_with_compaction_entry() {
        use uncode_core::session::{CompactionEntry, generate_entry_id};
        let header = SessionHeader::new("test".into(), "model".into(), "/tmp".into());
        let entries = vec![SessionEntry::Compaction(Box::new(CompactionEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            summary: "Discussed authentication strategy".into(),
            first_kept_entry_id: "abc".into(),
            tokens_before: 5000,
            files_read: vec![],
            files_modified: vec![],
        }))];
        let html = export_html(&header, &entries, &[]);
        assert!(html.contains("上下文压缩"));
        assert!(html.contains("authentication strategy"));
    }

    #[test]
    fn test_export_html_with_branch_summary() {
        use uncode_core::session::{BranchSummaryEntry, generate_entry_id};
        let header = SessionHeader::new("test".into(), "model".into(), "/tmp".into());
        let entries = vec![SessionEntry::BranchSummary(Box::new(BranchSummaryEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            from_id: "old".into(),
            summary: "Explored caching approach".into(),
        }))];
        let html = export_html(&header, &entries, &[]);
        assert!(html.contains("分支摘要"));
        assert!(html.contains("caching approach"));
    }

    #[test]
    fn test_export_html_tool_call_block() {
        use uncode_core::message::ToolCall;
        let header = SessionHeader::new("id".into(), "model".into(), "/tmp".into());
        let messages = vec![(
            Role::Assistant,
            vec![ContentBlock::ToolCall(Box::new(ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "main.rs"}),
            }))],
        )];
        let html = export_html(&header, &[], &messages);
        assert!(html.contains("工具调用: read"));
        assert!(html.contains("main.rs"));
    }

    #[test]
    fn test_export_html_with_model_change() {
        use uncode_core::session::{ModelChangeEntry, generate_entry_id};
        let header = SessionHeader::new("test".into(), "model".into(), "/tmp".into());
        let entries = vec![SessionEntry::ModelChange(Box::new(ModelChangeEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            provider: "openai".into(),
            model_id: "gpt-4o".into(),
        }))];
        let html = export_html(&header, &entries, &[]);
        assert!(html.contains("模型切换"));
        assert!(html.contains("gpt-4o"));
    }

    #[test]
    fn test_export_html_tool_result_success() {
        use uncode_core::message::ToolResult;
        let header = SessionHeader::new("id".into(), "model".into(), "/tmp".into());
        let messages = vec![(
            Role::Tool,
            vec![ContentBlock::ToolResult(Box::new(ToolResult {
                tool_call_id: "tc".into(),
                content: "ok output".into(),
                is_error: false,
            }))],
        )];
        let html = export_html(&header, &[], &messages);
        assert!(html.contains("工具结果 (result)"));
        assert!(html.contains("ok output"));
    }

    /// 文档骨架片段须按出现顺序排列（golden 结构，不绑定动态日期/UUID）。
    #[test]
    fn test_export_html_document_structure_order() {
        let header =
            SessionHeader::new("golden-sid".into(), "golden-model".into(), "/golden".into());
        let messages = vec![(
            Role::User,
            vec![ContentBlock::Text {
                text: "body marker".into(),
            }],
        )];
        let html = export_html(&header, &[], &messages);

        let markers = [
            "<!DOCTYPE html>",
            r#"<html lang="zh-CN">"#,
            "<head>",
            "<body>",
            "<h1>",
            "<div class=\"meta\">",
            "golden-model",
            "golden-sid",
            r#"class="msg user""#,
            "body marker",
            "</html>",
        ];
        let mut offset = 0usize;
        for marker in markers {
            let rest = &html[offset..];
            let pos = rest
                .find(marker)
                .unwrap_or_else(|| panic!("missing marker {marker:?} after offset {offset}"));
            offset += pos + marker.len();
        }
    }
}
