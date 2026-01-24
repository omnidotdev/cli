//! Error types for the core module.

/// Core error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Task execution failed.
    #[error("task execution failed: {0}")]
    TaskFailed(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for core operations.
pub type Result<T> = std::result::Result<T, Error>;
