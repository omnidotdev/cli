//! Skill system for loading reusable agent instructions
//!
//! Skills are markdown files with YAML frontmatter that provide
//! specialized instructions for specific tasks

mod parse;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use parse::parse_skill_file;

/// Skill information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill identifier (matches directory name)
    pub name: String,
    /// Brief description of when to use this skill
    pub description: String,
    /// Full path to SKILL.md file
    pub location: PathBuf,
    /// Optional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Skill discovery and management
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    /// Discover skills from standard locations
    ///
    /// Searches in order:
    /// 1. Project-local: `.omni/skill/*/SKILL.md`
    /// 2. Global: `~/.config/omni/skill/*/SKILL.md`
    ///
    /// Also checks for compatibility with:
    /// - `.opencode/skill/*/SKILL.md`
    /// - `.claude/skills/*/SKILL.md`
    #[must_use]
    pub fn discover(project_root: &Path) -> Self {
        let mut skills = HashMap::new();

        // Project-local paths (walk up from project root)
        let local_patterns = [".omni/skill", ".opencode/skill", ".claude/skills"];

        for pattern in local_patterns {
            let skill_dir = project_root.join(pattern);
            if skill_dir.is_dir() {
                Self::scan_directory(&skill_dir, &mut skills);
            }
        }

        // Global paths
        if let Some(config_dir) = dirs::config_dir() {
            let global_patterns = [
                config_dir.join("omni/skill"),
                config_dir.join("opencode/skill"),
            ];

            for path in global_patterns {
                if path.is_dir() {
                    Self::scan_directory(&path, &mut skills);
                }
            }
        }

        // Also check ~/.claude/skills for compatibility
        if let Some(home) = dirs::home_dir() {
            let claude_skills = home.join(".claude/skills");
            if claude_skills.is_dir() {
                Self::scan_directory(&claude_skills, &mut skills);
            }
        }

        Self { skills }
    }

    /// Scan a directory for SKILL.md files
    fn scan_directory(dir: &Path, skills: &mut HashMap<String, Skill>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            // Parse skill file
            match parse_skill_file(&skill_file) {
                Ok(skill) => {
                    // Validate name matches directory
                    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if skill.name != dir_name {
                        tracing::warn!(
                            skill = %skill.name,
                            dir = %dir_name,
                            "skill name doesn't match directory name"
                        );
                    }

                    // Check for duplicates
                    if skills.contains_key(&skill.name) {
                        tracing::warn!(
                            skill = %skill.name,
                            path = %skill_file.display(),
                            "duplicate skill, using latest"
                        );
                    }

                    skills.insert(skill.name.clone(), skill);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %skill_file.display(),
                        error = %e,
                        "failed to parse skill file"
                    );
                }
            }
        }
    }

    /// Get a skill by name
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Get all available skills
    #[must_use]
    pub fn all(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// List skill names
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.skills.keys().map(String::as_str).collect()
    }

    /// Check if a skill exists
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Load skill content (the markdown body)
    ///
    /// # Errors
    ///
    /// Returns error if skill doesn't exist or file can't be read
    pub fn load_content(&self, name: &str) -> anyhow::Result<String> {
        let skill = self
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("skill not found: {name}"))?;

        let content = std::fs::read_to_string(&skill.location)?;

        // Extract content after frontmatter
        let content = if let Some(rest) = content.strip_prefix("---") {
            // Find end of frontmatter
            if let Some(end) = rest.find("---") {
                rest[end + 3..].trim().to_string()
            } else {
                content
            }
        } else {
            content
        };

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_skill(dir: &Path, name: &str, description: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSkill content here."
            ),
        ).unwrap();
    }

    #[test]
    fn discover_finds_skills() {
        let dir = TempDir::new().unwrap();
        let skill_root = dir.path().join(".omni/skill");
        fs::create_dir_all(&skill_root).unwrap();

        create_skill(&skill_root, "test-skill", "A test skill");
        create_skill(&skill_root, "another-skill", "Another skill");

        let registry = SkillRegistry::discover(dir.path());

        assert_eq!(registry.all().len(), 2);
        assert!(registry.contains("test-skill"));
        assert!(registry.contains("another-skill"));
    }

    #[test]
    fn get_returns_skill() {
        let dir = TempDir::new().unwrap();
        let skill_root = dir.path().join(".omni/skill");
        fs::create_dir_all(&skill_root).unwrap();

        create_skill(&skill_root, "my-skill", "My skill description");

        let registry = SkillRegistry::discover(dir.path());
        let skill = registry.get("my-skill").unwrap();

        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "My skill description");
    }

    #[test]
    fn load_content_extracts_body() {
        let dir = TempDir::new().unwrap();
        let skill_root = dir.path().join(".omni/skill");
        fs::create_dir_all(&skill_root).unwrap();

        create_skill(&skill_root, "content-skill", "Test");

        let registry = SkillRegistry::discover(dir.path());
        let content = registry.load_content("content-skill").unwrap();

        assert!(content.contains("# content-skill"));
        assert!(content.contains("Skill content here."));
    }

    #[test]
    fn missing_skill_returns_none() {
        let registry = SkillRegistry::default();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let registry = SkillRegistry::default();
        assert!(registry.load_content("nonexistent").is_err());
    }
}
