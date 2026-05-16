/// 消息队列 — Agent 工作时用户可排队发送指令
///
/// 两种排队策略（参照 Pi）：
///  - FollowUp：Agent 完成全部工作后投递（默认）
///  - Steering：当前工具调用完成后立即投递（用于修正方向）
///
/// 排队消息类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueType {
    /// Agent 完成全部工作后投递
    FollowUp,
    /// 当前工具调用完成后立即投递
    Steering,
}

/// Drain 模式（对齐 Pi QueueMode）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueueMode {
    /// 每次只取一条消息
    #[default]
    OneAtATime,
    /// 一次取出全部消息
    All,
}

/// 排队消息
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub text: String,
    pub queue_type: QueueType,
}

/// TUI 侧消息队列
pub struct MessageQueue {
    messages: Vec<QueuedMessage>,
    steering_mode: QueueMode,
    follow_up_mode: QueueMode,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            steering_mode: QueueMode::All,
            follow_up_mode: QueueMode::OneAtATime,
        }
    }

    /// 添加排队消息
    pub fn enqueue(&mut self, text: String, queue_type: QueueType) {
        self.messages.push(QueuedMessage { text, queue_type });
    }

    /// 按 mode drain follow-up 消息
    pub fn drain_follow_up(&mut self) -> Vec<String> {
        self.drain_by_type(QueueType::FollowUp, self.follow_up_mode)
    }

    /// 按 mode drain steering 消息
    pub fn drain_steering(&mut self) -> Vec<String> {
        self.drain_by_type(QueueType::Steering, self.steering_mode)
    }

    fn drain_by_type(&mut self, queue_type: QueueType, mode: QueueMode) -> Vec<String> {
        let indices: Vec<usize> = self
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.queue_type == queue_type)
            .map(|(i, _)| i)
            .collect();

        let to_take = match mode {
            QueueMode::All => indices.len(),
            QueueMode::OneAtATime => usize::min(1, indices.len()),
        };

        let mut result = Vec::with_capacity(to_take);
        for &idx in indices.iter().take(to_take) {
            result.push(self.messages[idx].text.clone());
        }
        // Remove in reverse order to keep indices valid
        for &idx in indices.iter().take(to_take).rev() {
            self.messages.remove(idx);
        }
        result
    }

    /// 队列是否为空
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 队列中消息数量
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// 获取所有排队消息的引用
    pub fn messages(&self) -> &[QueuedMessage] {
        &self.messages
    }

    /// 清空队列，返回被清空的消息
    pub fn clear(&mut self) -> Vec<QueuedMessage> {
        std::mem::take(&mut self.messages)
    }

    pub fn set_steering_mode(&mut self, mode: QueueMode) {
        self.steering_mode = mode;
    }

    pub fn set_follow_up_mode(&mut self, mode: QueueMode) {
        self.follow_up_mode = mode;
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_drain_follow_up_one_at_a_time() {
        let mut q = MessageQueue::new();
        q.enqueue("first".into(), QueueType::FollowUp);
        q.enqueue("steer".into(), QueueType::Steering);
        q.enqueue("second".into(), QueueType::FollowUp);

        assert_eq!(q.len(), 3);
        assert_eq!(q.drain_follow_up(), vec!["first".to_string()]);
        assert_eq!(q.len(), 2);
        assert_eq!(q.drain_follow_up(), vec!["second".to_string()]);
        assert!(q.drain_follow_up().is_empty());
    }

    #[test]
    fn test_drain_follow_up_all_mode() {
        let mut q = MessageQueue::new();
        q.set_follow_up_mode(QueueMode::All);
        q.enqueue("first".into(), QueueType::FollowUp);
        q.enqueue("steer".into(), QueueType::Steering);
        q.enqueue("second".into(), QueueType::FollowUp);

        let result = q.drain_follow_up();
        assert_eq!(result, vec!["first".to_string(), "second".to_string()]);
        assert_eq!(q.len(), 1); // steering remains
    }

    #[test]
    fn test_drain_steering() {
        let mut q = MessageQueue::new();
        q.enqueue("follow".into(), QueueType::FollowUp);
        q.enqueue("steer1".into(), QueueType::Steering);
        q.enqueue("steer2".into(), QueueType::Steering);

        let steering = q.drain_steering();
        assert_eq!(steering, vec!["steer1".to_string(), "steer2".to_string()]);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn test_drain_steering_one_at_a_time() {
        let mut q = MessageQueue::new();
        q.set_steering_mode(QueueMode::OneAtATime);
        q.enqueue("steer1".into(), QueueType::Steering);
        q.enqueue("steer2".into(), QueueType::Steering);

        assert_eq!(q.drain_steering(), vec!["steer1".to_string()]);
        assert_eq!(q.drain_steering(), vec!["steer2".to_string()]);
    }

    #[test]
    fn test_empty_queue() {
        let mut q = MessageQueue::new();
        assert!(q.is_empty());
        assert!(q.drain_follow_up().is_empty());
        assert!(q.drain_steering().is_empty());
    }

    #[test]
    fn test_clear() {
        let mut q = MessageQueue::new();
        q.enqueue("a".into(), QueueType::FollowUp);
        q.enqueue("b".into(), QueueType::Steering);
        let cleared = q.clear();
        assert!(q.is_empty());
        assert_eq!(cleared.len(), 2);
    }
}
