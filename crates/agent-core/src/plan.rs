//! Plan file management for plan mode.

use std::path::{Path, PathBuf};

use chrono::Local;

/// Manages plan file storage and validation.
pub struct PlanManager {
    /// Project root if in a git repo.
    project_root: Option<PathBuf>,
    /// Global plans directory.
    global_dir: PathBuf,
}

impl PlanManager {
    /// Create a new plan manager, detecting project root from current directory.
    #[must_use]
    pub fn new() -> Self {
        let project_root = Self::detect_project_root();
        let global_dir = Self::default_global_dir();

        Self {
            project_root,
            global_dir,
        }
    }

    /// Create a plan manager with explicit paths (for testing).
    #[must_use]
    pub const fn with_paths(project_root: Option<PathBuf>, global_dir: PathBuf) -> Self {
        Self {
            project_root,
            global_dir,
        }
    }

    /// Generate a path for a new plan file.
    #[must_use]
    pub fn new_plan_path(&self, slug: &str) -> PathBuf {
        let sanitized = Self::sanitize_slug(slug);
        let date = Local::now().format("%Y-%m-%d");
        let filename = format!("{date}-{sanitized}.md");

        if let Some(root) = &self.project_root {
            root.join(".omni").join("plans").join(filename)
        } else {
            self.global_dir.join(filename)
        }
    }

    /// Check if a path is a valid plan file location.
    #[must_use]
    pub fn is_plan_path(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // Check project-local plans directory
        if let Some(root) = &self.project_root {
            let plans_dir = root.join(".omni").join("plans");
            if path.starts_with(&plans_dir) && path_str.ends_with(".md") {
                return true;
            }
        }

        // Check global plans directory
        if path.starts_with(&self.global_dir) && path_str.ends_with(".md") {
            return true;
        }

        false
    }

    /// Get the plans directory (creates if needed).
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn ensure_plans_dir(&self) -> std::io::Result<PathBuf> {
        let dir = if let Some(root) = &self.project_root {
            root.join(".omni").join("plans")
        } else {
            self.global_dir.clone()
        };

        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Detect git project root from current directory.
    fn detect_project_root() -> Option<PathBuf> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()?;

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Some(PathBuf::from(path))
        } else {
            None
        }
    }

    /// Get default global plans directory.
    fn default_global_dir() -> PathBuf {
        directories::BaseDirs::new()
            .map(|base| base.data_dir().join("omni").join("cli").join("plans"))
            .unwrap_or_else(|| PathBuf::from(".omni/plans"))
    }

    /// Sanitize a slug for use in filenames.
    fn sanitize_slug(slug: &str) -> String {
        slug.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else if c.is_whitespace() {
                    '-'
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches(|c| c == '-' || c == '_')
            .to_string()
    }
}

impl Default for PlanManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_slug_handles_spaces() {
        assert_eq!(
            PlanManager::sanitize_slug("refactor auth system"),
            "refactor-auth-system"
        );
    }

    #[test]
    fn sanitize_slug_handles_special_chars() {
        assert_eq!(
            PlanManager::sanitize_slug("add feature: login!"),
            "add-feature_-login"
        );
    }

    #[test]
    fn is_plan_path_validates_global() {
        let manager = PlanManager::with_paths(None, PathBuf::from("/tmp/plans"));
        assert!(manager.is_plan_path(Path::new("/tmp/plans/2026-01-26-test.md")));
        assert!(!manager.is_plan_path(Path::new("/tmp/plans/2026-01-26-test.txt")));
        assert!(!manager.is_plan_path(Path::new("/other/path.md")));
    }

    #[test]
    fn is_plan_path_validates_project_local() {
        let manager =
            PlanManager::with_paths(Some(PathBuf::from("/project")), PathBuf::from("/tmp/plans"));
        assert!(manager.is_plan_path(Path::new("/project/.omni/plans/test.md")));
        assert!(!manager.is_plan_path(Path::new("/project/src/test.md")));
    }
}
