//! UI action channel bridge — WASM blocking thread → TUI async event loop.

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc as tokio_mpsc;

use uncode_core::ui_action::UiAction;

/// A pending UI action + response channel.
pub struct PendingUiAction {
    pub action: UiAction,
    pub response_tx: std_mpsc::Sender<Result<(), String>>,
}

/// TUI-side UI bridge — wraps tokio mpsc receiver.
pub struct UiBridge {
    rx: tokio_mpsc::Receiver<PendingUiAction>,
}

impl UiBridge {
    pub fn new(rx: tokio_mpsc::Receiver<PendingUiAction>) -> Self {
        Self { rx }
    }

    pub async fn recv(&mut self) -> Option<PendingUiAction> {
        self.rx.recv().await
    }
}

/// Create UI action channel pair.
pub fn ui_channel(capacity: usize) -> (tokio_mpsc::Sender<PendingUiAction>, UiBridge) {
    let (tx, rx) = tokio_mpsc::channel(capacity);
    (tx, UiBridge::new(rx))
}
