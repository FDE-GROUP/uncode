use thiserror::Error;

/// uncode 统一错误类型，覆盖所有子系统的错误
#[derive(Error, Debug)]
pub enum UncodeError {
    #[error("LLM error: {0}")]
    Llm(String),

    #[error("LLM authentication failed: {0}")]
    LlmAuth(String),

    #[error("LLM rate limited: {0}")]
    LlmRateLimit(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Tool '{name}' not found")]
    ToolNotFound { name: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("{0}")]
    Other(String),
}

/// uncode 通用 Result 类型别名
pub type UncodeResult<T> = Result<T, UncodeError>;
