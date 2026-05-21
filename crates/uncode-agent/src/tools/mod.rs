//! Built-in tool collection for the agent.
//!
//! Provides file operations and command execution tools.
//! All tools implement the `ToolExecutor` trait and are registered via `ToolRegistry`.

pub mod bash;
pub mod bash_exec;
pub mod builtin;
pub mod diff;
pub mod edit;
pub mod find;
pub mod grep;
pub mod hashline;
pub mod local_env;
pub mod ls;
pub mod read;
pub mod registry;
pub mod url_safety;
pub mod web_fetch;
pub mod web_search;
pub mod write;

pub use bash::BashTool;
pub use builtin::{
    PI_BUILTIN_TOOL_NAMES, ToolLaunchConfig, apply_pi_default_active_tools, configure_active_tools,
    is_pi_builtin_tool, new_pi_coding_registry, register_coding_tools,
    register_coding_tools_and_configure,
};
pub use diff::unified_diff;
pub use diff::{DiffLine, DiffStats, Hunk, Patch};
pub use edit::EditTool;
pub use find::FindTool;
pub use grep::GrepTool;
pub use hashline::{compute_line_hash, parse_anchor, validate_anchors};
pub use local_env::{
    LocalExecutionEnv, LocalFileSystem, LocalShell, clean_binary_output, truncate_output,
};
pub use ls::LsTool;
pub use read::ReadTool;
pub use registry::ToolRegistry;
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;
pub use write::WriteTool;

use std::sync::{Arc, OnceLock};

use uncode_core::tool::ExecutionEnv;

/// Process-wide default when [`ToolContext::execution_env`] is unset.
pub fn default_execution_env() -> Arc<dyn ExecutionEnv> {
    static ENV: OnceLock<Arc<dyn ExecutionEnv>> = OnceLock::new();
    ENV.get_or_init(|| Arc::new(LocalExecutionEnv::new()))
        .clone()
}

/// Effective execution environment for a tool call.
pub(crate) fn ctx_execution_env(ctx: &uncode_core::tool::ToolContext) -> Arc<dyn ExecutionEnv> {
    ctx.execution_env
        .clone()
        .unwrap_or_else(default_execution_env)
}

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

/// Write `content` to `path` atomically using a unique temp file in the target directory.
pub(crate) fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    use std::io::Write;

    let display = path.display().to_string();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));

    std::fs::create_dir_all(parent).map_err(|e| format!("write {display}: {e}"))?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("write {display}: temp file: {e}"))?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| format!("write {display}: {e}"))?;
    tmp.flush().map_err(|e| format!("write {display}: {e}"))?;

    tmp.persist(path)
        .map_err(|e| format!("write {display}: {}", e.error))?;
    Ok(())
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
