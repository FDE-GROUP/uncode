use tokio::sync::mpsc;
use uncode_core::message::Message;

const CHANNEL_CAPACITY: usize = 64;

pub struct MessageQueue {
    steering_tx: mpsc::Sender<Message>,
    steering_rx: mpsc::Receiver<Message>,
    follow_up_tx: mpsc::Sender<Message>,
    follow_up_rx: mpsc::Receiver<Message>,
}

impl MessageQueue {
    pub fn new() -> Self {
        let (steering_tx, steering_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (follow_up_tx, follow_up_rx) = mpsc::channel(CHANNEL_CAPACITY);
        Self {
            steering_tx,
            steering_rx,
            follow_up_tx,
            follow_up_rx,
        }
    }

    pub async fn steer(&self, msg: Message) {
        let _ = self.steering_tx.send(msg).await;
    }

    pub async fn follow_up(&self, msg: Message) {
        let _ = self.follow_up_tx.send(msg).await;
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
    pub steering: mpsc::Sender<Message>,
    pub follow_up: mpsc::Sender<Message>,
}
