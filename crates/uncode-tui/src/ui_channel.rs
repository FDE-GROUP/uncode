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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc as std_mpsc;
    use uncode_core::ui_action::UiAction;

    #[tokio::test]
    async fn test_ui_channel_factory() {
        let (tx, mut bridge) = ui_channel(8);
        let (rtx, rrx) = std_mpsc::channel();
        let pending = PendingUiAction {
            action: UiAction::SetTitle { title: "hello".into() },
            response_tx: rtx,
        };
        tx.try_send(pending).unwrap();
        let received = bridge.recv().await;
        assert!(received.is_some());
        let pending = received.unwrap();
        match &pending.action {
            UiAction::SetTitle { title } => assert_eq!(title, "hello"),
            _ => panic!("expected SetTitle"),
        }
        pending.response_tx.send(Ok(())).unwrap();
        assert!(rrx.recv().is_ok());
    }

    #[tokio::test]
    async fn test_recv_awaits() {
        let (tx, mut bridge) = ui_channel(4);
        let (rtx, _rrx) = std_mpsc::channel::<Result<(), String>>();
        tx.send(PendingUiAction {
            action: UiAction::SetWorkingVisible { visible: false },
            response_tx: rtx,
        }).await.unwrap();
        let received = bridge.recv().await;
        assert!(received.is_some());
    }
}
