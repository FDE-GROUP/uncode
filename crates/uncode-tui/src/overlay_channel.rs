//! Overlay channel bridge — WASM blocking thread → TUI async event loop.

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc as tokio_mpsc;

use uncode_core::overlay::OverlayAction;

/// A pending overlay action + response channel.
pub struct PendingOverlayAction {
    pub action: OverlayAction,
    pub response_tx: std_mpsc::Sender<Result<(), String>>,
}

/// TUI-side overlay bridge — wraps tokio mpsc receiver.
pub struct OverlayBridge {
    rx: tokio_mpsc::Receiver<PendingOverlayAction>,
}

impl OverlayBridge {
    pub fn new(rx: tokio_mpsc::Receiver<PendingOverlayAction>) -> Self {
        Self { rx }
    }

    pub fn try_recv(&mut self) -> Option<PendingOverlayAction> {
        self.rx.try_recv().ok()
    }

    pub async fn recv(&mut self) -> Option<PendingOverlayAction> {
        self.rx.recv().await
    }
}

/// Create overlay channel pair.
pub fn overlay_channel(
    capacity: usize,
) -> (tokio_mpsc::Sender<PendingOverlayAction>, OverlayBridge) {
    let (tx, rx) = tokio_mpsc::channel(capacity);
    (tx, OverlayBridge::new(rx))
}
