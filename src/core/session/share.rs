//! Session sharing for URL-based access
//!
//! Generate short tokens to share sessions via URLs

use serde::{Deserialize, Serialize};

use super::{ExportedSession, SessionManager};

/// Share token info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareToken {
    /// Short token for URLs (8 chars)
    pub token: String,
    /// Session ID this token refers to
    pub session_id: String,
    /// Project ID for context
    pub project_id: String,
    /// Secret for modification (UUID)
    pub secret: String,
    /// Creation timestamp (millis)
    pub created_at: i64,
    /// Expiration timestamp (millis), None = never
    pub expires_at: Option<i64>,
    /// Number of times accessed
    pub access_count: u32,
}

/// Share creation options
#[derive(Debug, Clone, Default)]
pub struct ShareOptions {
    /// Time-to-live in seconds (None = never expires)
    pub ttl_seconds: Option<u64>,
}

impl ShareToken {
    /// Check if the token has expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = chrono::Utc::now().timestamp_millis();
            now > expires_at
        } else {
            false
        }
    }
}

/// Generate a short share token (8 chars, URL-safe)
fn generate_token() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..8)
        .map(|_| {
            let idx = rng.random_range(0..CHARS.len());
            CHARS[idx] as char
        })
        .collect()
}

/// Generate a secret (UUID v4)
fn generate_secret() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl SessionManager {
    /// Create a share token for a session
    ///
    /// # Errors
    ///
    /// Returns error if session doesn't exist or storage fails
    pub fn create_share(
        &self,
        session_id: &str,
        options: ShareOptions,
    ) -> anyhow::Result<ShareToken> {
        // Verify session exists
        let session = self.get_session(session_id)?;

        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = options
            .ttl_seconds
            .map(|ttl| now + (ttl as i64 * 1000));

        let token = ShareToken {
            token: generate_token(),
            session_id: session_id.to_string(),
            project_id: session.project_id,
            secret: generate_secret(),
            created_at: now,
            expires_at,
            access_count: 0,
        };

        // Store the token
        self.storage
            .write(&["share", &token.token], &token)?;

        // Also store reverse mapping (session -> token) for lookup
        self.storage
            .write(&["session_share", session_id], &token.token)?;

        Ok(token)
    }

    /// Get a share token by token string
    ///
    /// # Errors
    ///
    /// Returns error if token doesn't exist
    pub fn get_share(&self, token: &str) -> anyhow::Result<ShareToken> {
        Ok(self.storage.read(&["share", token])?)
    }

    /// Get a share token for a session (if one exists)
    pub fn get_share_for_session(&self, session_id: &str) -> anyhow::Result<Option<ShareToken>> {
        let token: Result<String, _> = self.storage.read(&["session_share", session_id]);
        match token {
            Ok(t) => {
                let share = self.get_share(&t)?;
                if share.is_expired() {
                    // Clean up expired token
                    let _ = self.revoke_share(&t, &share.secret);
                    Ok(None)
                } else {
                    Ok(Some(share))
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Get shared session data (for public access)
    ///
    /// # Errors
    ///
    /// Returns error if token is invalid or expired
    pub fn get_shared_session(&self, token: &str) -> anyhow::Result<ExportedSession> {
        let mut share = self.get_share(token)?;

        if share.is_expired() {
            anyhow::bail!("share token has expired");
        }

        // Increment access count
        share.access_count += 1;
        self.storage.write(&["share", token], &share)?;

        // Export the session
        self.export_session(&share.session_id)
    }

    /// Revoke a share token
    ///
    /// # Errors
    ///
    /// Returns error if token doesn't exist or secret is wrong
    pub fn revoke_share(&self, token: &str, secret: &str) -> anyhow::Result<()> {
        let share = self.get_share(token)?;

        if share.secret != secret {
            anyhow::bail!("invalid secret");
        }

        // Remove token
        self.storage.remove(&["share", token])?;

        // Remove reverse mapping
        self.storage
            .remove(&["session_share", &share.session_id])?;

        Ok(())
    }

    /// List all active shares
    pub fn list_shares(&self) -> anyhow::Result<Vec<ShareToken>> {
        let tokens: Vec<ShareToken> = self
            .storage
            .list_prefix::<ShareToken>("share")?
            .into_iter()
            .filter(|t| !t.is_expired())
            .collect();
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::{Project, ProjectTime};
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
    fn create_share_returns_token() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        let share = manager
            .create_share(&session.id, ShareOptions::default())
            .unwrap();

        assert_eq!(share.token.len(), 8);
        assert_eq!(share.session_id, session.id);
        assert!(!share.secret.is_empty());
    }

    #[test]
    fn get_share_retrieves_token() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        let share = manager
            .create_share(&session.id, ShareOptions::default())
            .unwrap();

        let retrieved = manager.get_share(&share.token).unwrap();
        assert_eq!(retrieved.session_id, session.id);
    }

    #[test]
    fn get_shared_session_increments_count() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        let share = manager
            .create_share(&session.id, ShareOptions::default())
            .unwrap();

        assert_eq!(share.access_count, 0);

        let _ = manager.get_shared_session(&share.token).unwrap();
        let updated = manager.get_share(&share.token).unwrap();
        assert_eq!(updated.access_count, 1);
    }

    #[test]
    fn revoke_share_removes_token() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        let share = manager
            .create_share(&session.id, ShareOptions::default())
            .unwrap();

        manager.revoke_share(&share.token, &share.secret).unwrap();

        assert!(manager.get_share(&share.token).is_err());
    }

    #[test]
    fn revoke_with_wrong_secret_fails() {
        let (manager, _dir) = temp_manager();
        let session = manager.create_session().unwrap();

        let share = manager
            .create_share(&session.id, ShareOptions::default())
            .unwrap();

        let result = manager.revoke_share(&share.token, "wrong-secret");
        assert!(result.is_err());
    }
}
