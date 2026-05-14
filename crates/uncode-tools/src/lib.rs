//! uncode-tools — 内置工具集
//!
//! 提供 Agent 可调用的文件操作和命令执行工具。
//! 所有工具实现 `ToolExecutor` trait，通过 `ToolRegistry` 注册。

pub mod bash;
pub mod edit;
pub mod grep;
pub mod read;
pub mod registry;
pub mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use registry::ToolRegistry;
pub use write::WriteTool;

#[cfg(test)]
mod tests;
