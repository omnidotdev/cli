//! Git worktree management for isolated workspaces
//!
//! Provides ability to create, remove, and reset git worktrees for
//! parallel development with feature branch isolation

use std::path::{Path, PathBuf};
use std::process::Command;

use rand::prelude::IndexedRandom;

use crate::core::project::Project;

/// Worktree information
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Worktree name
    pub name: String,
    /// Associated branch name
    pub branch: String,
    /// Path to the worktree directory
    pub directory: PathBuf,
}

/// Worktree manager for a project
pub struct WorktreeManager {
    /// Root directory for worktrees
    root: PathBuf,
    /// Main repository worktree
    main_worktree: PathBuf,
}

/// Word lists for random name generation
const ADJECTIVES: &[&str] = &[
    "brave", "calm", "clever", "cosmic", "crisp", "curious", "eager", "gentle", "glowing", "happy",
    "hidden", "jolly", "kind", "lucky", "mighty", "misty", "neon", "nimble", "playful", "proud",
    "quick", "quiet", "shiny", "silent", "stellar", "sunny", "swift", "tidy", "witty",
];

const NOUNS: &[&str] = &[
    "cabin", "cactus", "canyon", "circuit", "comet", "eagle", "engine", "falcon", "forest",
    "garden", "harbor", "island", "knight", "lagoon", "meadow", "moon", "mountain", "nebula",
    "orchid", "otter", "panda", "pixel", "planet", "river", "rocket", "sailor", "squid", "star",
    "tiger", "wizard", "wolf",
];

impl WorktreeManager {
    /// Create a worktree manager for a project
    ///
    /// # Errors
    ///
    /// Returns error if data directory cannot be determined
    pub fn for_project(project: &Project) -> anyhow::Result<Self> {
        let root = crate::config::Config::data_dir()?
            .join("worktree")
            .join(&project.id);

        Ok(Self {
            root,
            main_worktree: project.worktree.clone(),
        })
    }

    /// List all worktrees for this project
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail
    pub fn list(&self) -> anyhow::Result<Vec<WorktreeInfo>> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.main_worktree)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to list worktrees: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch: Option<String> = None;

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take()) {
                    // Skip the main worktree
                    if path != self.main_worktree {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        worktrees.push(WorktreeInfo {
                            name,
                            branch,
                            directory: path,
                        });
                    }
                }
                continue;
            }

            if let Some(path) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(path.trim()));
            } else if let Some(branch) = line.strip_prefix("branch ") {
                current_branch = Some(
                    branch
                        .trim()
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch.trim())
                        .to_string(),
                );
            }
        }

        // Handle last entry
        if let (Some(path), Some(branch)) = (current_path, current_branch) {
            if path != self.main_worktree {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                worktrees.push(WorktreeInfo {
                    name,
                    branch,
                    directory: path,
                });
            }
        }

        Ok(worktrees)
    }

    /// Create a new worktree
    ///
    /// # Errors
    ///
    /// Returns error if worktree creation fails
    pub fn create(&self, name: Option<&str>) -> anyhow::Result<WorktreeInfo> {
        std::fs::create_dir_all(&self.root)?;

        let info = self.find_available_name(name)?;

        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "--no-checkout",
                "-b",
                &info.branch,
                &info.directory.to_string_lossy(),
            ])
            .current_dir(&self.main_worktree)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Populate the worktree
        let output = Command::new("git")
            .args(["reset", "--hard"])
            .current_dir(&info.directory)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to populate worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        tracing::info!(
            name = %info.name,
            branch = %info.branch,
            directory = %info.directory.display(),
            "created worktree"
        );

        Ok(info)
    }

    /// Remove a worktree
    ///
    /// # Errors
    ///
    /// Returns error if removal fails
    pub fn remove(&self, directory: &Path) -> anyhow::Result<()> {
        // Get the branch name before removing
        let worktrees = self.list()?;
        let worktree = worktrees
            .iter()
            .find(|w| w.directory == directory)
            .ok_or_else(|| anyhow::anyhow!("Worktree not found"))?;

        let branch = worktree.branch.clone();

        // Remove the worktree
        let output = Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                &directory.to_string_lossy(),
            ])
            .current_dir(&self.main_worktree)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to remove worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Delete the branch
        let output = Command::new("git")
            .args(["branch", "-D", &branch])
            .current_dir(&self.main_worktree)
            .output()?;

        if !output.status.success() {
            tracing::warn!(
                branch = %branch,
                error = %String::from_utf8_lossy(&output.stderr),
                "failed to delete worktree branch"
            );
        }

        tracing::info!(directory = %directory.display(), "removed worktree");

        Ok(())
    }

    /// Reset a worktree to the default branch
    ///
    /// # Errors
    ///
    /// Returns error if reset fails
    pub fn reset(&self, directory: &Path) -> anyhow::Result<()> {
        if directory == self.main_worktree {
            anyhow::bail!("Cannot reset the primary workspace");
        }

        // Verify it's a valid worktree
        let worktrees = self.list()?;
        if !worktrees.iter().any(|w| w.directory == directory) {
            anyhow::bail!("Worktree not found");
        }

        // Find the target branch
        let target = self.find_default_branch()?;

        // Fetch if remote
        if target.contains('/') {
            let parts: Vec<&str> = target.splitn(2, '/').collect();
            if parts.len() == 2 {
                let _ = Command::new("git")
                    .args(["fetch", parts[0], parts[1]])
                    .current_dir(&self.main_worktree)
                    .output();
            }
        }

        // Reset to target
        let output = Command::new("git")
            .args(["reset", "--hard", &target])
            .current_dir(directory)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to reset worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Clean untracked files
        let output = Command::new("git")
            .args(["clean", "-fdx"])
            .current_dir(directory)
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to clean worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        tracing::info!(directory = %directory.display(), target = %target, "reset worktree");

        Ok(())
    }

    fn find_available_name(&self, base: Option<&str>) -> anyhow::Result<WorktreeInfo> {
        let mut rng = rand::rng();

        for _ in 0..26 {
            let name = match base {
                Some(b) => slugify(b),
                None => random_name(&mut rng),
            };
            let branch = format!("omni/{name}");
            let directory = self.root.join(&name);

            // Check if directory exists
            if directory.exists() {
                if base.is_some() {
                    // For custom names, append random suffix
                    continue;
                }
                continue;
            }

            // Check if branch exists
            let output = Command::new("git")
                .args([
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ])
                .current_dir(&self.main_worktree)
                .output()?;

            if output.status.success() {
                continue;
            }

            return Ok(WorktreeInfo {
                name,
                branch,
                directory,
            });
        }

        anyhow::bail!("Failed to generate a unique worktree name")
    }

    fn find_default_branch(&self) -> anyhow::Result<String> {
        // Try remote HEAD first
        let output = Command::new("git")
            .args(["remote"])
            .current_dir(&self.main_worktree)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let remotes: Vec<&str> = stdout
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let remote = if remotes.contains(&"origin") {
            Some("origin")
        } else if remotes.len() == 1 {
            Some(remotes[0])
        } else if remotes.contains(&"upstream") {
            Some("upstream")
        } else {
            None
        };

        if let Some(remote) = remote {
            let output = Command::new("git")
                .args(["symbolic-ref", &format!("refs/remotes/{remote}/HEAD")])
                .current_dir(&self.main_worktree)
                .output()?;

            if output.status.success() {
                let reference = String::from_utf8_lossy(&output.stdout);
                let branch = reference
                    .trim()
                    .strip_prefix("refs/remotes/")
                    .unwrap_or(reference.trim());
                return Ok(branch.to_string());
            }
        }

        // Fall back to local main/master
        for branch in ["main", "master"] {
            let output = Command::new("git")
                .args([
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ])
                .current_dir(&self.main_worktree)
                .output()?;

            if output.status.success() {
                return Ok(branch.to_string());
            }
        }

        anyhow::bail!("Default branch not found")
    }
}

fn random_name(rng: &mut rand::rngs::ThreadRng) -> String {
    let adj = ADJECTIVES.choose(rng).unwrap_or(&"quick");
    let noun = NOUNS.choose(rng).unwrap_or(&"fox");
    format!("{adj}-{noun}")
}

fn slugify(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_handles_spaces() {
        assert_eq!(slugify("my feature"), "my-feature");
    }

    #[test]
    fn slugify_handles_special_chars() {
        assert_eq!(slugify("fix/bug#123"), "fix-bug-123");
    }

    #[test]
    fn random_name_produces_valid_format() {
        let mut rng = rand::rng();
        let name = random_name(&mut rng);
        assert!(name.contains('-'));
        assert!(name.len() > 5);
    }
}
