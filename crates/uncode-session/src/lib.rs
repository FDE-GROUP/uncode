//! uncode-session — 会话持久化层
//!
//! 将 Agent 对话记录以 JSONL 格式写入本地文件。
//! `SessionStore` 负责底层文件 I/O，`SessionManager` 提供高级会话管理 API。

pub mod manager;
pub mod store;

pub use manager::SessionManager;
pub use store::{SessionError, SessionResult, SessionStore};

#[cfg(test)]
mod tests;
