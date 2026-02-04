//! Conversation state management.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::types::{Content, ContentBlock, Message, Role};

/// Maximum size per file (100KB).
const MAX_FILE_SIZE: usize = 100 * 1024;
/// Maximum total size for all files (500KB).
const MAX_TOTAL_SIZE: usize = 500 * 1024;

/// Read file content with size validation.
///
/// # Errors
///
/// Returns an error message if the file doesn't exist, is too large, or can't be read.
pub fn read_file_for_context(path: &str) -> Result<String, String> {
    let file_path = Path::new(path);

    if !file_path.exists() {
        return Err(format!("file not found: {path}"));
    }

    if !file_path.is_file() {
        return Err(format!("not a file: {path}"));
    }

    let metadata =
        fs::metadata(file_path).map_err(|e| format!("cannot read file metadata: {e}"))?;
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);

    if size > MAX_FILE_SIZE {
        return Err(format!(
            "file too large: {path} ({size} bytes, max {MAX_FILE_SIZE} bytes)"
        ));
    }

    fs::read_to_string(file_path).map_err(|e| format!("cannot read file: {e}"))
}

/// Parse @-mentions from text and return list of paths.
///
/// Finds all `@path` patterns where `@` is followed by non-whitespace characters
/// until whitespace or end of text. The `@` prefix is stripped from the returned paths.
#[must_use]
pub fn parse_at_mentions(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let at_start_or_after_whitespace = i == 0 || chars[i - 1].is_whitespace();
        if chars[i] == '@' && at_start_or_after_whitespace {
            let start = i + 1;
            let mut end = start;

            while end < len && !chars[end].is_whitespace() {
                end += 1;
            }

            if end > start {
                let path: String = chars[start..end].iter().collect();
                paths.push(path);
            }

            i = end;
        } else {
            i += 1;
        }
    }

    paths
}

/// Prepare message with file contexts prepended.
///
/// Parses @-mentions from the text, reads each referenced file, validates sizes,
/// and prepends file content wrapped in XML tags to the user text.
///
/// # Errors
///
/// Returns an error message if any file fails to read or total size exceeds limit.
pub fn prepare_message_with_files(user_text: &str) -> Result<String, String> {
    let paths = parse_at_mentions(user_text);

    if paths.is_empty() {
        return Ok(user_text.to_string());
    }

    let mut file_contexts = Vec::new();
    let mut total_size = 0usize;

    for path in &paths {
        let content = read_file_for_context(path)?;
        total_size += content.len();

        if total_size > MAX_TOTAL_SIZE {
            return Err(format!(
                "total file content too large ({total_size} bytes, max {MAX_TOTAL_SIZE} bytes)"
            ));
        }

        file_contexts.push(format!(
            "<file-context path=\"{path}\">\n{content}\n</file-context>"
        ));
    }

    if file_contexts.is_empty() {
        Ok(user_text.to_string())
    } else {
        Ok(format!("{}\n\n{}", file_contexts.join("\n\n"), user_text))
    }
}

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

    #[test]
    fn parse_at_mentions_extracts_paths() {
        assert_eq!(
            parse_at_mentions("check @src/main.rs please"),
            vec!["src/main.rs"]
        );
        assert_eq!(
            parse_at_mentions("@file1.txt and @file2.txt"),
            vec!["file1.txt", "file2.txt"]
        );
        assert_eq!(
            parse_at_mentions("@path/to/file.rs at start"),
            vec!["path/to/file.rs"]
        );
    }

    #[test]
    fn parse_at_mentions_ignores_email_like() {
        assert!(parse_at_mentions("email user@example.com").is_empty());
    }

    #[test]
    fn parse_at_mentions_handles_edge_cases() {
        assert!(parse_at_mentions("no mentions here").is_empty());
        assert!(parse_at_mentions("@ alone").is_empty());
        assert_eq!(parse_at_mentions("@single"), vec!["single"]);
    }

    #[test]
    fn read_file_for_context_handles_missing_file() {
        let result = read_file_for_context("/nonexistent/path/file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file not found"));
    }

    #[test]
    fn prepare_message_no_mentions_returns_original() {
        let text = "plain message without mentions";
        let result = prepare_message_with_files(text).unwrap();
        assert_eq!(result, text);
    }

    #[test]
    fn prepare_message_with_missing_file_returns_error() {
        let result = prepare_message_with_files("check @/nonexistent/file.txt");
        assert!(result.is_err());
    }
}
