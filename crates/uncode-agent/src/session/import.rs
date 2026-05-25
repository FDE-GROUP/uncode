//! JSONL 导入工具
//!
//! 首次启动时自动检测旧 `sessions/` 目录，将 JSONL 数据导入 SurrealDB。

use std::io::BufRead;
use std::path::Path;

use super::store::{SessionResult, SessionStore};
use uncode_core::session::{SessionEntry, SessionHeader};

#[derive(Debug, Default)]
pub struct ImportStats {
    pub sessions_imported: usize,
    pub entries_imported: usize,
    pub sessions_skipped: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for ImportStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "imported {} sessions ({} entries), {} skipped, {} errors",
            self.sessions_imported,
            self.entries_imported,
            self.sessions_skipped,
            self.errors.len()
        )
    }
}

/// 导入 JSONL 目录到 SurrealDB
pub async fn import_jsonl_dir(
    store: &SessionStore,
    jsonl_dir: &Path,
) -> SessionResult<ImportStats> {
    let mut stats = ImportStats::default();

    if !jsonl_dir.exists() {
        return Ok(stats);
    }

    let entries = match std::fs::read_dir(jsonl_dir) {
        Ok(e) => e,
        Err(_) => return Ok(stats),
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                stats.errors.push(format!("read_dir: {e}"));
                continue;
            }
        };

        let path = entry.path();
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }

        match import_single_jsonl(store, &path).await {
            Ok(entry_count) => {
                stats.sessions_imported += 1;
                stats.entries_imported += entry_count;
            }
            Err(e) => {
                stats.sessions_skipped += 1;
                stats.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    Ok(stats)
}

async fn import_single_jsonl(store: &SessionStore, path: &Path) -> SessionResult<usize> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .collect::<Result<_, _>>()
        .map_err(super::store::SessionError::Io)?;

    if lines.is_empty() {
        return Err(super::store::SessionError::InvalidData("empty file".into()));
    }

    let header: SessionHeader = serde_json::from_str(&lines[0])
        .map_err(|e| super::store::SessionError::InvalidData(format!("header parse: {e}")))?;

    // 如果 session 已存在则跳过
    if store.read_header(&header.id).await.is_ok() {
        return Ok(0);
    }

    store
        .init_session_with_title(
            &header.id,
            &header.model,
            &header.working_dir,
            header.title.clone(),
        )
        .await?;

    // 导入 v1 migration 逻辑：如果没有 parent_id 则在内存中 chain
    // 因为 SurrealDB append_entry 会自动设置 parent_id，
    // 我们直接按顺序追加，SurrealDB 层面自动链接。
    let mut count = 0;
    for (i, line) in lines.iter().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<SessionEntry>(line) {
            match store.append_entry(&header.id, &entry).await {
                Ok(()) => count += 1,
                Err(e) => {
                    tracing::warn!("import entry {} failed: {e}", entry.entry_id());
                }
            }
        }
    }

    tracing::info!(
        "imported session {} ({} entries) from {}",
        header.id,
        count,
        path.display()
    );

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::store::SessionStore;

    async fn new_store() -> SessionStore {
        SessionStore::new_memory().await.unwrap()
    }

    fn make_header(id: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "session",
            "id": id,
            "model": "m1",
            "title": null,
            "working_dir": "/tmp",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        })
    }

    fn make_message_entry(id: &str, text: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "message",
            "id": id,
            "timestamp": "2024-01-01T00:00:00Z",
            "role": "user",
            "content": [{"type": "text", "text": text}]
        })
    }

    fn write_jsonl(
        path: &std::path::Path,
        header: &serde_json::Value,
        entries: &[serde_json::Value],
    ) {
        let mut content = header.to_string();
        content.push('\n');
        for entry in entries {
            content.push_str(&entry.to_string());
            content.push('\n');
        }
        std::fs::write(path, content).expect("write jsonl");
    }

    #[test]
    fn test_import_stats_default() {
        let stats = ImportStats::default();
        assert_eq!(stats.sessions_imported, 0);
        assert_eq!(stats.entries_imported, 0);
        assert_eq!(stats.sessions_skipped, 0);
        assert!(stats.errors.is_empty());

        let display = format!("{stats}");
        assert!(display.contains("imported 0 sessions (0 entries)"));
        assert!(display.contains("0 skipped"));
        assert!(display.contains("0 errors"));
    }

    #[tokio::test]
    async fn test_import_nonexistent_dir() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let store = new_store().await;
        let nonexistent = tmp.path().join("nonexistent");
        let stats = import_jsonl_dir(&store, &nonexistent)
            .await
            .expect("import");
        assert_eq!(stats.sessions_imported, 0);
        assert!(stats.errors.is_empty());
    }

    #[tokio::test]
    async fn test_import_dir_with_valid_jsonl() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let store = new_store().await;

        let jsonl_path = tmp.path().join("test.jsonl");
        let header = make_header("s1");
        let entry = make_message_entry("e1", "hello");
        write_jsonl(&jsonl_path, &header, &[entry]);

        let stats = import_jsonl_dir(&store, tmp.path()).await.expect("import");
        assert!(stats.sessions_imported >= 1);
        assert!(stats.entries_imported >= 1);
        assert!(stats.errors.is_empty());
    }

    #[tokio::test]
    async fn test_import_dir_skips_non_jsonl() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let store = new_store().await;

        // Create a .jsonl file and a .txt file
        let jsonl_path = tmp.path().join("sessions.jsonl");
        let header = make_header("s2");
        let entry = make_message_entry("e2", "world");
        write_jsonl(&jsonl_path, &header, &[entry]);

        // Create a .txt file that should be ignored
        let txt_path = tmp.path().join("readme.txt");
        std::fs::write(&txt_path, "not a jsonl file").expect("write txt");

        let stats = import_jsonl_dir(&store, tmp.path()).await.expect("import");
        assert_eq!(stats.sessions_imported, 1);
        assert_eq!(stats.sessions_skipped, 0);
        assert_eq!(stats.errors.len(), 0);
    }

    #[tokio::test]
    async fn test_import_dir_skips_existing_session() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let store = new_store().await;

        let jsonl_path = tmp.path().join("test.jsonl");
        let header = make_header("s3");
        let entry = make_message_entry("e3", "greetings");
        write_jsonl(&jsonl_path, &header, &[entry]);

        // First import → session imported
        let stats1 = import_jsonl_dir(&store, tmp.path()).await.expect("import");
        assert_eq!(stats1.sessions_imported, 1);
        assert_eq!(stats1.entries_imported, 1);

        // Second import → session already exists, returns Ok(0)
        let stats2 = import_jsonl_dir(&store, tmp.path()).await.expect("import");
        assert_eq!(stats2.sessions_imported, 1);
        assert_eq!(stats2.entries_imported, 0);

        // Verify session still has only the original entry
        let entries = store.load_entries("s3").await.expect("load");
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn test_import_empty_jsonl_is_error() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let store = new_store().await;

        let jsonl_path = tmp.path().join("empty.jsonl");
        // Write a completely empty file
        std::fs::write(&jsonl_path, "").expect("write empty");

        let stats = import_jsonl_dir(&store, tmp.path()).await.expect("import");
        assert_eq!(stats.sessions_skipped, 1);
        assert_eq!(stats.sessions_imported, 0);
        assert_eq!(stats.errors.len(), 1);
        assert!(!stats.errors[0].is_empty());
    }
}
