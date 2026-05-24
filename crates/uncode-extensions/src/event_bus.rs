//! 跨扩展事件总线 (#393)
//!
//! 允许扩展通过自定义 channel 互相通信。
//! 对标 Pi 的 `pi.events.emit(channel, data)` / `pi.events.on(channel, handler)` 机制。

use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 事件订阅处理器
pub type EventHandler = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// 订阅 ID，用于取消订阅
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

/// 跨扩展事件总线
///
/// 扩展通过 `emit` 发布自定义事件，通过 `subscribe` 订阅事件。
/// 事件在 host 侧同步分发，不经过 WASM 回调（避免重入问题）。
pub struct EventBus {
    /// channel → 订阅列表
    subscriptions: DashMap<String, Vec<(SubscriptionId, EventHandler)>>,
    next_id: AtomicU64,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscriptions: DashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    /// 发布事件到指定 channel
    pub fn emit(&self, channel: &str, data: serde_json::Value) {
        if let Some(subs) = self.subscriptions.get(channel) {
            for (_, handler) in subs.value() {
                handler(channel, data.clone());
            }
        }
    }

    /// 订阅指定 channel 的事件，返回订阅 ID
    pub fn subscribe(&self, channel: &str, handler: EventHandler) -> SubscriptionId {
        let id = SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.subscriptions
            .entry(channel.to_string())
            .or_default()
            .push((id, handler));
        id
    }

    /// 取消订阅
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut found = false;
        let channel_keys: Vec<String> =
            self.subscriptions.iter().map(|e| e.key().clone()).collect();
        for key in &channel_keys {
            if let Some(mut entry) = self.subscriptions.get_mut(key) {
                let before = entry.len();
                entry.retain(|(sid, _)| *sid != id);
                if entry.len() < before {
                    found = true;
                }
            }
        }
        // Clean up empty channels
        for key in &channel_keys {
            self.subscriptions.remove_if(key, |_, v| v.is_empty());
        }
        found
    }

    /// 获取指定 channel 的订阅数量
    pub fn subscription_count(&self, channel: &str) -> usize {
        self.subscriptions
            .get(channel)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// 清除所有订阅
    pub fn clear(&self) {
        self.subscriptions.clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn test_emit_delivers_to_subscribers() {
        let bus = EventBus::new();
        let received = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        bus.subscribe(
            "test-channel",
            Arc::new(move |ch, data| {
                r.lock().unwrap().push((ch.to_string(), data));
            }),
        );
        bus.emit("test-channel", serde_json::json!({"key": "value"}));
        let msgs = received.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0, "test-channel");
        assert_eq!(msgs[0].1["key"], "value");
    }

    #[test]
    fn test_emit_ignores_unsubscribed_channel() {
        let bus = EventBus::new();
        bus.emit("nonexistent", serde_json::json!("data"));
        // No panic = pass
    }

    #[test]
    fn test_multiple_subscribers() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicU64::new(0));
        let c1 = count.clone();
        let c2 = count.clone();
        bus.subscribe(
            "chan",
            Arc::new(move |_, _| {
                c1.fetch_add(1, Ordering::Relaxed);
            }),
        );
        bus.subscribe(
            "chan",
            Arc::new(move |_, _| {
                c2.fetch_add(1, Ordering::Relaxed);
            }),
        );
        bus.emit("chan", serde_json::json!(null));
        assert_eq!(count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_unsubscribe() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicU64::new(0));
        let c = count.clone();
        let id = bus.subscribe(
            "chan",
            Arc::new(move |_, _| {
                c.fetch_add(1, Ordering::Relaxed);
            }),
        );
        bus.emit("chan", serde_json::json!(null));
        assert_eq!(count.load(Ordering::Relaxed), 1);
        assert!(bus.unsubscribe(id));
        bus.emit("chan", serde_json::json!(null));
        assert_eq!(count.load(Ordering::Relaxed), 1); // No increment
    }

    #[test]
    fn test_unsubscribe_unknown_id() {
        let bus = EventBus::new();
        assert!(!bus.unsubscribe(SubscriptionId(999)));
    }
}
