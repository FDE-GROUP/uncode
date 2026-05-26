use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::broadcast;
use tokio::sync::oneshot;

use uncode_core::event::AgentEvent;

static REGISTRY: LazyLock<Mutex<HashMap<String, oneshot::Sender<Vec<Vec<String>>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static EVENT_SENDER: LazyLock<Mutex<Option<broadcast::Sender<AgentEvent>>>> =
    LazyLock::new(|| Mutex::new(None));

/// Set the global event sender (called once from harness).
pub fn set_event_sender(tx: broadcast::Sender<AgentEvent>) {
    *EVENT_SENDER.lock() = Some(tx);
}

/// Send an AgentEvent through the global sender.
pub fn send_event(event: AgentEvent) {
    if let Some(ref tx) = *EVENT_SENDER.lock() {
        let _ = tx.send(event);
    }
}

/// Register a question response channel for the given tool call ID.
pub fn register(id: &str, tx: oneshot::Sender<Vec<Vec<String>>>) {
    REGISTRY.lock().insert(id.to_string(), tx);
}

/// Resolve a question: send the user's answers back to the waiting tool.
/// Returns true if the tool was waiting and received the answer.
pub fn resolve(id: &str, answers: Vec<Vec<String>>) -> bool {
    if let Some(tx) = REGISTRY.lock().remove(id) {
        let _ = tx.send(answers);
        true
    } else {
        false
    }
}
