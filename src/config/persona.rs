//! Persona configuration for AI assistant personalities.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A persona defines the AI assistant's personality and behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// Display name for the persona.
    pub name: String,

    /// Short tagline describing the persona.
    #[serde(default)]
    pub tagline: Option<String>,

    /// Personality description (e.g., "Calm, precise, slightly formal").
    #[serde(default)]
    pub personality: Option<String>,

    /// Areas of expertise.
    #[serde(default)]
    pub expertise: Vec<String>,

    /// System prompt content.
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl Default for Persona {
    fn default() -> Self {
        Self::orin()
    }
}

impl Persona {
    /// Create the default Orin persona.
    #[must_use]
    pub fn orin() -> Self {
        Self {
            name: "Orin".to_string(),
            tagline: Some("Omni's friendly otter assistant".to_string()),
            personality: Some("Friendly, helpful, and knowledgeable about the Omni ecosystem".to_string()),
            expertise: vec![
                "Omni ecosystem".to_string(),
                "software development".to_string(),
                "coding assistance".to_string(),
            ],
            system_prompt: Some(
                "You are Orin, a friendly and helpful AI assistant created by Omni. \
                 You specialize in software development and are knowledgeable about the Omni ecosystem. \
                 You communicate in a warm, approachable manner while providing accurate technical guidance."
                    .to_string(),
            ),
        }
    }

    /// Load a persona from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let persona: Self = toml::from_str(&contents)?;
        Ok(persona)
    }

    /// Save a persona to a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Get the system prompt for this persona.
    #[must_use]
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Build a full system prompt including personality and expertise.
    #[must_use]
    pub fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref prompt) = self.system_prompt {
            parts.push(prompt.clone());
        } else {
            parts.push(format!("You are {}, an AI assistant.", self.name));
        }

        if let Some(ref personality) = self.personality {
            parts.push(format!("Your personality: {personality}."));
        }

        if !self.expertise.is_empty() {
            let expertise_str = self.expertise.join(", ");
            parts.push(format!("Your areas of expertise include: {expertise_str}."));
        }

        parts.join(" ")
    }
}

/// Get the personas directory path.
///
/// # Errors
///
/// Returns an error if the config directory cannot be determined.
pub fn personas_dir() -> anyhow::Result<PathBuf> {
    Ok(super::Config::config_dir()?.join("personas"))
}

/// List available personas.
///
/// # Errors
///
/// Returns an error if the personas directory cannot be read.
pub fn list_personas() -> anyhow::Result<Vec<String>> {
    let dir = personas_dir()?;

    if !dir.exists() {
        return Ok(vec!["orin".to_string()]);
    }

    let mut personas = vec!["orin".to_string()]; // Orin is always available.

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|e| e == "toml") {
            if let Some(stem) = path.file_stem() {
                let name = stem.to_string_lossy().to_string();
                if name != "orin" {
                    personas.push(name);
                }
            }
        }
    }

    personas.sort();
    Ok(personas)
}

/// Load a persona by name.
///
/// Returns Orin if the name is "orin" or if the persona file doesn't exist.
///
/// # Errors
///
/// Returns an error if the persona file exists but cannot be read.
pub fn load_persona(name: &str) -> anyhow::Result<Persona> {
    if name.eq_ignore_ascii_case("orin") {
        return Ok(Persona::orin());
    }

    let path = personas_dir()?.join(format!("{name}.toml"));

    if path.exists() {
        Persona::load(&path)
    } else {
        // Fall back to Orin if persona not found.
        tracing::warn!(persona = %name, "persona not found, using Orin");
        Ok(Persona::orin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orin_has_system_prompt() {
        let orin = Persona::orin();
        assert!(orin.system_prompt.is_some());
        assert_eq!(orin.name, "Orin");
    }

    #[test]
    fn build_system_prompt_combines_parts() {
        let persona = Persona {
            name: "Test".to_string(),
            tagline: None,
            personality: Some("Helpful".to_string()),
            expertise: vec!["Rust".to_string(), "Python".to_string()],
            system_prompt: Some("You are a test assistant.".to_string()),
        };

        let prompt = persona.build_system_prompt();
        assert!(prompt.contains("test assistant"));
        assert!(prompt.contains("Helpful"));
        assert!(prompt.contains("Rust"));
    }
}
