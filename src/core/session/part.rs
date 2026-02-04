//! Part types for message content.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::new_part_id;

/// Reference to a file included in a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileReference {
    /// Path to the file.
    pub path: String,
    /// Optional hash of the file content for verification.
    pub content_hash: Option<String>,
}

/// Part of a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Part {
    /// Text content.
    Text(TextPart),
    /// Tool invocation.
    Tool(ToolPart),
    /// Model reasoning (extended thinking).
    Reasoning(ReasoningPart),
}

impl Part {
    /// Get the part ID.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Text(p) => &p.id,
            Self::Tool(p) => &p.id,
            Self::Reasoning(p) => &p.id,
        }
    }

    /// Get the message ID.
    #[must_use]
    pub fn message_id(&self) -> &str {
        match self {
            Self::Text(p) => &p.message_id,
            Self::Tool(p) => &p.message_id,
            Self::Reasoning(p) => &p.message_id,
        }
    }

    /// Get the session ID.
    #[must_use]
    pub fn session_id(&self) -> &str {
        match self {
            Self::Text(p) => &p.session_id,
            Self::Tool(p) => &p.session_id,
            Self::Reasoning(p) => &p.session_id,
        }
    }
}

/// Text part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart {
    /// Unique part identifier.
    pub id: String,
    /// Parent message ID.
    pub message_id: String,
    /// Session ID.
    pub session_id: String,
    /// Text content.
    pub text: String,
    /// Whether this is synthetic (auto-generated).
    #[serde(default)]
    pub synthetic: bool,
    /// Timestamps.
    pub time: Option<PartTime>,
    /// File references included in this message.
    #[serde(default)]
    pub file_references: Vec<FileReference>,
}

impl TextPart {
    /// Create a new text part.
    #[must_use]
    pub fn new(message_id: &str, session_id: &str, text: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: new_part_id(),
            message_id: message_id.to_string(),
            session_id: session_id.to_string(),
            text: text.into(),
            synthetic: false,
            time: Some(PartTime {
                start: now,
                end: Some(now),
            }),
            file_references: Vec::new(),
        }
    }

    /// Create a synthetic text part.
    #[must_use]
    pub fn synthetic(message_id: &str, session_id: &str, text: impl Into<String>) -> Self {
        let mut part = Self::new(message_id, session_id, text);
        part.synthetic = true;
        part
    }

    /// Append text to this part.
    pub fn append(&mut self, text: &str) {
        self.text.push_str(text);
    }
}

/// Tool part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPart {
    /// Unique part identifier.
    pub id: String,
    /// Parent message ID.
    pub message_id: String,
    /// Session ID.
    pub session_id: String,
    /// Tool call ID from the API.
    pub call_id: String,
    /// Tool name.
    pub tool: String,
    /// Tool state.
    pub state: ToolState,
}

impl ToolPart {
    /// Create a new pending tool part.
    #[must_use]
    pub fn new(
        message_id: &str,
        session_id: &str,
        call_id: &str,
        tool: &str,
        input: Value,
    ) -> Self {
        Self {
            id: new_part_id(),
            message_id: message_id.to_string(),
            session_id: session_id.to_string(),
            call_id: call_id.to_string(),
            tool: tool.to_string(),
            state: ToolState::Pending {
                input,
                raw: String::new(),
            },
        }
    }

    /// Mark tool as running.
    pub fn start(&mut self) {
        if let ToolState::Pending { input, .. } = &self.state {
            self.state = ToolState::Running {
                input: input.clone(),
                time: PartTime {
                    start: chrono::Utc::now().timestamp_millis(),
                    end: None,
                },
            };
        }
    }

    /// Mark tool as completed.
    pub fn complete(&mut self, output: impl Into<String>) {
        let now = chrono::Utc::now().timestamp_millis();
        match &self.state {
            ToolState::Running { input, time } => {
                self.state = ToolState::Completed {
                    input: input.clone(),
                    output: output.into(),
                    time: PartTime {
                        start: time.start,
                        end: Some(now),
                    },
                    compacted: None,
                };
            }
            ToolState::Pending { input, .. } => {
                self.state = ToolState::Completed {
                    input: input.clone(),
                    output: output.into(),
                    time: PartTime {
                        start: now,
                        end: Some(now),
                    },
                    compacted: None,
                };
            }
            _ => {}
        }
    }

    /// Mark tool as errored.
    pub fn error(&mut self, error: impl Into<String>) {
        let now = chrono::Utc::now().timestamp_millis();
        let input = match &self.state {
            ToolState::Running { input, .. }
            | ToolState::Pending { input, .. }
            | ToolState::Completed { input, .. }
            | ToolState::Error { input, .. } => input.clone(),
        };
        let start = match &self.state {
            ToolState::Running { time, .. } => time.start,
            _ => now,
        };

        self.state = ToolState::Error {
            input,
            error: error.into(),
            time: PartTime {
                start,
                end: Some(now),
            },
        };
    }

    /// Mark tool output as compacted.
    pub fn compact(&mut self) {
        if let ToolState::Completed { compacted, .. } = &mut self.state {
            *compacted = Some(chrono::Utc::now().timestamp_millis());
        }
    }

    /// Check if the tool output is compacted.
    #[must_use]
    pub const fn is_compacted(&self) -> bool {
        matches!(
            &self.state,
            ToolState::Completed {
                compacted: Some(_),
                ..
            }
        )
    }

    /// Get the tool output if completed.
    #[must_use]
    pub fn output(&self) -> Option<&str> {
        match &self.state {
            ToolState::Completed {
                output, compacted, ..
            } => {
                if compacted.is_some() {
                    Some("[Old tool result content cleared]")
                } else {
                    Some(output)
                }
            }
            _ => None,
        }
    }
}

/// Tool execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ToolState {
    /// Tool is waiting to be executed.
    Pending {
        /// Input parameters.
        input: Value,
        /// Raw JSON string (for streaming).
        raw: String,
    },
    /// Tool is currently executing.
    Running {
        /// Input parameters.
        input: Value,
        /// Execution time.
        time: PartTime,
    },
    /// Tool completed successfully.
    Completed {
        /// Input parameters.
        input: Value,
        /// Output string.
        output: String,
        /// Execution time.
        time: PartTime,
        /// When output was compacted (cleared).
        compacted: Option<i64>,
    },
    /// Tool encountered an error.
    Error {
        /// Input parameters.
        input: Value,
        /// Error message.
        error: String,
        /// Execution time.
        time: PartTime,
    },
}

impl ToolState {
    /// Get a string representation of the state
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending { .. } => "pending",
            Self::Running { .. } => "running",
            Self::Completed { .. } => "completed",
            Self::Error { .. } => "error",
        }
    }
}

/// Reasoning part (extended thinking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningPart {
    /// Unique part identifier.
    pub id: String,
    /// Parent message ID.
    pub message_id: String,
    /// Session ID.
    pub session_id: String,
    /// Reasoning text.
    pub text: String,
    /// Timestamps.
    pub time: PartTime,
}

impl ReasoningPart {
    /// Create a new reasoning part.
    #[must_use]
    pub fn new(message_id: &str, session_id: &str, text: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: new_part_id(),
            message_id: message_id.to_string(),
            session_id: session_id.to_string(),
            text: text.into(),
            time: PartTime {
                start: now,
                end: None,
            },
        }
    }

    /// Append text to this part.
    pub fn append(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// Mark as complete.
    pub fn complete(&mut self) {
        self.time.end = Some(chrono::Utc::now().timestamp_millis());
    }
}

/// Part timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartTime {
    /// When the part started.
    pub start: i64,
    /// When the part ended.
    pub end: Option<i64>,
}
