//! uncode-extensions — WASM 扩展与生命周期钩子（L3 扩展层）。
//!
//! 提供 WASM 沙箱运行时和 Agent 生命周期钩子；扩展在 hook 点拦截或增强行为。
//!
//! **Pi:** 哲学对齐 Pi Extension / Pi Package（TS 运行时）；uncode 以 WASM + `HookRegistry` 分发。
//! **OpenCode:** 对照插件生态；uncode 不以 MCP 为主路径。

pub mod api;
pub mod command;
pub mod hooks;
pub mod loader;
pub mod tool;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(test)]
mod tests;
