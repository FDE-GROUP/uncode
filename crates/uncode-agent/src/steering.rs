use tokio::sync::mpsc;
use uncode_core::message::Message;

const CHANNEL_CAPACITY: usize = 64;

fn drain_receiver(rx: &mut mpsc::Receiver<Message>) -> Vec<Message> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

pub struct MessageQueue {
    steering_tx: mpsc::Sender<Message>,
    steering_rx: mpsc::Receiver<Message>,
    follow_up_tx: mpsc::Sender<Message>,
    follow_up_rx: mpsc::Receiver<Message>,
    next_turn_tx: mpsc::Sender<Message>,
    next_turn_rx: mpsc::Receiver<Message>,
}

impl MessageQueue {
    pub fn new() -> Self {
        let (steering_tx, steering_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (follow_up_tx, follow_up_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (next_turn_tx, next_turn_rx) = mpsc::channel(CHANNEL_CAPACITY);
        Self {
            steering_tx,
            steering_rx,
            follow_up_tx,
            follow_up_rx,
            next_turn_tx,
            next_turn_rx,
        }
    }

    pub async fn steer(&self, msg: Message) {
        let _ = self.steering_tx.send(msg).await;
    }

    pub async fn follow_up(&self, msg: Message) {
        let _ = self.follow_up_tx.send(msg).await;
    }

    pub async fn next_turn(&self, msg: Message) {
        let _ = self.next_turn_tx.send(msg).await;
    }

    pub fn drain_steering(&mut self) -> Vec<Message> {
        drain_receiver(&mut self.steering_rx)
    }

    pub fn drain_follow_up(&mut self) -> Vec<Message> {
        drain_receiver(&mut self.follow_up_rx)
    }

    pub fn drain_next_turn(&mut self) -> Vec<Message> {
        drain_receiver(&mut self.next_turn_rx)
    }

    pub fn clear_steering(&mut self) -> Vec<Message> {
        self.drain_steering()
    }

    pub fn clear_follow_up(&mut self) -> Vec<Message> {
        self.drain_follow_up()
    }

    pub fn clear_all(&mut self) -> (Vec<Message>, Vec<Message>) {
        (self.drain_steering(), self.drain_follow_up())
    }

    pub fn has_items(&self) -> bool {
        !self.steering_rx.is_empty() || !self.follow_up_rx.is_empty()
    }

    pub fn clone_handle(&self) -> MessageQueueHandle {
        MessageQueueHandle {
            steering: self.steering_tx.clone(),
            follow_up: self.follow_up_tx.clone(),
            next_turn: self.next_turn_tx.clone(),
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
    pub next_turn: mpsc::Sender<Message>,
}
