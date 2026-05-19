//! uncode-shared — 共享基础类型
//!
//! 零业务语义的共享基础设施：错误类型、配置类型。
//! 位于依赖树最底层，不依赖任何内部 crate。

pub mod config;
pub mod error;

#[cfg(test)]
mod tests;
