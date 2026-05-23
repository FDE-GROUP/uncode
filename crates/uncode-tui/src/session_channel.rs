//! Session action channel bridge — WASM blocking thread → TUI async event loop.

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc as tokio_mpsc;

use uncode_extensions::session::{SessionAction, SessionResponse};

/// A pending session action + response channel.
pub struct PendingSessionAction {
    pub action: SessionAction,
    pub response_tx: std_mpsc::Sender<Result<SessionResponse, String>>,
}

/// TUI-side session bridge — wraps tokio mpsc receiver.
pub struct SessionBridge {
    rx: tokio_mpsc::Receiver<PendingSessionAction>,
}

impl SessionBridge {
    pub fn new(rx: tokio_mpsc::Receiver<PendingSessionAction>) -> Self {
        Self { rx }
    }

    pub async fn recv(&mut self) -> Option<PendingSessionAction> {
        self.rx.recv().await
    }
}

/// Create session action channel pair.
pub fn session_channel(
    capacity: usize,
) -> (tokio_mpsc::Sender<PendingSessionAction>, SessionBridge) {
    let (tx, rx) = tokio_mpsc::channel(capacity);
    (tx, SessionBridge::new(rx))
}
