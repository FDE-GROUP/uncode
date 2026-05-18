//! 会话持久化层
//!
//! SurrealDB v3 异步存储后端。
//! `SessionStore` 封装 `SurrealSessionStore`，所有方法为 async。

pub mod export;
pub mod import;
pub mod manager;
pub mod migration;
pub mod store;
pub mod surreal_store;

pub use manager::SessionManager;
pub use store::{SessionError, SessionResult, SessionStore};

#[cfg(test)]
mod tests;
