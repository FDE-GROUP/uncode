//! uncode-tools — 内置工具集
//!
//! 提供 Agent 可调用的文件操作和命令执行工具。
//! 所有工具实现 `ToolExecutor` trait，通过 `ToolRegistry` 注册。

pub mod bash;
pub mod edit;
pub mod find;
pub mod grep;
pub mod local_env;
pub mod ls;
pub mod read;
pub mod registry;
pub mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use find::FindTool;
pub use grep::GrepTool;
pub use local_env::{
    LocalExecutionEnv, LocalFileSystem, LocalShell, clean_binary_output, truncate_output,
};
pub use ls::LsTool;
pub use read::ReadTool;
pub use registry::ToolRegistry;
pub use write::WriteTool;

/// Find the nearest existing ancestor, canonicalize it,
/// then re-attach the non-existing suffix.
fn normalize_path(full: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if let Ok(c) = full.canonicalize() {
        return Ok(c);
    }
    // Walk up to find an existing ancestor
    let mut existing = full.to_path_buf();
    let mut suffix = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            suffix.push(name.to_os_string());
        }
        if !existing.pop() {
            break;
        }
    }
    let canonical_base = existing
        .canonicalize()
        .map_err(|e| format!("resolve path {}: {e}", full.display()))?;
    let mut result = canonical_base;
    for name in suffix.into_iter().rev() {
        result.push(name);
    }
    Ok(result)
}

/// Resolve and validate a path argument.
///
/// - Relative paths are resolved against the current working directory.
/// - The result is canonicalized to eliminate `..` traversal.
/// - Paths that escape the current working directory via `..` are rejected.
fn resolve_path(raw: &str) -> Result<std::path::PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot get cwd: {e}"))?;
    let p = std::path::Path::new(raw);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    };

    let canonical = normalize_path(&full)?;

    // Verify the resolved path is within cwd (prevents .. traversal)
    let canonical_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.clone());
    if !canonical.starts_with(&canonical_cwd) {
        return Err(format!(
            "path '{}' resolves outside the project directory",
            raw
        ));
    }

    Ok(canonical)
}

#[cfg(test)]
mod tests;
