//! Session compaction for context management
//!
//! When a session's context exceeds the threshold, older messages are
//! summarized and replaced with a compact summary to free up context space

use super::{Message, Part, Session, SessionManager};

/// Default token threshold before triggering compaction
pub const DEFAULT_COMPACTION_THRESHOLD: u32 = 100_000;

/// Minimum messages to keep before compacting
pub const MIN_MESSAGES_TO_KEEP: usize = 4;

/// Compaction result containing the summary
#[derive(Debug)]
pub struct CompactionResult {
    /// Number of messages compacted
    pub messages_compacted: usize,
    /// Estimated tokens freed
    pub tokens_freed: u32,
    /// The summary text
    pub summary: String,
}

impl SessionManager {
    /// Check if a session needs compaction based on token count
    ///
    /// # Errors
    ///
    /// Returns error if message listing fails
    pub fn needs_compaction(&self, session_id: &str, threshold: u32) -> anyhow::Result<bool> {
        let messages = self.list_messages(session_id)?;
        let total_tokens: u32 = messages
            .iter()
            .filter_map(|m| m.usage())
            .map(super::message::TokenUsage::total)
            .sum();

        Ok(total_tokens > threshold)
    }

    /// Estimate total tokens in a session
    ///
    /// # Errors
    ///
    /// Returns error if message listing fails
    pub fn estimate_session_tokens(&self, session_id: &str) -> anyhow::Result<u32> {
        let messages = self.list_messages(session_id)?;
        let total: u32 = messages
            .iter()
            .filter_map(|m| m.usage())
            .map(super::message::TokenUsage::total)
            .sum();
        Ok(total)
    }

    /// Get messages eligible for compaction (older messages, excluding recent)
    ///
    /// # Errors
    ///
    /// Returns error if message listing fails
    pub fn get_compactable_messages(
        &self,
        session_id: &str,
        keep_recent: usize,
    ) -> anyhow::Result<Vec<Message>> {
        let messages = self.list_messages(session_id)?;
        let keep = keep_recent.max(MIN_MESSAGES_TO_KEEP);

        if messages.len() <= keep {
            return Ok(Vec::new());
        }

        let compact_count = messages.len() - keep;
        Ok(messages.into_iter().take(compact_count).collect())
    }

    /// Build context for summarization from messages and their parts
    ///
    /// # Errors
    ///
    /// Returns error if part listing fails
    pub fn build_compaction_context(&self, messages: &[Message]) -> anyhow::Result<String> {
        use std::fmt::Write;
        let mut context = String::new();

        for msg in messages {
            let role = match msg {
                Message::User(_) => "User",
                Message::Assistant(_) => "Assistant",
            };

            let _ = writeln!(context, "\n## {role}");

            // Get parts for this message
            let parts = self.list_parts(msg.id())?;
            for part in parts {
                match part {
                    Part::Text(t) => {
                        context.push_str(&t.text);
                        context.push('\n');
                    }
                    Part::Tool(t) => {
                        let _ = writeln!(context, "[Tool: {} - {}]", t.tool, t.state.as_str());
                    }
                    Part::Reasoning(r) => {
                        let truncated: String = r.text.chars().take(100).collect();
                        let _ = writeln!(context, "[Reasoning: {truncated}...]");
                    }
                }
            }
        }

        Ok(context)
    }

    /// Delete compacted messages and their parts
    ///
    /// # Errors
    ///
    /// Returns error if deletion fails
    pub fn delete_compacted_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> anyhow::Result<()> {
        for msg in messages {
            self.delete_message(session_id, msg.id())?;
        }
        Ok(())
    }

    /// Update session with compaction info
    ///
    /// # Errors
    ///
    /// Returns error if update fails
    pub fn mark_session_compacted(&self, session_id: &str) -> anyhow::Result<Session> {
        Ok(self.storage().update(
            &["session", &self.project().id, session_id],
            |s: &mut Session| {
                s.time.compacted = Some(chrono::Utc::now().timestamp_millis());
            },
        )?)
    }
}

/// Generate a compaction prompt for the LLM
#[must_use]
pub fn compaction_prompt(context: &str) -> String {
    format!(
        r"Summarize this conversation history concisely. Preserve:
- Key decisions and their reasoning
- Important file paths and code snippets mentioned
- Unresolved issues or pending tasks
- Technical context needed to continue

Keep the summary under 2000 tokens. Focus on actionable information.

Conversation:
{context}

Summary:"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::{Project, ProjectTime};
    use crate::core::session::UserMessage;
    use crate::core::storage::Storage;

    fn temp_manager() -> (SessionManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::with_root(dir.path().to_path_buf());
        let project = Project {
            id: "test-project".to_string(),
            worktree: dir.path().to_path_buf(),
            vcs: Some("git".to_string()),
            time: ProjectTime {
                created: 0,
                initialized: 0,
            },
        };
        (SessionManager::new(storage, project), dir)
    }

    #[test]
    fn get_compactable_messages_respects_minimum() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        // Add 3 messages
        for _ in 0..3 {
            let msg = Message::User(UserMessage::new(
                &session.id,
                "build",
                "anthropic",
                "claude",
            ));
            manager.save_message(&session.id, &msg).unwrap();
        }

        // Should return empty since we have fewer than MIN_MESSAGES_TO_KEEP
        let compactable = manager.get_compactable_messages(&session.id, 2).unwrap();
        assert!(compactable.is_empty());
    }

    #[test]
    fn get_compactable_messages_returns_older() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        // Add 6 messages
        for _ in 0..6 {
            let msg = Message::User(UserMessage::new(
                &session.id,
                "build",
                "anthropic",
                "claude",
            ));
            manager.save_message(&session.id, &msg).unwrap();
        }

        // Keep 4, should compact 2
        let compactable = manager.get_compactable_messages(&session.id, 4).unwrap();
        assert_eq!(compactable.len(), 2);
    }
}
