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
