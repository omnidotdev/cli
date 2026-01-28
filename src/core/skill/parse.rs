//! YAML frontmatter parsing for skill files

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use super::Skill;

/// Raw frontmatter structure
#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

/// Parse a SKILL.md file
///
/// # Errors
///
/// Returns error if file can't be read or frontmatter is invalid
pub fn parse_skill_file(path: &Path) -> anyhow::Result<Skill> {
    let content = std::fs::read_to_string(path)?;

    // Must start with ---
    if !content.starts_with("---") {
        anyhow::bail!("skill file must start with YAML frontmatter");
    }

    // Find end of frontmatter
    let rest = &content[3..];
    let end = rest
        .find("---")
        .ok_or_else(|| anyhow::anyhow!("unterminated frontmatter"))?;

    let yaml = &rest[..end];
    let frontmatter: Frontmatter = serde_yaml::from_str(yaml)?;

    // Validate name
    validate_name(&frontmatter.name)?;

    Ok(Skill {
        name: frontmatter.name,
        description: frontmatter.description,
        location: path.to_path_buf(),
        metadata: frontmatter.metadata,
    })
}

/// Validate skill name format
///
/// Rules:
/// - 1-64 characters
/// - Lowercase alphanumeric with single hyphens
/// - Cannot start/end with hyphen
/// - No consecutive hyphens
fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("skill name cannot be empty");
    }

    if name.len() > 64 {
        anyhow::bail!("skill name too long (max 64 chars)");
    }

    if name.starts_with('-') || name.ends_with('-') {
        anyhow::bail!("skill name cannot start or end with hyphen");
    }

    if name.contains("--") {
        anyhow::bail!("skill name cannot contain consecutive hyphens");
    }

    for c in name.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            anyhow::bail!("skill name must be lowercase alphanumeric with hyphens");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parse_valid_skill() {
        let dir = TempDir::new().unwrap();
        let skill_file = dir.path().join("SKILL.md");
        fs::write(
            &skill_file,
            r#"---
name: test-skill
description: A test skill for testing
metadata:
  audience: developers
---

# Test Skill

This is the skill content.
"#,
        )
        .unwrap();

        let skill = parse_skill_file(&skill_file).unwrap();
        assert_eq!(skill.name, "test-skill");
        assert_eq!(skill.description, "A test skill for testing");
        assert_eq!(
            skill.metadata.get("audience"),
            Some(&"developers".to_string())
        );
    }

    #[test]
    fn parse_minimal_skill() {
        let dir = TempDir::new().unwrap();
        let skill_file = dir.path().join("SKILL.md");
        fs::write(
            &skill_file,
            r#"---
name: minimal
description: Minimal skill
---
Content here.
"#,
        )
        .unwrap();

        let skill = parse_skill_file(&skill_file).unwrap();
        assert_eq!(skill.name, "minimal");
        assert!(skill.metadata.is_empty());
    }

    #[test]
    fn reject_missing_frontmatter() {
        let dir = TempDir::new().unwrap();
        let skill_file = dir.path().join("SKILL.md");
        fs::write(&skill_file, "# Just markdown").unwrap();

        assert!(parse_skill_file(&skill_file).is_err());
    }

    #[test]
    fn reject_unterminated_frontmatter() {
        let dir = TempDir::new().unwrap();
        let skill_file = dir.path().join("SKILL.md");
        fs::write(&skill_file, "---\nname: broken\n").unwrap();

        assert!(parse_skill_file(&skill_file).is_err());
    }

    #[test]
    fn validate_name_accepts_valid() {
        assert!(validate_name("test").is_ok());
        assert!(validate_name("test-skill").is_ok());
        assert!(validate_name("my-cool-skill").is_ok());
        assert!(validate_name("a1b2c3").is_ok());
    }

    #[test]
    fn validate_name_rejects_invalid() {
        assert!(validate_name("").is_err());
        assert!(validate_name("-start").is_err());
        assert!(validate_name("end-").is_err());
        assert!(validate_name("double--hyphen").is_err());
        assert!(validate_name("UPPERCASE").is_err());
        assert!(validate_name("has_underscore").is_err());
        assert!(validate_name("has space").is_err());
    }
}
