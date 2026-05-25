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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc as std_mpsc;
    use uncode_core::overlay::OverlayAction;

    #[test]
    fn test_overlay_channel_factory() {
        let (tx, mut bridge) = overlay_channel(8);
        let (rtx, _rrx) = std_mpsc::channel();
        let action = OverlayAction::Hide {
            key: "test".into(),
        };
        let pending = PendingOverlayAction {
            action,
            response_tx: rtx,
        };
        tx.try_send(pending).unwrap();
        let received = bridge.try_recv().unwrap();
        match &received.action {
            OverlayAction::Hide { key } => assert_eq!(key, "test"),
            _ => panic!("expected Hide"),
        }
    }

    #[test]
    fn test_try_recv_empty() {
        let (_tx, mut bridge) = overlay_channel(1);
        assert!(bridge.try_recv().is_none());
    }

    #[tokio::test]
    async fn test_recv_awaits_message() {
        let (tx, mut bridge) = overlay_channel(8);
        let (rtx, _rrx) = std_mpsc::channel();
        let pending = PendingOverlayAction {
            action: OverlayAction::Hide { key: "k".into() },
            response_tx: rtx,
        };
        tx.send(pending).await.unwrap();
        let received = bridge.recv().await.unwrap();
        assert!(matches!(received.action, OverlayAction::Hide { .. }));
    }
}
