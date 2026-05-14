use tokio::sync::mpsc;
use uncode_core::message::Message;

pub struct MessageQueue {
    steering_tx: mpsc::UnboundedSender<Message>,
    steering_rx: mpsc::UnboundedReceiver<Message>,
    follow_up_tx: mpsc::UnboundedSender<Message>,
    follow_up_rx: mpsc::UnboundedReceiver<Message>,
}

impl MessageQueue {
    pub fn new() -> Self {
        let (steering_tx, steering_rx) = mpsc::unbounded_channel();
        let (follow_up_tx, follow_up_rx) = mpsc::unbounded_channel();
        Self {
            steering_tx,
            steering_rx,
            follow_up_tx,
            follow_up_rx,
        }
    }

    pub fn steer(&self, msg: Message) {
        let _ = self.steering_tx.send(msg);
    }

    pub fn follow_up(&self, msg: Message) {
        let _ = self.follow_up_tx.send(msg);
    }

    pub fn drain_steering(&mut self) -> Vec<Message> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.steering_rx.try_recv() {
            messages.push(msg);
        }
        messages
    }

    pub async fn wait_follow_up(&mut self) -> Vec<Message> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.follow_up_rx.try_recv() {
            messages.push(msg);
        }
        messages
    }

    pub fn clone_sender(&self) -> MessageQueueHandle {
        MessageQueueHandle {
            steering: self.steering_tx.clone(),
            follow_up: self.follow_up_tx.clone(),
        }
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct MessageQueueHandle {
    pub steering: mpsc::UnboundedSender<Message>,
    pub follow_up: mpsc::UnboundedSender<Message>,
}
