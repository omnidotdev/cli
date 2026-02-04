//! Initialize command for generating AGENTS.md documentation.

use std::path::Path;

/// The prompt template for generating AGENTS.md.
pub const INIT_PROMPT: &str = r#"Please analyze this codebase and create an AGENTS.md file containing:
1. Build/lint/test commands - especially for running a single test
2. Code style guidelines including imports, formatting, types, naming conventions, error handling, etc.

The file you create will be given to agentic coding agents (such as yourself) that operate in this repository. Make it about 150 lines long.
If there are Cursor rules (in .cursor/rules/ or .cursorrules) or Copilot rules (in .github/copilot-instructions.md), make sure to include them.

If there's already an AGENTS.md, improve it if it's located in {path}"#;

/// Arguments for the init command.
#[derive(Debug, Clone, Default)]
pub struct InitArgs {
    /// Custom path for AGENTS.md (defaults to current directory).
    pub path: Option<String>,
}

/// Get the init prompt with the specified path.
#[must_use]
pub fn get_init_prompt(path: Option<&str>) -> String {
    let path = path.unwrap_or(".");
    INIT_PROMPT.replace("{path}", path)
}

/// Check if AGENTS.md already exists at the given path.
#[must_use]
pub fn agents_md_exists(base_path: Option<&str>) -> bool {
    let base = base_path.unwrap_or(".");
    let path = Path::new(base).join("AGENTS.md");
    path.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_init_prompt_default_path() {
        let prompt = get_init_prompt(None);
        assert!(prompt.contains("located in ."));
    }

    #[test]
    fn test_get_init_prompt_custom_path() {
        let prompt = get_init_prompt(Some("/custom/path"));
        assert!(prompt.contains("located in /custom/path"));
    }

    #[test]
    fn test_prompt_contains_required_elements() {
        let prompt = get_init_prompt(None);
        assert!(prompt.contains("AGENTS.md"));
        assert!(prompt.contains("Build/lint/test commands"));
        assert!(prompt.contains("Code style guidelines"));
        assert!(prompt.contains("Cursor rules"));
        assert!(prompt.contains("Copilot rules"));
    }
}
