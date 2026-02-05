//! Session management for conversation persistence.
//!
//! Sessions contain messages, which contain parts. This hierarchical structure
//! enables granular storage, streaming updates, and efficient compaction.

mod compaction;
mod export;
mod message;
mod part;
mod share;
mod titling;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ulid::Ulid;

pub use compaction::{
    CompactionResult, DEFAULT_COMPACTION_THRESHOLD, MIN_MESSAGES_TO_KEEP, compaction_prompt,
};
pub use export::{ExportFormat, ExportedMessage, ExportedSession};
pub use message::{
    AssistantMessage, Message, MessageSummary, MessageTime, TokenUsage, UserMessage,
};
pub use part::{FileReference, Part, PartTime, ReasoningPart, TextPart, ToolPart, ToolState};
pub use share::{ShareOptions, ShareToken};
pub use titling::{MAX_TITLE_LENGTH, extract_title, titling_prompt};

use super::project::Project;

/// Target session for resumption
#[derive(Debug, Clone, Default)]
pub enum SessionTarget {
    /// Create a new session
    #[default]
    New,
    /// Continue the most recent session
    MostRecent,
    /// Resume a specific session by ID
    Specific(String),
}

impl SessionTarget {
    /// Create a session target from CLI flags
    #[must_use]
    pub fn from_flags(continue_flag: bool, session_id: Option<String>) -> Self {
        match (continue_flag, session_id) {
            (true, _) => Self::MostRecent,
            (_, Some(id)) => Self::Specific(id),
            _ => Self::New,
        }
    }
}
use super::storage::Storage;

/// Generate a new session ID.
#[must_use]
pub fn new_session_id() -> String {
    format!("ses_{}", Ulid::new())
}

/// Generate a new message ID.
#[must_use]
pub fn new_message_id() -> String {
    format!("msg_{}", Ulid::new())
}

/// Generate a new part ID.
#[must_use]
pub fn new_part_id() -> String {
    format!("prt_{}", Ulid::new())
}

/// Generate a human-readable slug.
#[must_use]
pub fn new_slug() -> String {
    // Simple slug: adjective-noun format
    let adjectives = [
        "quick", "bright", "calm", "bold", "keen", "swift", "wise", "warm",
    ];
    let nouns = ["fox", "owl", "bear", "wolf", "hawk", "deer", "lynx", "crow"];

    use rand::prelude::IndexedRandom;
    let mut rng = rand::rng();

    let adj = adjectives.choose(&mut rng).unwrap_or(&"quick");
    let noun = nouns.choose(&mut rng).unwrap_or(&"fox");
    let num: u16 = rand::Rng::random_range(&mut rng, 100..1000);

    format!("{adj}-{noun}-{num}")
}

/// Session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,

    /// Human-readable slug.
    pub slug: String,

    /// Project this session belongs to.
    pub project_id: String,

    /// Session title (auto-generated or user-set).
    pub title: String,

    /// Working directory for this session.
    pub directory: PathBuf,

    /// Timestamps.
    pub time: SessionTime,

    /// File change summary.
    pub summary: Option<SessionSummary>,
}

/// Session timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTime {
    /// When the session was created.
    pub created: i64,

    /// When the session was last updated.
    pub updated: i64,

    /// When the session was last compacted.
    pub compacted: Option<i64>,
}

/// Session file change summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Lines added.
    pub additions: u32,

    /// Lines removed.
    pub deletions: u32,

    /// Files changed.
    pub files: u32,
}

impl Session {
    /// Create a new session.
    #[must_use]
    pub fn new(project: &Project) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let slug = new_slug();

        Self {
            id: new_session_id(),
            slug: slug.clone(),
            project_id: project.id.clone(),
            title: format!("New session - {slug}"),
            directory: project.worktree.clone(),
            time: SessionTime {
                created: now,
                updated: now,
                compacted: None,
            },
            summary: None,
        }
    }

    /// Check if the title is still the default.
    #[must_use]
    pub fn has_default_title(&self) -> bool {
        self.title.starts_with("New session - ")
    }
}

/// Session manager for CRUD operations.
pub struct SessionManager {
    storage: Storage,
    project: Project,
}

impl SessionManager {
    /// Create a new session manager.
    #[must_use]
    pub const fn new(storage: Storage, project: Project) -> Self {
        Self { storage, project }
    }

    /// Create a session manager for the current project
    ///
    /// # Errors
    ///
    /// Returns error if project detection or storage initialization fails
    pub fn for_current_project() -> anyhow::Result<Self> {
        let storage = Storage::new()?;
        let project = Project::detect(&std::env::current_dir()?)?;
        Ok(Self::new(storage, project))
    }

    /// Get the storage reference.
    #[must_use]
    pub const fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Get the project reference.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    /// Create a new session.
    ///
    /// # Errors
    ///
    /// Returns error if storage write fails.
    pub fn create_session(&self) -> anyhow::Result<Session> {
        let session = Session::new(&self.project);
        self.storage
            .write(&["session", &self.project.id, &session.id], &session)?;
        Ok(session)
    }

    /// Get a session by ID.
    ///
    /// # Errors
    ///
    /// Returns error if session not found.
    pub fn get_session(&self, session_id: &str) -> anyhow::Result<Session> {
        Ok(self
            .storage
            .read(&["session", &self.project.id, session_id])?)
    }

    /// Find a session by ID or slug.
    ///
    /// Tries exact ID match first, then searches by slug.
    ///
    /// # Errors
    ///
    /// Returns error if session not found.
    pub fn find_session(&self, id_or_slug: &str) -> anyhow::Result<Session> {
        // Try exact ID match first
        if let Ok(session) = self.get_session(id_or_slug) {
            return Ok(session);
        }

        // Search by slug
        let sessions = self.list_sessions()?;
        for session in sessions {
            if session.slug == id_or_slug {
                return Ok(session);
            }
        }

        anyhow::bail!("session not found: {id_or_slug}")
    }

    /// Update a session.
    ///
    /// # Errors
    ///
    /// Returns error if storage write fails.
    pub fn update_session(&self, session: &Session) -> anyhow::Result<()> {
        self.storage
            .write(&["session", &self.project.id, &session.id], session)?;
        Ok(())
    }

    /// Touch a session (update timestamp).
    ///
    /// # Errors
    ///
    /// Returns error if storage update fails.
    pub fn touch_session(&self, session_id: &str) -> anyhow::Result<Session> {
        Ok(self.storage.update(
            &["session", &self.project.id, session_id],
            |s: &mut Session| {
                s.time.updated = chrono::Utc::now().timestamp_millis();
            },
        )?)
    }

    /// Delete a session and all its messages.
    ///
    /// # Errors
    ///
    /// Returns error if deletion fails.
    pub fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        // Delete all messages and their parts
        let messages = self.list_messages(session_id)?;
        for msg in messages {
            self.delete_message(session_id, msg.id())?;
        }

        // Delete session
        self.storage
            .remove(&["session", &self.project.id, session_id])?;
        Ok(())
    }

    /// List all sessions for the current project.
    ///
    /// Returns sessions sorted by update time (newest first).
    ///
    /// # Errors
    ///
    /// Returns error if storage read fails.
    pub fn list_sessions(&self) -> anyhow::Result<Vec<Session>> {
        let keys = self.storage.list(&["session", &self.project.id])?;
        let mut sessions = Vec::new();

        for key in keys {
            if let Some(session_id) = key.last() {
                if let Ok(session) =
                    self.storage
                        .read::<Session>(&["session", &self.project.id, session_id])
                {
                    sessions.push(session);
                }
            }
        }

        // Sort by updated time, newest first
        sessions.sort_by(|a, b| b.time.updated.cmp(&a.time.updated));
        Ok(sessions)
    }

    /// Get the most recent session, or create one if none exist.
    ///
    /// # Errors
    ///
    /// Returns error if storage operations fail.
    pub fn get_or_create_current(&self) -> anyhow::Result<Session> {
        let sessions = self.list_sessions()?;
        if let Some(session) = sessions.into_iter().next() {
            Ok(session)
        } else {
            self.create_session()
        }
    }

    // Message operations

    /// Save a message.
    ///
    /// # Errors
    ///
    /// Returns error if storage write fails.
    pub fn save_message(&self, session_id: &str, message: &Message) -> anyhow::Result<()> {
        self.storage
            .write(&["message", session_id, message.id()], message)?;
        Ok(())
    }

    /// Get a message by ID.
    ///
    /// # Errors
    ///
    /// Returns error if message not found.
    pub fn get_message(&self, session_id: &str, message_id: &str) -> anyhow::Result<Message> {
        Ok(self.storage.read(&["message", session_id, message_id])?)
    }

    /// Delete a message and all its parts.
    ///
    /// # Errors
    ///
    /// Returns error if deletion fails.
    pub fn delete_message(&self, session_id: &str, message_id: &str) -> anyhow::Result<()> {
        // Delete all parts
        let parts = self.list_parts(message_id)?;
        for part in parts {
            self.delete_part(message_id, part.id())?;
        }

        // Delete message
        self.storage.remove(&["message", session_id, message_id])?;
        Ok(())
    }

    /// List all messages in a session.
    ///
    /// Returns messages sorted by ID (chronological order).
    ///
    /// # Errors
    ///
    /// Returns error if storage read fails.
    pub fn list_messages(&self, session_id: &str) -> anyhow::Result<Vec<Message>> {
        let keys = self.storage.list(&["message", session_id])?;
        let mut messages = Vec::new();

        for key in keys {
            if let Some(message_id) = key.last() {
                if let Ok(message) = self
                    .storage
                    .read::<Message>(&["message", session_id, message_id])
                {
                    messages.push(message);
                }
            }
        }

        // Sort by ID (ULID ensures chronological order)
        messages.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(messages)
    }

    // Part operations

    /// Save a part.
    ///
    /// # Errors
    ///
    /// Returns error if storage write fails.
    pub fn save_part(&self, message_id: &str, part: &Part) -> anyhow::Result<()> {
        self.storage.write(&["part", message_id, part.id()], part)?;
        Ok(())
    }

    /// Get a part by ID.
    ///
    /// # Errors
    ///
    /// Returns error if part not found.
    pub fn get_part(&self, message_id: &str, part_id: &str) -> anyhow::Result<Part> {
        Ok(self.storage.read(&["part", message_id, part_id])?)
    }

    /// Update a part.
    ///
    /// # Errors
    ///
    /// Returns error if storage update fails.
    pub fn update_part<F>(&self, message_id: &str, part_id: &str, f: F) -> anyhow::Result<Part>
    where
        F: FnOnce(&mut Part),
    {
        Ok(self.storage.update(&["part", message_id, part_id], f)?)
    }

    /// Delete a part.
    ///
    /// # Errors
    ///
    /// Returns error if deletion fails.
    pub fn delete_part(&self, message_id: &str, part_id: &str) -> anyhow::Result<()> {
        self.storage.remove(&["part", message_id, part_id])?;
        Ok(())
    }

    /// List all parts for a message.
    ///
    /// Returns parts sorted by ID (chronological order).
    ///
    /// # Errors
    ///
    /// Returns error if storage read fails.
    pub fn list_parts(&self, message_id: &str) -> anyhow::Result<Vec<Part>> {
        let keys = self.storage.list(&["part", message_id])?;
        let mut parts = Vec::new();

        for key in keys {
            if let Some(part_id) = key.last() {
                if let Ok(part) = self.storage.read::<Part>(&["part", message_id, part_id]) {
                    parts.push(part);
                }
            }
        }

        // Sort by ID (ULID ensures chronological order)
        parts.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_manager() -> (SessionManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::with_root(dir.path().to_path_buf());
        let project = Project {
            id: "test-project".to_string(),
            worktree: dir.path().to_path_buf(),
            vcs: Some("git".to_string()),
            time: super::super::project::ProjectTime {
                created: 0,
                initialized: 0,
            },
        };
        (SessionManager::new(storage, project), dir)
    }

    #[test]
    fn create_and_list_sessions() {
        let (manager, _dir) = temp_manager();

        let first = manager.create_session().unwrap();
        let second = manager.create_session().unwrap();

        let all_sessions = manager.list_sessions().unwrap();
        assert_eq!(all_sessions.len(), 2);

        let ids: Vec<_> = all_sessions.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&first.id.as_str()));
        assert!(ids.contains(&second.id.as_str()));
    }

    #[test]
    fn delete_session_cascades() {
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

        let part = Part::Text(TextPart::new(msg.id(), &session.id, "Hello"));
        manager.save_part(msg.id(), &part).unwrap();

        // Delete session
        manager.delete_session(&session.id).unwrap();

        // Everything should be gone
        assert!(manager.get_session(&session.id).is_err());
        assert!(manager.get_message(&session.id, msg.id()).is_err());
        assert!(manager.get_part(msg.id(), part.id()).is_err());
    }

    #[test]
    fn get_or_create_returns_existing() {
        let (manager, _dir) = temp_manager();

        let session1 = manager.create_session().unwrap();
        let session2 = manager.get_or_create_current().unwrap();

        assert_eq!(session1.id, session2.id);
    }
}
