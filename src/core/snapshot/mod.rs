//! Snapshot management for file change tracking
//!
//! Uses a shadow git repository to track file changes and enable
//! rollback/undo functionality

use std::path::PathBuf;
use std::process::Command;

use crate::core::project::Project;

/// Snapshot manager for tracking file changes
pub struct SnapshotManager {
    /// Path to the shadow git directory
    git_dir: PathBuf,
    /// Working directory to track
    worktree: PathBuf,
}

/// Result of creating a snapshot
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Tree hash identifying this snapshot
    pub hash: String,
}

/// Files changed since a snapshot
#[derive(Debug, Clone)]
pub struct Patch {
    /// Snapshot hash
    pub hash: String,
    /// Changed file paths
    pub files: Vec<PathBuf>,
}

/// File diff information
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// File path
    pub file: PathBuf,
    /// Content before
    pub before: String,
    /// Content after
    pub after: String,
    /// Lines added
    pub additions: u32,
    /// Lines deleted
    pub deletions: u32,
}

impl SnapshotManager {
    /// Create a snapshot manager for a project
    ///
    /// # Errors
    ///
    /// Returns error if data directory cannot be determined
    pub fn for_project(project: &Project) -> anyhow::Result<Self> {
        let git_dir = crate::config::Config::data_dir()?
            .join("snapshot")
            .join(&project.id);

        Ok(Self {
            git_dir,
            worktree: project.worktree.clone(),
        })
    }

    /// Initialize the snapshot repository
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn init(&self) -> anyhow::Result<()> {
        if !self.git_dir.exists() {
            std::fs::create_dir_all(&self.git_dir)?;

            let output = Command::new("git")
                .args(["init", "--bare"])
                .current_dir(&self.git_dir)
                .output()?;

            if !output.status.success() {
                anyhow::bail!(
                    "Failed to init snapshot repo: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            tracing::info!(git_dir = %self.git_dir.display(), "initialized snapshot repo");
        }

        Ok(())
    }

    /// Create a snapshot of the current state
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn track(&self) -> anyhow::Result<Snapshot> {
        self.init()?;

        // Add all files
        let output = Command::new("git")
            .args([
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "add",
                ".",
            ])
            .current_dir(&self.worktree)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to add files: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Write tree to get hash
        let output = Command::new("git")
            .args([
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "write-tree",
            ])
            .current_dir(&self.worktree)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to write tree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        tracing::info!(hash = %hash, "created snapshot");

        Ok(Snapshot { hash })
    }

    /// Get changed files since a snapshot
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn patch(&self, hash: &str) -> anyhow::Result<Patch> {
        // Add current state
        let _ = Command::new("git")
            .args([
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "add",
                ".",
            ])
            .current_dir(&self.worktree)
            .output()?;

        // Get changed files
        let output = Command::new("git")
            .args([
                "-c",
                "core.quotepath=false",
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "diff",
                "--no-ext-diff",
                "--name-only",
                hash,
                "--",
                ".",
            ])
            .current_dir(&self.worktree)
            .output()?;

        if !output.status.success() {
            tracing::warn!(hash = %hash, "failed to get diff");
            return Ok(Patch {
                hash: hash.to_string(),
                files: Vec::new(),
            });
        }

        let files: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| self.worktree.join(s))
            .collect();

        Ok(Patch {
            hash: hash.to_string(),
            files,
        })
    }

    /// Restore files to a snapshot state
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn restore(&self, hash: &str) -> anyhow::Result<()> {
        tracing::info!(hash = %hash, "restoring snapshot");

        // Read tree and checkout
        let output = Command::new("git")
            .args([
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "read-tree",
                hash,
            ])
            .current_dir(&self.worktree)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to read tree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let output = Command::new("git")
            .args([
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "checkout-index",
                "-a",
                "-f",
            ])
            .current_dir(&self.worktree)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to checkout: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Revert specific files from patches
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn revert(&self, patches: &[Patch]) -> anyhow::Result<()> {
        let mut reverted = std::collections::HashSet::new();

        for patch in patches {
            for file in &patch.files {
                if reverted.contains(file) {
                    continue;
                }

                let relative = file.strip_prefix(&self.worktree).unwrap_or(file);

                tracing::info!(file = %relative.display(), hash = %patch.hash, "reverting file");

                let output = Command::new("git")
                    .args([
                        "--git-dir",
                        &self.git_dir.to_string_lossy(),
                        "--work-tree",
                        &self.worktree.to_string_lossy(),
                        "checkout",
                        &patch.hash,
                        "--",
                        &relative.to_string_lossy(),
                    ])
                    .current_dir(&self.worktree)
                    .output()?;

                if !output.status.success() {
                    // Check if file existed in snapshot
                    let check = Command::new("git")
                        .args([
                            "--git-dir",
                            &self.git_dir.to_string_lossy(),
                            "--work-tree",
                            &self.worktree.to_string_lossy(),
                            "ls-tree",
                            &patch.hash,
                            "--",
                            &relative.to_string_lossy(),
                        ])
                        .current_dir(&self.worktree)
                        .output()?;

                    if check.status.success() && !check.stdout.is_empty() {
                        tracing::info!(file = %relative.display(), "file existed in snapshot but checkout failed");
                    } else {
                        // File didn't exist in snapshot - delete it
                        tracing::info!(file = %relative.display(), "file did not exist in snapshot, deleting");
                        let _ = std::fs::remove_file(file);
                    }
                }

                reverted.insert(file.clone());
            }
        }

        Ok(())
    }

    /// Get diff text since a snapshot
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn diff(&self, hash: &str) -> anyhow::Result<String> {
        // Add current state
        let _ = Command::new("git")
            .args([
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "add",
                ".",
            ])
            .current_dir(&self.worktree)
            .output()?;

        let output = Command::new("git")
            .args([
                "-c",
                "core.quotepath=false",
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "diff",
                "--no-ext-diff",
                hash,
                "--",
                ".",
            ])
            .current_dir(&self.worktree)
            .output()?;

        if !output.status.success() {
            tracing::warn!(hash = %hash, "failed to get diff");
            return Ok(String::new());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Cleanup old snapshots
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn cleanup(&self) -> anyhow::Result<()> {
        if !self.git_dir.exists() {
            return Ok(());
        }

        let output = Command::new("git")
            .args([
                "--git-dir",
                &self.git_dir.to_string_lossy(),
                "--work-tree",
                &self.worktree.to_string_lossy(),
                "gc",
                "--prune=7.days",
            ])
            .current_dir(&self.worktree)
            .output()?;

        if output.status.success() {
            tracing::info!("snapshot cleanup completed");
        } else {
            tracing::warn!(
                "snapshot cleanup failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_manager_paths() {
        let project = Project {
            id: "test-project".to_string(),
            worktree: PathBuf::from("/tmp/test"),
            vcs: Some("git".to_string()),
            time: crate::core::project::ProjectTime {
                created: 0,
                initialized: 0,
            },
        };

        let manager = SnapshotManager::for_project(&project).unwrap();
        assert!(manager.git_dir.to_string_lossy().contains("snapshot"));
        assert!(manager.git_dir.to_string_lossy().contains("test-project"));
    }
}
