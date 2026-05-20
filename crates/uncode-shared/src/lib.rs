//! uncode-shared — 共享基础类型（依赖树叶子）。
//!
//! 错误类型、应用配置；无 Agent 机制语义。
//!
//! **Pi:** 配置/错误形状为 uncode 自有；模型与供应商概念对齐 Pi `Model` / provider 表（见 `AppConfig`）。

pub mod config;
pub mod error;

#[cfg(test)]
mod tests;
