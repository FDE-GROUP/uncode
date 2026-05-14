//! uncode-core — 共享类型系统
//!
//! 定义 uncode 项目所有 crate 共用的数据类型、trait 和错误类型。
//! 位于依赖树的最底层，不依赖任何内部 crate。

pub mod config;
pub mod context;
pub mod error;
pub mod event;
pub mod message;
pub mod model;
pub mod session;
pub mod tool;

#[cfg(test)]
mod tests;
