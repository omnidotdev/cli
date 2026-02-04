//! LLM provider abstraction for BYOM (Bring Your Own Model).

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use super::error::Result;
use super::types::{ContentBlock, Message, StopReason, Tool};

/// Reasoning effort level for thinking-capable models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningEffort {
    /// Maximum thinking depth ("xhigh" in API)
    Max,
    /// Good balance of thinking (default, "high" in API)
    #[default]
    High,
    /// Quick thinking ("low" in API)
    Low,
    /// No reasoning (omit from request)
    None,
}

impl ReasoningEffort {
    /// Cycle to next effort level: High → Max → Low → None → High
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::High => Self::Max,
            Self::Max => Self::Low,
            Self::Low => Self::None,
            Self::None => Self::High,
        }
    }

    /// Convert to `OpenRouter` API value. Returns `None` for `ReasoningEffort::None`.
    #[must_use]
    pub const fn to_api_value(self) -> Option<&'static str> {
        match self {
            Self::Max => Some("xhigh"),
            Self::High => Some("high"),
            Self::Low => Some("low"),
            Self::None => None,
        }
    }
}

impl std::fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Max => write!(f, "max"),
            Self::High => write!(f, "high"),
            Self::Low => write!(f, "low"),
            Self::None => write!(f, "none"),
        }
    }
}

/// Configuration for an LLM request.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Model identifier.
    pub model: String,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// System prompt.
    pub system: Option<String>,
    /// Available tools.
    pub tools: Option<Vec<Tool>>,
    /// Reasoning effort level.
    pub reasoning_effort: ReasoningEffort,
}

/// A streaming event from the LLM.
#[derive(Debug, Clone)]
pub enum CompletionEvent {
    /// Start of a thinking block.
    ThinkingStart,
    /// A chunk of thinking content.
    ThinkingDelta(String),
    /// A thinking block has completed.
    ThinkingDone,
    /// A chunk of text content.
    TextDelta(String),
    /// Start of a tool use block.
    ToolUseStart {
        index: usize,
        id: String,
        name: String,
    },
    /// Partial JSON input for a tool.
    ToolInputDelta { index: usize, partial_json: String },
    /// A content block has completed.
    ContentBlockDone { index: usize, block: ContentBlock },
    /// The completion has finished.
    Done {
        stop_reason: Option<StopReason>,
        usage: Option<crate::core::agent::types::Usage>,
    },
    /// An error occurred.
    Error(String),
}

/// Stream of completion events.
pub type CompletionStream = Pin<Box<dyn Stream<Item = Result<CompletionEvent>> + Send>>;

/// Trait for LLM providers.
///
/// Implement this trait to add support for a new LLM provider.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name.
    fn name(&self) -> &'static str;

    /// Stream a completion request.
    ///
    /// Returns a stream of completion events.
    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream>;
}
