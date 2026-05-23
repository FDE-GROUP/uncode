//! Dialog channel bridge — WASM 阻塞线程 ↔ TUI 异步事件循环。
//!
//! Extension 在 `spawn_blocking` 线程中通过 `blocking_send` 发送请求，
//! 阻塞等待 `std::sync::mpsc::Receiver`。TUI 事件循环通过
//! `tokio::sync::mpsc::Receiver` 接收请求，用户响应后通过
//! `std::sync::mpsc::Sender` 回传。

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc as tokio_mpsc;

use uncode_core::dialog::{DialogRequest, DialogResponse};

/// 一个待处理的对话框请求 + 响应通道。
pub struct PendingDialog {
    pub request: DialogRequest,
    pub response_tx: std_mpsc::Sender<DialogResponse>,
}

/// TUI 侧的 dialog 桥接器 — 封装 tokio mpsc receiver。
pub struct DialogBridge {
    rx: tokio_mpsc::Receiver<PendingDialog>,
}

impl DialogBridge {
    pub fn new(rx: tokio_mpsc::Receiver<PendingDialog>) -> Self {
        Self { rx }
    }

    /// 尝试接收下一个 dialog 请求（非阻塞）。
    pub fn try_recv(&mut self) -> Option<PendingDialog> {
        self.rx.try_recv().ok()
    }

    /// 异步接收下一个 dialog 请求（用于 tokio::select!）。
    pub async fn recv(&mut self) -> Option<PendingDialog> {
        self.rx.recv().await
    }
}

/// 创建 dialog 通道对。
///
/// 返回 (sender, bridge):
/// - `sender`: 传给 CLI callback 闭包，用于 `blocking_send`
/// - `bridge`: 传给 TUI，用于事件循环轮询
pub fn dialog_channel(capacity: usize) -> (tokio_mpsc::Sender<PendingDialog>, DialogBridge) {
    let (tx, rx) = tokio_mpsc::channel(capacity);
    (tx, DialogBridge::new(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_channel_send_recv() {
        let (tx, mut bridge) = dialog_channel(4);
        let (resp_tx, resp_rx) = std_mpsc::channel();

        let pending = PendingDialog {
            request: DialogRequest::Confirm {
                message: "ok?".into(),
            },
            response_tx: resp_tx,
        };

        tx.blocking_send(pending).unwrap();
        let received = bridge.try_recv().unwrap();

        assert!(matches!(received.request, DialogRequest::Confirm { .. }));
        received
            .response_tx
            .send(DialogResponse::Confirmed(true))
            .unwrap();
        assert_eq!(resp_rx.recv().unwrap(), DialogResponse::Confirmed(true));
    }

    #[test]
    fn test_dialog_channel_try_recv_empty() {
        let (_, mut bridge) = dialog_channel(4);
        assert!(bridge.try_recv().is_none());
    }
}
