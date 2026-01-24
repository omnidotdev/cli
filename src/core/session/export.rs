//! Session export for sharing and backup
//!
//! Export sessions to JSON or Markdown format

use std::path::Path;

use serde::Serialize;

use super::{Message, Part, Session, SessionManager};

/// Export format
#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    /// JSON format (full fidelity)
    Json,
    /// Markdown format (human readable)
    Markdown,
}

/// Exported session data
#[derive(Debug, Serialize)]
pub struct ExportedSession {
    /// Session metadata
    pub session: Session,
    /// Messages with their parts
    pub messages: Vec<ExportedMessage>,
}

/// Exported message with parts
#[derive(Debug, Serialize)]
pub struct ExportedMessage {
    /// Message data
    #[serde(flatten)]
    pub message: Message,
    /// Parts belonging to this message
    pub parts: Vec<Part>,
}

impl SessionManager {
    /// Export a session to a structured format
    ///
    /// # Errors
    ///
    /// Returns error if session or messages cannot be loaded
    pub fn export_session(&self, session_id: &str) -> anyhow::Result<ExportedSession> {
        let session = self.get_session(session_id)?;
        let messages = self.list_messages(session_id)?;

        let mut exported_messages = Vec::new();
        for message in messages {
            let parts = self.list_parts(message.id())?;
            exported_messages.push(ExportedMessage { message, parts });
        }

        Ok(ExportedSession {
            session,
            messages: exported_messages,
        })
    }

    /// Export session to JSON string
    ///
    /// # Errors
    ///
    /// Returns error if export or serialization fails
    pub fn export_to_json(&self, session_id: &str) -> anyhow::Result<String> {
        let exported = self.export_session(session_id)?;
        Ok(serde_json::to_string_pretty(&exported)?)
    }

    /// Export session to Markdown string
    ///
    /// # Errors
    ///
    /// Returns error if export fails
    pub fn export_to_markdown(&self, session_id: &str) -> anyhow::Result<String> {
        let exported = self.export_session(session_id)?;
        Ok(format_as_markdown(&exported))
    }

    /// Export session to file
    ///
    /// # Errors
    ///
    /// Returns error if export or file write fails
    pub fn export_to_file(
        &self,
        session_id: &str,
        path: &Path,
        format: ExportFormat,
    ) -> anyhow::Result<()> {
        let content = match format {
            ExportFormat::Json => self.export_to_json(session_id)?,
            ExportFormat::Markdown => self.export_to_markdown(session_id)?,
        };

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Format exported session as Markdown
fn format_as_markdown(exported: &ExportedSession) -> String {
    use std::fmt::Write;
    let mut md = String::new();

    // Header
    let _ = writeln!(md, "# {}\n", exported.session.title);
    let _ = writeln!(md, "**Session ID:** {}", exported.session.id);
    let _ = writeln!(
        md,
        "**Created:** {}",
        format_timestamp(exported.session.time.created)
    );
    let _ = writeln!(
        md,
        "**Directory:** {}\n",
        exported.session.directory.display()
    );
    md.push_str("---\n\n");

    // Messages
    for exported_msg in &exported.messages {
        let role = match &exported_msg.message {
            Message::User(_) => "User",
            Message::Assistant(_) => "Assistant",
        };

        let _ = writeln!(md, "## {role}\n");

        // Parts
        for part in &exported_msg.parts {
            match part {
                Part::Text(t) => {
                    md.push_str(&t.text);
                    md.push_str("\n\n");
                }
                Part::Tool(t) => {
                    let _ = writeln!(md, "**Tool:** `{}`\n", t.tool);
                    match &t.state {
                        super::ToolState::Completed { output, .. } => {
                            if !output.is_empty() {
                                md.push_str("```\n");
                                // Truncate long output
                                if output.len() > 1000 {
                                    md.push_str(&output[..1000]);
                                    md.push_str("\n... (truncated)");
                                } else {
                                    md.push_str(output);
                                }
                                md.push_str("\n```\n\n");
                            }
                        }
                        super::ToolState::Error { error, .. } => {
                            let _ = writeln!(md, "**Error:** {error}\n");
                        }
                        _ => {}
                    }
                }
                Part::Reasoning(r) => {
                    md.push_str("<details>\n<summary>Reasoning</summary>\n\n");
                    md.push_str(&r.text);
                    md.push_str("\n\n</details>\n\n");
                }
            }
        }

        md.push_str("---\n\n");
    }

    md
}

/// Format timestamp as ISO 8601
fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts).map_or_else(
        || "Unknown".to_string(),
        |dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::{Project, ProjectTime};
    use crate::core::session::{TextPart, UserMessage};
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
    fn export_session_includes_parts() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        // Add a message with a part
        let msg = Message::User(UserMessage::new(
            &session.id,
            "build",
            "anthropic",
            "claude",
        ));
        manager.save_message(&session.id, &msg).unwrap();

        let part = Part::Text(TextPart::new(&msg.id(), &session.id, "Hello world"));
        manager.save_part(&msg.id(), &part).unwrap();

        // Export
        let exported = manager.export_session(&session.id).unwrap();
        assert_eq!(exported.messages.len(), 1);
        assert_eq!(exported.messages[0].parts.len(), 1);
    }

    #[test]
    fn export_to_json_is_valid() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        let json = manager.export_to_json(&session.id).unwrap();
        // Should be valid JSON
        let _: serde_json::Value = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn export_to_markdown_contains_title() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        let md = manager.export_to_markdown(&session.id).unwrap();
        assert!(md.contains(&session.title));
    }
}
