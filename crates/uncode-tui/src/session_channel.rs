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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc as std_mpsc;
    use uncode_extensions::session::{SessionAction, SessionResponse};

    #[tokio::test]
    async fn test_session_channel_factory() {
        let (tx, mut bridge) = session_channel(8);
        let (rtx, rrx) = std_mpsc::channel();
        let pending = PendingSessionAction {
            action: SessionAction::SetName { name: "test".into() },
            response_tx: rtx,
        };
        tx.try_send(pending).unwrap();
        let received = bridge.recv().await;
        assert!(received.is_some());
        let pending = received.unwrap();
        assert!(matches!(pending.action, SessionAction::SetName { .. }));
        pending.response_tx.send(Ok(SessionResponse::Ok)).unwrap();
        assert!(rrx.recv().is_ok());
    }

    #[tokio::test]
    async fn test_recv_awaits() {
        let (tx, mut bridge) = session_channel(4);
        let (rtx, _rrx) = std_mpsc::channel::<Result<SessionResponse, String>>();
        tx.send(PendingSessionAction {
            action: SessionAction::Fork { entry_id: "e1".into() },
            response_tx: rtx,
        }).await.unwrap();
        let received = bridge.recv().await;
        assert!(received.is_some());
    }
}
