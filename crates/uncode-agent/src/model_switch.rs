use std::sync::Arc;

use uncode_llm::driver::LlmDriver;
use uncode_session::store::{SessionResult, SessionStore};

pub fn switch_model(
    driver: &mut Arc<dyn LlmDriver>,
    new_driver: Arc<dyn LlmDriver>,
    model_name: &str,
    session_store: &Arc<SessionStore>,
    session_id: Option<&str>,
) -> SessionResult<()> {
    let old_name = driver.provider_name();
    *driver = new_driver;

    if let Some(sid) = session_id {
        let entry = uncode_core::session::SessionEntry::System(uncode_core::session::SystemEntry {
            timestamp: chrono::Utc::now(),
            event: uncode_core::session::SystemEventType::SessionEnd,
            data: serde_json::json!({
                "type": "model_change",
                "from": old_name,
                "to": model_name,
            }),
        });
        session_store.append_entry(sid, &entry)?;
    }

    tracing::info!("model switched: {old_name} -> {model_name}");
    Ok(())
}
