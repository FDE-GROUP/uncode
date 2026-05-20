//! 端到端导出 smoke：内存 SessionStore → HTML 文件（不占用 ~/.local/share/uncode 锁）。
//!
//! ```bash
//! cargo run -p uncode-agent --example export_smoke -- /tmp/uncode-export.html
//! ```

use std::path::PathBuf;

use uncode_agent::session::export::export_html;
use uncode_agent::session::store::SessionStore;
use uncode_core::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
use uncode_core::session::{
    BranchSummaryEntry, CompactionEntry, MessageEntry, ModelChangeEntry, SessionEntry,
    generate_entry_id,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let out: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/uncode-export-smoke.html"));

    let store = SessionStore::new_memory().await?;
    let sid = "smoke-export-demo";
    let working_dir = std::env::current_dir()?.display().to_string();
    store
        .init_session_with_title(
            sid,
            "deepseek-v3",
            &working_dir,
            Some("导出功能 Smoke 测试".into()),
        )
        .await?;

    store
        .append_entry(
            sid,
            &SessionEntry::Message(Message::user("请检查 export 是否正常").into()),
        )
        .await?;
    store
        .append_entry(
            sid,
            &SessionEntry::Message(Message::assistant("正在调用 read 工具").into()),
        )
        .await?;
    store
        .append_entry(
            sid,
            &SessionEntry::Message(Box::new(MessageEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc-smoke".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "README.md"}),
                }))],
                usage: None,
            })),
        )
        .await?;
    store
        .append_entry(
            sid,
            &SessionEntry::Message(Box::new(MessageEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(Box::new(ToolResult {
                    tool_call_id: "tc-smoke".into(),
                    content: "# uncode\n\nRust-native agent.".into(),
                    is_error: false,
                }))],
                usage: None,
            })),
        )
        .await?;
    store
        .append_entry(
            sid,
            &SessionEntry::Compaction(Box::new(CompactionEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                summary: "讨论了 export HTML 与 JSONL 导入路径".into(),
                first_kept_entry_id: "keep".into(),
                tokens_before: 8000,
                files_read: vec![],
                files_modified: vec![],
            })),
        )
        .await?;
    store
        .append_entry(
            sid,
            &SessionEntry::ModelChange(Box::new(ModelChangeEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                provider: "deepseek".into(),
                model_id: "deepseek-v3".into(),
            })),
        )
        .await?;
    store
        .append_entry(
            sid,
            &SessionEntry::BranchSummary(Box::new(BranchSummaryEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                from_id: "root".into(),
                summary: "尝试了 /export html 与 CLI export 子命令".into(),
            })),
        )
        .await?;

    let header = store.read_header(sid).await?;
    let entries = store.load_entries(sid).await?;
    let html = export_html(&header, &entries, &[]);

    std::fs::write(&out, &html)?;
    eprintln!("Wrote {} bytes to {}", html.len(), out.display());
    eprintln!("Open: file://{}", out.canonicalize()?.display());

    Ok(())
}
