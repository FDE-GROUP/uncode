use std::path::PathBuf;
use thiserror::Error;

// ── Stable error code ranges ──
// FileError:     1000-1099
// ExecutionError: 2000-2099
// CompactionError: 3000-3099
// BranchSummaryError: 4000-4099
// HarnessError:  5000-5099

/// 文件操作错误
#[derive(Error, Debug, Clone)]
pub enum FileError {
    #[error("File not found: {path}")]
    NotFound { path: PathBuf, code: u32 },
    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf, code: u32 },
    #[error("Path outside sandbox: {path}")]
    SandboxViolation { path: PathBuf, code: u32 },
    #[error("File too large: {path} ({size} bytes)")]
    TooLarge { path: PathBuf, size: u64, code: u32 },
    #[error("IO error: {message}")]
    Io { message: String, code: u32 },
    #[error("{message}")]
    Other { message: String, code: u32 },
}

impl FileError {
    pub fn code(&self) -> u32 {
        match self {
            Self::NotFound { code, .. } => *code,
            Self::PermissionDenied { code, .. } => *code,
            Self::SandboxViolation { code, .. } => *code,
            Self::TooLarge { code, .. } => *code,
            Self::Io { code, .. } => *code,
            Self::Other { code, .. } => *code,
        }
    }

    pub fn not_found(path: impl Into<PathBuf>) -> Self {
        Self::NotFound {
            path: path.into(),
            code: 1001,
        }
    }

    pub fn permission_denied(path: impl Into<PathBuf>) -> Self {
        Self::PermissionDenied {
            path: path.into(),
            code: 1002,
        }
    }

    pub fn sandbox_violation(path: impl Into<PathBuf>) -> Self {
        Self::SandboxViolation {
            path: path.into(),
            code: 1003,
        }
    }

    pub fn too_large(path: impl Into<PathBuf>, size: u64) -> Self {
        Self::TooLarge {
            path: path.into(),
            size,
            code: 1004,
        }
    }
}

impl From<std::io::Error> for FileError {
    fn from(e: std::io::Error) -> Self {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => 1001,
            std::io::ErrorKind::PermissionDenied => 1002,
            _ => 1099,
        };
        Self::Io {
            message: e.to_string(),
            code,
        }
    }
}

/// Shell 执行错误
#[derive(Error, Debug, Clone)]
pub enum ExecutionError {
    #[error("Command failed (exit {exit_code}): {command}")]
    NonZeroExit {
        command: String,
        exit_code: i32,
        code: u32,
    },
    #[error("Command timed out after {timeout_ms}ms: {command}")]
    Timeout {
        command: String,
        timeout_ms: u64,
        code: u32,
    },
    #[error("Command cancelled: {command}")]
    Cancelled { command: String, code: u32 },
    #[error("{message}")]
    Other { message: String, code: u32 },
}

impl ExecutionError {
    pub fn code(&self) -> u32 {
        match self {
            Self::NonZeroExit { code, .. } => *code,
            Self::Timeout { code, .. } => *code,
            Self::Cancelled { code, .. } => *code,
            Self::Other { code, .. } => *code,
        }
    }

    pub fn non_zero_exit(command: impl Into<String>, exit_code: i32) -> Self {
        Self::NonZeroExit {
            command: command.into(),
            exit_code,
            code: 2001,
        }
    }

    pub fn timeout(command: impl Into<String>, timeout_ms: u64) -> Self {
        Self::Timeout {
            command: command.into(),
            timeout_ms,
            code: 2002,
        }
    }

    pub fn cancelled(command: impl Into<String>) -> Self {
        Self::Cancelled {
            command: command.into(),
            code: 2003,
        }
    }
}

/// 上下文压缩错误
#[derive(Error, Debug, Clone)]
pub enum CompactionError {
    #[error("Compaction LLM call failed: {message}")]
    LlmFailed { message: String, code: u32 },
    #[error("Cut point not found")]
    CutPointNotFound { code: u32 },
    #[error("Session not available: {message}")]
    SessionUnavailable { message: String, code: u32 },
    #[error("{message}")]
    Other { message: String, code: u32 },
}

impl CompactionError {
    pub fn code(&self) -> u32 {
        match self {
            Self::LlmFailed { code, .. } => *code,
            Self::CutPointNotFound { code } => *code,
            Self::SessionUnavailable { code, .. } => *code,
            Self::Other { code, .. } => *code,
        }
    }

    pub fn llm_failed(message: impl Into<String>) -> Self {
        Self::LlmFailed {
            message: message.into(),
            code: 3001,
        }
    }

    pub fn cut_point_not_found() -> Self {
        Self::CutPointNotFound { code: 3002 }
    }
}

/// 分支摘要错误
#[derive(Error, Debug, Clone)]
pub enum BranchSummaryError {
    #[error("Branch summary LLM call failed: {message}")]
    LlmFailed { message: String, code: u32 },
    #[error("Target entry not found: {target_id}")]
    TargetNotFound { target_id: String, code: u32 },
    #[error("{message}")]
    Other { message: String, code: u32 },
}

impl BranchSummaryError {
    pub fn code(&self) -> u32 {
        match self {
            Self::LlmFailed { code, .. } => *code,
            Self::TargetNotFound { code, .. } => *code,
            Self::Other { code, .. } => *code,
        }
    }
}

/// Harness 编排层错误
#[derive(Error, Debug, Clone)]
pub enum HarnessError {
    #[error("Agent is busy (phase: {phase})")]
    Busy { phase: String, code: u32 },
    #[error("Session required but not set")]
    NoSession { code: u32 },
    #[error("{message}")]
    Other { message: String, code: u32 },
}

impl HarnessError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Busy { code, .. } => *code,
            Self::NoSession { code } => *code,
            Self::Other { code, .. } => *code,
        }
    }

    pub fn busy(phase: impl Into<String>) -> Self {
        Self::Busy {
            phase: phase.into(),
            code: 5001,
        }
    }

    pub fn no_session() -> Self {
        Self::NoSession { code: 5002 }
    }
}

/// uncode 统一错误类型，覆盖所有子系统的错误
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum UncodeError {
    // ── 结构化子错误 ──
    #[error("{0}")]
    File(#[from] FileError),

    #[error("{0}")]
    Execution(#[from] ExecutionError),

    #[error("{0}")]
    Compaction(#[from] CompactionError),

    #[error("{0}")]
    BranchSummary(#[from] BranchSummaryError),

    #[error("{0}")]
    Harness(#[from] HarnessError),

    // ── LLM 错误 ──
    #[error("LLM error: {0}")]
    Llm(String),

    #[error("LLM authentication failed: {0}")]
    LlmAuth(String),

    #[error("LLM rate limited: {0}")]
    LlmRateLimit(String),

    // ── 其他 ──
    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Tool '{name}' not found")]
    ToolNotFound { name: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("{0}")]
    Other(String),
}

/// 向后兼容：Io 错误自动路由到 File variant
impl From<std::io::Error> for UncodeError {
    fn from(e: std::io::Error) -> Self {
        Self::File(FileError::from(e))
    }
}

/// uncode 通用 Result 类型别名
pub type UncodeResult<T> = Result<T, UncodeError>;
