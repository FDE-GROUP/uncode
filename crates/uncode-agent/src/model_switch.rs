use std::sync::Arc;

use crate::session::store::{SessionResult, SessionStore};
use uncode_core::session::{ModelChangeEntry, SessionEntry, generate_entry_id};

/// 运行时切换 LLM 模型并记录 ModelChange 到会话 JSONL
pub fn switch_model(
    current_model_id: &mut String,
    new_model_id: &str,
    new_provider: &str,
    session_store: &Arc<SessionStore>,
    session_id: Option<&str>,
) -> SessionResult<()> {
    let old = current_model_id.clone();
    *current_model_id = new_model_id.to_string();

    if let Some(sid) = session_id {
        let entry = SessionEntry::ModelChange(ModelChangeEntry {
            id: generate_entry_id(),
            parent_id: None,
            timestamp: chrono::Utc::now(),
            provider: new_provider.to_string(),
            model_id: new_model_id.to_string(),
        });
        session_store.append_entry(sid, &entry)?;
    }

    tracing::info!("model switched: {old} -> {new_model_id}");
    Ok(())
}
