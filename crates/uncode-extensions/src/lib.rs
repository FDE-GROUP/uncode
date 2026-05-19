//! uncode-extensions — WASM 扩展系统
//!
//! 提供 WASM 沙箱运行时和 Agent 生命周期钩子系统。
//! 扩展可以通过生命周期钩子拦截和增强 Agent 行为。

pub mod api;
pub mod hooks;
pub mod loader;

#[cfg(test)]
mod tests;
