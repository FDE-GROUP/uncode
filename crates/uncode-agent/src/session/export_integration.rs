//! 集成测试：`SessionStore` / JSONL 导入 → `export_html`（与 CLI `run_export` 一致）。

use std::io::Write;

use tempfile::tempdir;
use uncode_core::message::{ContentBlock, Message, Role, ToolCall, ToolResult};
use uncode_core::session::{
    BranchSummaryEntry, CompactionEntry, MessageEntry, ModelChangeEntry, SessionEntry,
    SessionHeader, generate_entry_id,
};

use super::export::export_html;
use super::import::import_jsonl_dir;
use super::store::SessionStore;

async fn memory_store() -> SessionStore {
    SessionStore::new_memory().await.expect("memory store")
}

async fn export_session(store: &SessionStore, session_id: &str) -> String {
    let header = store.read_header(session_id).await.expect("header");
    let entries = store.load_entries(session_id).await.expect("entries");
    export_html(&header, &entries, &[])
}

#[tokio::test]
async fn export_from_store_linear_conversation() {
    let store = memory_store().await;
    let sid = "export-linear";
    store
        .init_session(sid, "deepseek-v3", "/tmp/proj")
        .await
        .unwrap();

    store
        .append_entry(
            sid,
            &SessionEntry::Message(Message::user("fix the bug").into()),
        )
        .await
        .unwrap();
    store
        .append_entry(
            sid,
            &SessionEntry::Message(Message::assistant("checking grep").into()),
        )
        .await
        .unwrap();

    let html = export_session(&store, sid).await;
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("export-linear"));
    assert!(html.contains("deepseek-v3"));
    assert!(html.contains("fix the bug"));
    assert!(html.contains("checking grep"));
    assert!(html.contains(r#"class="msg user""#));
    assert!(html.contains(r#"class="msg assistant""#));
}

#[tokio::test]
async fn export_entries_take_priority_over_messages_fallback() {
    let header = SessionHeader::new("prio".into(), "model".into(), "/tmp".into());
    let entries = vec![SessionEntry::Message(Message::user("from store").into())];
    let fallback = vec![(
        Role::User,
        vec![ContentBlock::Text {
            text: "fallback only".into(),
        }],
    )];

    let html = export_html(&header, &entries, &fallback);
    assert!(html.contains("from store"));
    assert!(!html.contains("fallback only"));
}

#[tokio::test]
async fn export_fallback_messages_when_entries_empty() {
    let header = SessionHeader::new("fallback".into(), "model".into(), "/tmp".into());
    let messages = vec![(
        Role::Assistant,
        vec![ContentBlock::Text {
            text: "legacy transcript".into(),
        }],
    )];

    let html = export_html(&header, &[], &messages);
    assert!(html.contains("legacy transcript"));
    assert!(html.contains(r#"class="msg assistant""#));
}

#[tokio::test]
async fn export_store_escapes_html_in_messages() {
    let store = memory_store().await;
    let sid = "export-xss";
    store.init_session(sid, "model", "/tmp").await.unwrap();

    let payload = r#"<script>alert("x")</script> & "quotes""#;
    store
        .append_entry(sid, &SessionEntry::Message(Message::user(payload).into()))
        .await
        .unwrap();

    let html = export_session(&store, sid).await;
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&amp;"));
    assert!(html.contains("&quot;"));
    assert!(!html.contains("<script>alert"));
}

#[tokio::test]
async fn export_store_full_timeline() {
    let store = memory_store().await;
    let sid = "export-timeline";
    store
        .init_session_with_title(sid, "gpt-4o", "/repo", Some("Timeline".into()))
        .await
        .unwrap();

    store
        .append_entry(sid, &SessionEntry::Message(Message::user("start").into()))
        .await
        .unwrap();

    store
        .append_entry(
            sid,
            &SessionEntry::Compaction(Box::new(CompactionEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                summary: "Summarized auth discussion".into(),
                first_kept_entry_id: "keep-me".into(),
                tokens_before: 12_000,
                files_read: vec![],
                files_modified: vec![],
            })),
        )
        .await
        .unwrap();

    store
        .append_entry(
            sid,
            &SessionEntry::ModelChange(Box::new(ModelChangeEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                provider: "openai".into(),
                model_id: "gpt-4o-mini".into(),
            })),
        )
        .await
        .unwrap();

    store
        .append_entry(
            sid,
            &SessionEntry::BranchSummary(Box::new(BranchSummaryEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                from_id: "branch-root".into(),
                summary: "Tried Redis cache layer".into(),
            })),
        )
        .await
        .unwrap();

    let html = export_session(&store, sid).await;
    assert!(html.contains("Timeline"));
    assert!(html.contains("start"));
    assert!(html.contains("上下文压缩"));
    assert!(html.contains("auth discussion"));
    assert!(html.contains("模型切换"));
    assert!(html.contains("gpt-4o-mini"));
    assert!(html.contains("分支摘要"));
    assert!(html.contains("Redis cache"));
}

#[tokio::test]
async fn export_store_tool_call_and_error_result() {
    let store = memory_store().await;
    let sid = "export-tools";
    store.init_session(sid, "model", "/tmp").await.unwrap();

    store
        .append_entry(
            sid,
            &SessionEntry::Message(Box::new(MessageEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(Box::new(ToolCall {
                    id: "tc-1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "src/lib.rs"}),
                }))],
                usage: None,
            })),
        )
        .await
        .unwrap();

    store
        .append_entry(
            sid,
            &SessionEntry::Message(Box::new(MessageEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(Box::new(ToolResult {
                    tool_call_id: "tc-1".into(),
                    content: "permission denied".into(),
                    is_error: true,
                }))],
                usage: None,
            })),
        )
        .await
        .unwrap();

    let html = export_session(&store, sid).await;
    assert!(html.contains("工具调用: read"));
    assert!(html.contains("src/lib.rs"));
    assert!(html.contains("工具结果 (error)"));
    assert!(html.contains("permission denied"));
}

#[tokio::test]
async fn export_after_jsonl_import_matches_cli_path() {
    let store = memory_store().await;
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("session.jsonl");

    let header = SessionHeader::new(
        "jsonl-export-session".into(),
        "deepseek-v3".into(),
        "/imported/wd".into(),
    )
    .with_title("Imported Session".into());

    let user = SessionEntry::Message(Message::user("imported user line").into());
    let assistant = SessionEntry::Message(Message::assistant("imported reply").into());

    let mut file = std::fs::File::create(&path).expect("create jsonl");
    writeln!(file, "{}", serde_json::to_string(&header).unwrap()).unwrap();
    writeln!(file, "{}", serde_json::to_string(&user).unwrap()).unwrap();
    writeln!(file, "{}", serde_json::to_string(&assistant).unwrap()).unwrap();

    let stats = import_jsonl_dir(&store, dir.path()).await.expect("import");
    assert_eq!(stats.sessions_imported, 1);
    assert_eq!(stats.entries_imported, 2);

    let html = export_session(&store, "jsonl-export-session").await;
    assert!(html.contains("Imported Session"));
    assert!(html.contains("imported user line"));
    assert!(html.contains("imported reply"));
    assert!(html.contains("jsonl-export-session"));
}

#[tokio::test]
async fn export_jsonl_import_is_idempotent() {
    let store = memory_store().await;
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("once.jsonl");

    let header = SessionHeader::new("idem-session".into(), "model".into(), "/wd".into());
    let entry = SessionEntry::Message(Message::user("once").into());

    let mut file = std::fs::File::create(&path).expect("create jsonl");
    writeln!(file, "{}", serde_json::to_string(&header).unwrap()).unwrap();
    writeln!(file, "{}", serde_json::to_string(&entry).unwrap()).unwrap();

    let first = import_jsonl_dir(&store, dir.path()).await.expect("import");
    assert_eq!(first.sessions_imported, 1);
    assert_eq!(first.entries_imported, 1);

    let second = import_jsonl_dir(&store, dir.path())
        .await
        .expect("re-import");
    assert_eq!(second.entries_imported, 0);

    let entries = store.load_entries("idem-session").await.expect("entries");
    assert_eq!(entries.len(), 1, "re-import must not duplicate entries");

    let html = export_session(&store, "idem-session").await;
    assert!(html.contains("once"));
}

#[tokio::test]
async fn export_store_thinking_and_markdown_from_message() {
    let store = memory_store().await;
    let sid = "export-thinking";
    store.init_session(sid, "model", "/tmp").await.unwrap();

    store
        .append_entry(
            sid,
            &SessionEntry::Message(Box::new(MessageEntry {
                id: generate_entry_id(),
                parent_id: None,
                timestamp: chrono::Utc::now(),
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Thinking {
                        text: "plan steps".into(),
                    },
                    ContentBlock::Text {
                        text: "line\n```rs\nlet x = 1;\n```\ndone".into(),
                    },
                ],
                usage: None,
            })),
        )
        .await
        .unwrap();

    let html = export_session(&store, sid).await;
    assert!(html.contains("思考过程"));
    assert!(html.contains("plan steps"));
    assert!(html.contains("<pre><code>"));
    assert!(html.contains("let x = 1;"));
}

#[tokio::test]
async fn export_header_title_is_escaped_in_meta() {
    let store = memory_store().await;
    let sid = "export-title";
    store
        .init_session_with_title(
            sid,
            "model",
            "/tmp",
            Some(r#"<em>Title</em> & "ok""#.into()),
        )
        .await
        .unwrap();

    store
        .append_entry(sid, &SessionEntry::Message(Message::user("x").into()))
        .await
        .unwrap();

    let html = export_session(&store, sid).await;
    assert!(html.contains("&lt;em&gt;Title&lt;/em&gt;"));
    assert!(html.contains("&amp;"));
    assert!(!html.contains("<em>Title</em>"));
}
