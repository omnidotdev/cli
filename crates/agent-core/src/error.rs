//! Agent error types.

/// Agent-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// API key not configured.
    #[error("API key not configured")]
    ApiKeyMissing,

    /// Transport-level error (HTTP, gRPC, etc).
    #[error("transport error: {0}")]
    Transport(String),

    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned an error response.
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    /// Failed to parse API response.
    #[error("failed to parse response: {0}")]
    Parse(String),

    /// Tool execution failed.
    #[error("tool execution failed: {0}")]
    ToolExecution(String),

    /// Stream ended unexpectedly.
    #[error("stream ended unexpectedly")]
    StreamEnded,

    /// Configuration or file I/O error.
    #[error("config error: {0}")]
    Config(String),

    /// Agent entered an infinite loop.
    #[error("loop detected: {0}")]
    LoopDetected(String),
}

/// Result type for agent operations.
pub type Result<T> = std::result::Result<T, AgentError>;
