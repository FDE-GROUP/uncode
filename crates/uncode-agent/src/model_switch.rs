use std::sync::Arc;

use uncode_session::store::{SessionResult, SessionStore};

/// 运行时切换 LLM 模型并记录 ModelChange 到会话 JSONL
pub fn switch_model(
    current_model_id: &mut String,
    new_model_id: &str,
    session_store: &Arc<SessionStore>,
    session_id: Option<&str>,
) -> SessionResult<()> {
    let old = current_model_id.clone();
    *current_model_id = new_model_id.to_string();

    if let Some(sid) = session_id {
        let entry = uncode_core::session::SessionEntry::System(uncode_core::session::SystemEntry {
            timestamp: chrono::Utc::now(),
            event: uncode_core::session::SystemEventType::SessionEnd,
            data: serde_json::json!({
                "type": "model_change",
                "from": old,
                "to": new_model_id,
            }),
        });
        session_store.append_entry(sid, &entry)?;
    }

    tracing::info!("model switched: {old} -> {new_model_id}");
    Ok(())
}
