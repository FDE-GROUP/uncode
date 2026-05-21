//! 测试用 `ExecutionEnv` 桩实现，验证工具经 `ToolContext.execution_env` 访问 FS/Shell。

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uncode_core::error::UncodeError;
use uncode_core::tool::{
    DirEntry, ExecutionEnv, FileInfo, FileSystem, Shell, ShellOptions, ShellResult,
};

#[derive(Clone)]
pub struct StubFileSystem {
    content: String,
    is_dir: bool,
    read_paths: Arc<Mutex<Vec<PathBuf>>>,
}

impl StubFileSystem {
    pub fn file(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_dir: false,
            read_paths: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn directory() -> Self {
        Self {
            content: String::new(),
            is_dir: true,
            read_paths: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn read_paths(&self) -> Vec<PathBuf> {
        self.read_paths.lock().unwrap().clone()
    }
}

#[async_trait]
impl FileSystem for StubFileSystem {
    async fn read_text_file(&self, path: &Path) -> Result<String, UncodeError> {
        self.read_paths.lock().unwrap().push(path.to_path_buf());
        Ok(self.content.clone())
    }

    async fn write_file(&self, _path: &Path, _content: &str) -> Result<(), UncodeError> {
        Ok(())
    }

    async fn file_info(&self, path: &Path) -> Result<FileInfo, UncodeError> {
        Ok(FileInfo {
            path: path.to_path_buf(),
            size: self.content.len() as u64,
            is_dir: self.is_dir,
            is_file: !self.is_dir,
            is_symlink: false,
            modified: None,
        })
    }

    async fn list_dir(&self, _path: &Path) -> Result<Vec<DirEntry>, UncodeError> {
        Ok(vec![DirEntry {
            name: "stub-entry".into(),
            is_dir: false,
            is_file: true,
            is_symlink: false,
        }])
    }

    async fn exists(&self, _path: &Path) -> Result<bool, UncodeError> {
        Ok(true)
    }

    async fn canonical_path(&self, path: &Path) -> Result<PathBuf, UncodeError> {
        Ok(path.to_path_buf())
    }
}

pub struct StubShell;

#[async_trait]
impl Shell for StubShell {
    async fn exec(&self, _cmd: &str, _opts: ShellOptions) -> Result<ShellResult, UncodeError> {
        Ok(ShellResult {
            stdout: "stub-shell".into(),
            stderr: String::new(),
            exit_code: 0,
            cancelled: false,
        })
    }
}

pub struct StubExecutionEnv {
    fs: StubFileSystem,
    shell: StubShell,
}

impl StubExecutionEnv {
    pub fn with_file_content(content: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            fs: StubFileSystem::file(content),
            shell: StubShell,
        })
    }

    pub fn with_directory_listing() -> Arc<Self> {
        Arc::new(Self {
            fs: StubFileSystem::directory(),
            shell: StubShell,
        })
    }

    pub fn fs(&self) -> &StubFileSystem {
        &self.fs
    }
}

impl ExecutionEnv for StubExecutionEnv {
    fn fs(&self) -> &dyn FileSystem {
        &self.fs
    }

    fn shell(&self) -> &dyn Shell {
        &self.shell
    }
}
