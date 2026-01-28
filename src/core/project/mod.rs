//! Project detection and management.
//!
//! Projects are identified by their git root commit hash.
//! Non-git directories use the "global" project ID.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use super::storage::Storage;

/// Global project ID for non-git directories.
pub const GLOBAL_PROJECT_ID: &str = "global";

/// Project metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique project identifier (git root commit or "global").
    pub id: String,

    /// Git worktree root path.
    pub worktree: PathBuf,

    /// Version control system type.
    pub vcs: Option<String>,

    /// Timestamps.
    pub time: ProjectTime,
}

/// Project timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTime {
    /// When the project was first seen.
    pub created: i64,

    /// When the project was last initialized.
    pub initialized: i64,
}

impl Project {
    /// Detect the current project from the working directory.
    ///
    /// # Errors
    ///
    /// Returns error if project detection fails.
    pub fn detect(cwd: &Path) -> anyhow::Result<Self> {
        // Try to find git root
        let git_root = find_git_root(cwd);

        match git_root {
            Some(worktree) => {
                // Get the root commit hash as project ID
                let id = get_git_root_commit(&worktree)?;
                Ok(Self {
                    id,
                    worktree,
                    vcs: Some("git".to_string()),
                    time: ProjectTime {
                        created: chrono::Utc::now().timestamp_millis(),
                        initialized: chrono::Utc::now().timestamp_millis(),
                    },
                })
            }
            None => {
                // No git repo, use global project
                Ok(Self {
                    id: GLOBAL_PROJECT_ID.to_string(),
                    worktree: cwd.to_path_buf(),
                    vcs: None,
                    time: ProjectTime {
                        created: chrono::Utc::now().timestamp_millis(),
                        initialized: chrono::Utc::now().timestamp_millis(),
                    },
                })
            }
        }
    }

    /// Load or create project in storage.
    ///
    /// # Errors
    ///
    /// Returns error if storage operations fail.
    pub fn load_or_create(storage: &Storage, cwd: &Path) -> anyhow::Result<Self> {
        let detected = Self::detect(cwd)?;

        // Try to load existing project
        if let Ok(existing) = storage.read::<Self>(&["project", &detected.id]) {
            return Ok(existing);
        }

        // Save new project
        storage.write(&["project", &detected.id], &detected)?;
        Ok(detected)
    }
}

/// Find the git repository root from a directory.
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(PathBuf::from(path))
    } else {
        None
    }
}

/// Get the root commit hash of a git repository.
///
/// This returns the first commit, which uniquely identifies the repository.
fn get_git_root_commit(repo_path: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(["rev-list", "--max-parents=0", "--all"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("failed to get git root commit");
    }

    let commits = String::from_utf8_lossy(&output.stdout);
    let mut hashes: Vec<&str> = commits
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    // Sort to get consistent ID even with multiple root commits
    hashes.sort_unstable();

    hashes
        .first()
        .map(|s| (*s).to_string())
        .ok_or_else(|| anyhow::anyhow!("no root commit found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_current_directory() {
        // This test runs in the omni/cli git repo
        let cwd = std::env::current_dir().unwrap();
        let project = Project::detect(&cwd).unwrap();

        // Should be a git project
        assert!(project.vcs.is_some());
        assert_ne!(project.id, GLOBAL_PROJECT_ID);
        assert!(!project.id.is_empty());
    }

    #[test]
    fn detect_non_git_directory() {
        // Use temp directory which won't be a git repo
        let temp = tempfile::tempdir().unwrap();
        let project = Project::detect(temp.path()).unwrap();

        assert_eq!(project.id, GLOBAL_PROJECT_ID);
        assert!(project.vcs.is_none());
    }
}
