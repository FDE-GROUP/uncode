use tokio::sync::mpsc;
use tracing::warn;
use uncode_core::message::Message;

const CHANNEL_CAPACITY: usize = 64;

/// Steering 队列清空策略。
///
/// **Pi:** 对照 `"all"` / `"one-at-a-time"` drain 模式。
#[derive(Debug, Clone, Copy, Default)]
pub enum DrainMode {
    /// 一次性清空所有排队消息。
    #[default]
    All,
    /// 每次只取最旧的一条，剩余留给后续 Turn。
    OneAtATime,
}

fn drain_receiver(rx: &mut mpsc::Receiver<Message>, mode: DrainMode) -> Vec<Message> {
    match mode {
        DrainMode::All => std::iter::from_fn(|| rx.try_recv().ok()).collect(),
        DrainMode::OneAtATime => rx.try_recv().ok().into_iter().collect(),
    }
}

/// 三通道运行时消息队列（steering / follow_up / next_turn）。
///
/// **Pi:** 对应 `steering`、`followUp`、`nextTurn` 三个队列（各 `mpsc`，容量 64）。
pub struct MessageQueue {
    steering_tx: mpsc::Sender<Message>,
    steering_rx: mpsc::Receiver<Message>,
    follow_up_tx: mpsc::Sender<Message>,
    follow_up_rx: mpsc::Receiver<Message>,
    next_turn_tx: mpsc::Sender<Message>,
    next_turn_rx: mpsc::Receiver<Message>,
    steering_drain_mode: DrainMode,
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
            steering_drain_mode: DrainMode::default(),
        }
    }

    pub fn with_steering_drain_mode(mut self, mode: DrainMode) -> Self {
        self.steering_drain_mode = mode;
        self
    }

    pub async fn steer(&self, msg: Message) {
        if self.steering_tx.send(msg).await.is_err() {
            warn!("steering channel closed, message dropped");
        }
    }

    pub async fn follow_up(&self, msg: Message) {
        if self.follow_up_tx.send(msg).await.is_err() {
            warn!("follow_up channel closed, message dropped");
        }
    }

    pub async fn next_turn(&self, msg: Message) {
        if self.next_turn_tx.send(msg).await.is_err() {
            warn!("next_turn channel closed, message dropped");
        }
    }

    pub fn drain_steering(&mut self) -> Vec<Message> {
        drain_receiver(&mut self.steering_rx, self.steering_drain_mode)
    }

    pub fn drain_follow_up(&mut self) -> Vec<Message> {
        drain_receiver(&mut self.follow_up_rx, DrainMode::All)
    }

    pub fn drain_next_turn(&mut self) -> Vec<Message> {
        drain_receiver(&mut self.next_turn_rx, DrainMode::All)
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

    /// Return approximate message counts in each queue.
    ///
    /// Uses `len()` on the mpsc receiver; values are approximate since
    /// concurrent senders may change the count between call and observation.
    pub fn queue_counts(&self) -> (usize, usize, usize) {
        (
            self.steering_rx.len(),
            self.follow_up_rx.len(),
            self.next_turn_rx.len(),
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_drain_all_returns_everything() {
        let mut mq = MessageQueue::new();
        mq.steer(Message::user("a")).await;
        mq.steer(Message::user("b")).await;
        mq.steer(Message::user("c")).await;

        let msgs = mq.drain_steering();
        assert_eq!(msgs.len(), 3);
        // Queue should be empty after drain
        assert!(mq.drain_steering().is_empty());
    }

    #[tokio::test]
    async fn test_drain_one_at_a_time_returns_single() {
        let mut mq = MessageQueue::new().with_steering_drain_mode(DrainMode::OneAtATime);
        mq.steer(Message::user("a")).await;
        mq.steer(Message::user("b")).await;
        mq.steer(Message::user("c")).await;

        let first = mq.drain_steering();
        assert_eq!(first.len(), 1);

        let second = mq.drain_steering();
        assert_eq!(second.len(), 1);

        let third = mq.drain_steering();
        assert_eq!(third.len(), 1);

        assert!(mq.drain_steering().is_empty());
    }

    #[tokio::test]
    async fn test_drain_one_at_a_time_empty() {
        let mut mq = MessageQueue::new().with_steering_drain_mode(DrainMode::OneAtATime);
        assert!(mq.drain_steering().is_empty());
    }

    #[tokio::test]
    async fn test_follow_up_always_drains_all() {
        let mut mq = MessageQueue::new().with_steering_drain_mode(DrainMode::OneAtATime);
        mq.follow_up(Message::user("x")).await;
        mq.follow_up(Message::user("y")).await;

        // follow_up ignores steering_drain_mode, always drains all
        let msgs = mq.drain_follow_up();
        assert_eq!(msgs.len(), 2);
    }

    #[tokio::test]
    async fn test_has_items() {
        let mut mq = MessageQueue::new();
        assert!(!mq.has_items());
        mq.steer(Message::user("hi")).await;
        assert!(mq.has_items());
        mq.drain_steering();
        assert!(!mq.has_items());
    }
}
