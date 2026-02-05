//! Conversation state management.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::{Content, ContentBlock, Message, Role};

/// Manages multi-turn conversation state.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Conversation {
    messages: Vec<Message>,
    system: Option<String>,
}

impl Conversation {
    /// Create a new conversation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a conversation with a system prompt.
    #[must_use]
    pub fn with_system(system: impl Into<String>) -> Self {
        Self {
            messages: Vec::new(),
            system: Some(system.into()),
        }
    }

    /// Get the system prompt.
    #[must_use]
    pub fn system(&self) -> Option<&str> {
        self.system.as_deref()
    }

    /// Set or replace the system prompt.
    pub fn set_system(&mut self, system: impl Into<String>) {
        self.system = Some(system.into());
    }

    /// Get all messages.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Add a user message.
    pub fn add_user_message(&mut self, text: impl Into<String>) {
        self.messages.push(Message {
            role: Role::User,
            content: Content::Text(text.into()),
        });
    }

    /// Add an assistant message.
    pub fn add_assistant_message(&mut self, text: impl Into<String>) {
        self.messages.push(Message {
            role: Role::Assistant,
            content: Content::Text(text.into()),
        });
    }

    /// Add an assistant message with content blocks (for tool use).
    pub fn add_assistant_blocks(&mut self, blocks: Vec<ContentBlock>) {
        self.messages.push(Message {
            role: Role::Assistant,
            content: Content::Blocks(blocks),
        });
    }

    /// Add a tool result.
    pub fn add_tool_result(&mut self, tool_use_id: String, content: String, is_error: bool) {
        let block = ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error: if is_error { Some(true) } else { None },
        };

        // Tool results go in user messages
        self.messages.push(Message {
            role: Role::User,
            content: Content::Blocks(vec![block]),
        });
    }

    /// Clear all messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Save conversation to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load conversation from a file.
    ///
    /// Returns a new empty conversation if the file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)?;
        let conversation: Self = serde_json::from_str(&contents)?;
        Ok(conversation)
    }

    /// Check if the conversation has any messages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_tracks_messages() {
        let mut conv = Conversation::new();

        conv.add_user_message("Hello");
        conv.add_assistant_message("Hi there!");

        assert_eq!(conv.messages().len(), 2);
        assert_eq!(conv.messages()[0].role, Role::User);
        assert_eq!(conv.messages()[1].role, Role::Assistant);
    }

    #[test]
    fn conversation_clears() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.clear();

        assert!(conv.messages().is_empty());
    }
}
