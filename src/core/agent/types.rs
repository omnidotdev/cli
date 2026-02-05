//! Agent types.
//!
//! Re-exports from agent-core, plus CLI-specific types.

pub use agent_core::types::{
    Content, ContentBlock, Message, MessagesRequest, Role, StopReason, StreamEvent, Tool,
};

/// Events emitted during chat for UI rendering
#[derive(Debug, Clone)]
pub enum ChatEvent {
    /// Text chunk from the assistant
    Text(String),
    /// Tool invocation starting (for activity status)
    ToolStart { name: String },
    /// Tool invocation with result
    ToolCall {
        name: String,
        invocation: String,
        output: String,
        is_error: bool,
    },
    /// Token usage and cost for the response
    Usage {
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
    },
}
