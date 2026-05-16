//! uncode-tools — 内置工具集
//!
//! 提供 Agent 可调用的文件操作和命令执行工具。
//! 所有工具实现 `ToolExecutor` trait，通过 `ToolRegistry` 注册。

pub mod bash;
pub mod edit;
pub mod find;
pub mod grep;
pub mod ls;
pub mod read;
pub mod registry;
pub mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use find::FindTool;
pub use grep::GrepTool;
pub use ls::LsTool;
pub use read::ReadTool;
pub use registry::ToolRegistry;
pub use write::WriteTool;

/// Resolve a path argument: absolute paths are kept as-is,
/// relative paths are resolved against the current working directory.
fn resolve_path(raw: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

#[cfg(test)]
mod tests;
