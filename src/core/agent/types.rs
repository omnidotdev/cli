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
    /// Tool invocation starting — tool_id links start to result for parallel display
    ToolStart {
        tool_id: String,
        name: String,
    },
    /// Tool invocation finished
    ToolCall {
        tool_id: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_start_has_id() {
        let evt = ChatEvent::ToolStart {
            tool_id: "abc-123".to_string(),
            name: "Bash".to_string(),
        };
        if let ChatEvent::ToolStart { tool_id, .. } = evt {
            assert_eq!(tool_id, "abc-123");
        } else {
            panic!("wrong variant");
        }
    }
}
