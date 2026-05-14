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

/// 排队消息
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub text: String,
    pub queue_type: QueueType,
}

/// TUI 侧消息队列
pub struct MessageQueue {
    messages: Vec<QueuedMessage>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// 添加排队消息
    pub fn enqueue(&mut self, text: String, queue_type: QueueType) {
        self.messages.push(QueuedMessage { text, queue_type });
    }

    /// 取出第一条 follow-up 消息
    pub fn drain_follow_up(&mut self) -> Option<String> {
        let idx = self
            .messages
            .iter()
            .position(|m| m.queue_type == QueueType::FollowUp)?;
        Some(self.messages.remove(idx).text)
    }

    /// 取出所有 steering 消息（工具调用间隙投递）
    pub fn drain_steering(&mut self) -> Vec<String> {
        let steering: Vec<String> = self
            .messages
            .iter()
            .filter(|m| m.queue_type == QueueType::Steering)
            .map(|m| m.text.clone())
            .collect();
        self.messages
            .retain(|m| m.queue_type != QueueType::Steering);
        steering
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

    /// 清空队列
    pub fn clear(&mut self) {
        self.messages.clear();
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
    fn test_enqueue_and_drain_follow_up() {
        let mut q = MessageQueue::new();
        q.enqueue("first".into(), QueueType::FollowUp);
        q.enqueue("steer".into(), QueueType::Steering);
        q.enqueue("second".into(), QueueType::FollowUp);

        assert_eq!(q.len(), 3);
        assert_eq!(q.drain_follow_up(), Some("first".into()));
        assert_eq!(q.len(), 2);
        assert_eq!(q.drain_follow_up(), Some("second".into()));
        assert_eq!(q.drain_follow_up(), None);
    }

    #[test]
    fn test_drain_steering() {
        let mut q = MessageQueue::new();
        q.enqueue("follow".into(), QueueType::FollowUp);
        q.enqueue("steer1".into(), QueueType::Steering);
        q.enqueue("steer2".into(), QueueType::Steering);

        let steering = q.drain_steering();
        assert_eq!(steering, vec!["steer1".to_string(), "steer2".to_string()]);
        assert_eq!(q.len(), 1); // follow-up remains
    }

    #[test]
    fn test_empty_queue() {
        let mut q = MessageQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.drain_follow_up(), None);
        assert!(q.drain_steering().is_empty());
    }

    #[test]
    fn test_clear() {
        let mut q = MessageQueue::new();
        q.enqueue("a".into(), QueueType::FollowUp);
        q.enqueue("b".into(), QueueType::Steering);
        q.clear();
        assert!(q.is_empty());
    }
}
