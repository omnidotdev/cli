//! Message types for session persistence.

use serde::{Deserialize, Serialize};

use super::new_message_id;

/// Message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// User message.
    User(UserMessage),
    /// Assistant message.
    Assistant(AssistantMessage),
}

impl Message {
    /// Get the message ID.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::User(m) => &m.id,
            Self::Assistant(m) => &m.id,
        }
    }

    /// Get the session ID.
    #[must_use]
    pub fn session_id(&self) -> &str {
        match self {
            Self::User(m) => &m.session_id,
            Self::Assistant(m) => &m.session_id,
        }
    }

    /// Check if this is a user message.
    #[must_use]
    pub const fn is_user(&self) -> bool {
        matches!(self, Self::User(_))
    }

    /// Check if this is an assistant message.
    #[must_use]
    pub const fn is_assistant(&self) -> bool {
        matches!(self, Self::Assistant(_))
    }

    /// Get token usage (only available for assistant messages)
    #[must_use]
    pub const fn usage(&self) -> Option<&TokenUsage> {
        match self {
            Self::User(_) => None,
            Self::Assistant(m) => Some(&m.tokens),
        }
    }
}

/// User message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    /// Unique message identifier.
    pub id: String,

    /// Session this message belongs to.
    pub session_id: String,

    /// Timestamps.
    pub time: MessageTime,

    /// Agent that handled this message.
    pub agent: String,

    /// Model reference.
    pub model: ModelRef,

    /// Summary (auto-generated title).
    pub summary: Option<MessageSummary>,
}

impl UserMessage {
    /// Create a new user message.
    #[must_use]
    pub fn new(session_id: &str, agent: &str, provider_id: &str, model_id: &str) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: new_message_id(),
            session_id: session_id.to_string(),
            time: MessageTime {
                created: now,
                completed: None,
            },
            agent: agent.to_string(),
            model: ModelRef {
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
            },
            summary: None,
        }
    }
}

/// Assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// Unique message identifier.
    pub id: String,

    /// Session this message belongs to.
    pub session_id: String,

    /// Parent user message ID.
    pub parent_id: String,

    /// Timestamps.
    pub time: MessageTime,

    /// Agent that generated this message.
    pub agent: String,

    /// Provider ID.
    pub provider_id: String,

    /// Model ID.
    pub model_id: String,

    /// Token usage.
    pub tokens: TokenUsage,

    /// Cost in USD.
    pub cost: f64,

    /// Error if the message failed.
    pub error: Option<MessageError>,

    /// Whether this is a compaction summary.
    pub is_summary: bool,
}

impl AssistantMessage {
    /// Create a new assistant message.
    #[must_use]
    pub fn new(
        session_id: &str,
        parent_id: &str,
        agent: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: new_message_id(),
            session_id: session_id.to_string(),
            parent_id: parent_id.to_string(),
            time: MessageTime {
                created: now,
                completed: None,
            },
            agent: agent.to_string(),
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            tokens: TokenUsage::default(),
            cost: 0.0,
            error: None,
            is_summary: false,
        }
    }

    /// Mark this message as a compaction summary.
    #[must_use]
    pub const fn as_summary(mut self) -> Self {
        self.is_summary = true;
        self
    }

    /// Mark this message as completed.
    pub fn complete(&mut self) {
        self.time.completed = Some(chrono::Utc::now().timestamp_millis());
    }

    /// Set token usage and calculate cost.
    pub fn set_usage(&mut self, tokens: TokenUsage, cost_per_input: f64, cost_per_output: f64) {
        self.cost = f64::from(tokens.input)
            .mul_add(cost_per_input, f64::from(tokens.output) * cost_per_output)
            / 1_000_000.0;
        self.tokens = tokens;
    }
}

/// Model reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// Provider ID.
    pub provider_id: String,
    /// Model ID.
    pub model_id: String,
}

/// Message timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTime {
    /// When the message was created.
    pub created: i64,
    /// When the message was completed.
    pub completed: Option<i64>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens.
    pub input: u32,
    /// Output tokens.
    pub output: u32,
    /// Cached input tokens read.
    pub cache_read: u32,
    /// Cached input tokens written.
    pub cache_write: u32,
}

impl TokenUsage {
    /// Create token usage from raw values.
    #[must_use]
    pub const fn new(input: u32, output: u32, cache_read: u32, cache_write: u32) -> Self {
        Self {
            input,
            output,
            cache_read,
            cache_write,
        }
    }

    /// Total tokens used.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.input + self.output + self.cache_read
    }
}

/// Message summary (auto-generated).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    /// Auto-generated title.
    pub title: Option<String>,
}

/// Message error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageError {
    /// Error type.
    pub error_type: String,
    /// Error message.
    pub message: String,
    /// Whether the error is retryable.
    pub retryable: bool,
}

impl MessageError {
    /// Create an API error.
    #[must_use]
    pub fn api(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            error_type: "api".to_string(),
            message: message.into(),
            retryable,
        }
    }

    /// Create an auth error.
    #[must_use]
    pub fn auth(message: impl Into<String>) -> Self {
        Self {
            error_type: "auth".to_string(),
            message: message.into(),
            retryable: false,
        }
    }

    /// Create an abort error.
    #[must_use]
    pub fn aborted(message: impl Into<String>) -> Self {
        Self {
            error_type: "aborted".to_string(),
            message: message.into(),
            retryable: false,
        }
    }
}
