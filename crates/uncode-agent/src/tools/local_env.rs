//! LocalExecutionEnv — 基于 std::fs + tokio::process 的本地执行环境

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use uncode_core::error::UncodeError;
use uncode_core::tool::{
    DirEntry, ExecutionEnv, FileInfo, FileSystem, Shell, ShellOptions, ShellResult,
};

// ── LocalFileSystem ──

pub struct LocalFileSystem;

#[async_trait]
impl FileSystem for LocalFileSystem {
    async fn read_text_file(&self, path: &Path) -> Result<String, UncodeError> {
        match tokio::fs::read_to_string(path).await {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| UncodeError::File(e.into()))?;
                let text = clean_binary_output(&bytes);
                Ok(format!(
                    "[注意: 文件含非 UTF-8 字节，以下为替换字符 (U+FFFD) 预览]\n{text}"
                ))
            }
            Err(e) => Err(UncodeError::File(e.into())),
        }
    }

    async fn write_file(&self, path: &Path, content: &str) -> Result<(), UncodeError> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| UncodeError::File(e.into()))?;
        }
        tokio::fs::write(path, content)
            .await
            .map_err(|e| UncodeError::File(e.into()))
    }

    async fn file_info(&self, path: &Path) -> Result<FileInfo, UncodeError> {
        let meta = tokio::fs::symlink_metadata(path)
            .await
            .map_err(|e| UncodeError::File(e.into()))?;
        Ok(FileInfo {
            path: path.to_path_buf(),
            size: meta.len(),
            is_dir: meta.is_dir(),
            is_file: meta.is_file(),
            is_symlink: meta.is_symlink(),
            modified: meta.modified().ok(),
        })
    }

    async fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, UncodeError> {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path)
            .await
            .map_err(|e| UncodeError::File(e.into()))?;
        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| UncodeError::File(e.into()))?
        {
            let meta = entry
                .metadata()
                .await
                .map_err(|e| UncodeError::File(e.into()))?;
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: meta.is_dir(),
                is_file: meta.is_file(),
                is_symlink: meta.file_type().is_symlink(),
            });
        }
        Ok(entries)
    }

    async fn exists(&self, path: &Path) -> Result<bool, UncodeError> {
        Ok(tokio::fs::metadata(path).await.is_ok())
    }

    async fn canonical_path(&self, path: &Path) -> Result<PathBuf, UncodeError> {
        tokio::fs::canonicalize(path)
            .await
            .map_err(|e| UncodeError::File(e.into()))
    }
}

// ── LocalShell ──

pub struct LocalShell;

/// 清理非 UTF-8 字符：替换无效字节为 replacement char
pub fn clean_binary_output(raw: &[u8]) -> String {
    let mut clean = String::with_capacity(raw.len());
    for chunk in raw.utf8_chunks() {
        clean.push_str(chunk.valid());
        if !chunk.invalid().is_empty() {
            clean.push('\u{FFFD}');
        }
    }
    clean
}

/// 截断输出到 max_bytes，超出时追加提示
pub fn truncate_output(output: &str, max_bytes: usize) -> String {
    if output.len() <= max_bytes {
        return output.to_string();
    }
    // 在 max_bytes 附近找 UTF-8 边界
    let mut cut = max_bytes;
    while cut > 0 && !output.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut truncated = output[..cut].to_string();
    truncated.push_str("\n[truncated]");
    truncated
}

#[async_trait]
impl Shell for LocalShell {
    async fn exec(&self, cmd: &str, opts: ShellOptions) -> Result<ShellResult, UncodeError> {
        let timeout = opts.timeout_ms.unwrap_or(120_000);
        let (stdout, stderr, exit_code) =
            super::bash_exec::shell_exec_bash(cmd, opts.workdir.clone(), timeout).await?;
        Ok(ShellResult {
            stdout,
            stderr,
            exit_code,
            cancelled: false,
        })
    }
}

// ── LocalExecutionEnv ──

pub struct LocalExecutionEnv {
    fs: LocalFileSystem,
    shell: LocalShell,
}

impl LocalExecutionEnv {
    pub fn new() -> Self {
        Self {
            fs: LocalFileSystem,
            shell: LocalShell,
        }
    }
}

impl Default for LocalExecutionEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEnv for LocalExecutionEnv {
    fn fs(&self) -> &dyn FileSystem {
        &self.fs
    }
    fn shell(&self) -> &dyn Shell {
        &self.shell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_binary_output_valid_utf8() {
        let input = "hello world".as_bytes();
        let output = clean_binary_output(input);
        assert_eq!(output, "hello world");
    }

    #[test]
    fn test_clean_binary_output_invalid_bytes() {
        // "hello" + 0xFF (invalid) + " world"
        let mut input = b"hello".to_vec();
        input.push(0xFF);
        input.extend_from_slice(b" world");
        let output = clean_binary_output(&input);
        assert!(output.contains("hello"));
        assert!(output.contains("\u{FFFD}"));
        assert!(output.contains("world"));
    }

    #[test]
    fn test_clean_binary_output_empty() {
        assert_eq!(clean_binary_output(&[]), "");
    }

    #[test]
    fn test_truncate_output_under_limit() {
        let output = truncate_output("short", 100);
        assert_eq!(output, "short");
    }

    #[test]
    fn test_truncate_output_exact_limit() {
        let s = "x".repeat(50);
        let output = truncate_output(&s, 50);
        assert_eq!(output, s);
        assert!(!output.contains("[truncated]"));
    }

    #[test]
    fn test_truncate_output_over_limit() {
        let s = "x".repeat(100);
        let output = truncate_output(&s, 50);
        assert!(output.contains("[truncated]"));
        assert!(output.len() < 100);
    }

    #[test]
    fn test_truncate_output_multibyte_boundary() {
        // "你好" is 6 bytes, truncate at 5 → find boundary at 3
        let s = "你好世界";
        let output = truncate_output(s, 5);
        assert!(output.contains("[truncated]"));
        // Should not panic on UTF-8 boundary
    }

    #[tokio::test]
    async fn test_local_fs_read_invalid_utf8_lossy() {
        let dir = std::env::temp_dir().join(format!("uncode-test-utf8-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("binary.txt");
        std::fs::write(&file_path, b"ok\xff\xfe").unwrap();

        let fs = LocalFileSystem;
        let content = fs.read_text_file(&file_path).await.unwrap();
        assert!(content.contains("非 UTF-8"));
        assert!(content.contains('\u{FFFD}'));
        assert!(content.contains("ok"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_local_fs_write_and_read() {
        let dir = std::env::temp_dir().join(format!("uncode-test-fs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let fs = LocalFileSystem;
        let file_path = dir.join("test.txt");

        fs.write_file(&file_path, "hello").await.unwrap();
        let content = fs.read_text_file(&file_path).await.unwrap();
        assert_eq!(content, "hello");

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_local_fs_exists() {
        let dir = std::env::temp_dir().join(format!("uncode-test-exists-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let fs = LocalFileSystem;

        assert!(fs.exists(&dir).await.unwrap());
        assert!(!fs.exists(&dir.join("nonexistent")).await.unwrap());

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_local_fs_file_info() {
        let dir = std::env::temp_dir().join(format!("uncode-test-info-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let fs = LocalFileSystem;
        let file_path = dir.join("info.txt");
        std::fs::write(&file_path, "content").unwrap();

        let info = fs.file_info(&file_path).await.unwrap();
        assert!(info.is_file);
        assert!(!info.is_dir);
        assert_eq!(info.size, 7);

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_local_fs_list_dir() {
        let dir = std::env::temp_dir().join(format!("uncode-test-ls-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::create_dir(dir.join("subdir")).unwrap();

        let fs = LocalFileSystem;
        let entries = fs.list_dir(&dir).await.unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"subdir"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn test_local_shell_exec() {
        let shell = LocalShell;
        let result = shell
            .exec("echo hello", ShellOptions::default())
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_local_shell_exec_failure() {
        let shell = LocalShell;
        let result = shell
            .exec("exit 42", ShellOptions::default())
            .await
            .unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[test]
    fn test_execution_env_accessors() {
        let env = LocalExecutionEnv::new();
        let _ = env.fs() as &dyn FileSystem;
        let _ = env.shell() as &dyn Shell;
    }
}
