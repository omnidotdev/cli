use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Plugin type determines how the CLI delegates to the plugin
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    /// Delegates to an external binary
    Bin,
    /// Makes HTTP requests to an API endpoint
    Api,
    /// Launches a desktop application
    Launch,
}

/// A command exposed by a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    pub description: String,
    /// HTTP method for API plugins
    pub method: Option<String>,
    /// URL path for API plugins
    pub path: Option<String>,
}

/// System package manager hints for installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageHints {
    pub aur: Option<String>,
    pub homebrew: Option<String>,
}

/// Plugin manifest parsed from plugin.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    /// Binary name for bin/launch plugins
    pub bin: Option<String>,
    /// API endpoint for api plugins
    pub endpoint: Option<String>,
    /// Commands exposed by this plugin
    #[serde(default)]
    pub commands: HashMap<String, CommandDef>,
    /// System package manager hints
    pub packages: Option<PackageHints>,
}

impl PluginManifest {
    /// Read and parse a plugin manifest from a TOML file
    ///
    /// Expands `${VAR}` references in the `endpoint` field
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let mut manifest: Self = toml::from_str(&contents)?;

        if let Some(ref ep) = manifest.endpoint {
            manifest.endpoint = Some(expand_env_vars(ep));
        }

        Ok(manifest)
    }
}

/// Expand `${VAR}` references using environment variables
fn expand_env_vars(input: &str) -> String {
    let re = Regex::new(r"\$\{([^}]+)\}").expect("valid regex");
    re.replace_all(input, |caps: &regex::Captures| {
        std::env::var(&caps[1]).unwrap_or_default()
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN_MANIFEST: &str = r#"
name = "eden"
version = "0.1.0"
description = "Developer onboarding preflight checks"
type = "bin"
bin = "eden"

[commands.check]
description = "Run preflight checks"

[commands.init]
description = "Initialize a new project environment"
"#;

    const API_MANIFEST: &str = r#"
name = "runa"
version = "0.1.0"
description = "Streamlined project management"
type = "api"
endpoint = "https://runa.omni.dev/api"

[commands.list]
description = "List projects"
method = "GET"
path = "/projects"

[commands.create]
description = "Create a new task"
method = "POST"
path = "/tasks"
"#;

    const LAUNCH_MANIFEST: &str = r#"
name = "terminal"
version = "0.1.0"
description = "GPU-accelerated terminal emulator"
type = "launch"
bin = "omni-terminal"
"#;

    #[test]
    fn parse_bin_manifest() {
        let manifest: PluginManifest = toml::from_str(BIN_MANIFEST).unwrap();
        assert_eq!(manifest.name, "eden");
        assert_eq!(manifest.plugin_type, PluginType::Bin);
        assert_eq!(manifest.bin.as_deref(), Some("eden"));
        assert_eq!(manifest.commands.len(), 2);
        assert!(manifest.commands.contains_key("check"));
    }

    #[test]
    fn parse_api_manifest() {
        let manifest: PluginManifest = toml::from_str(API_MANIFEST).unwrap();
        assert_eq!(manifest.name, "runa");
        assert_eq!(manifest.plugin_type, PluginType::Api);
        assert_eq!(
            manifest.endpoint.as_deref(),
            Some("https://runa.omni.dev/api")
        );
        let list_cmd = &manifest.commands["list"];
        assert_eq!(list_cmd.method.as_deref(), Some("GET"));
        assert_eq!(list_cmd.path.as_deref(), Some("/projects"));
    }

    #[test]
    fn parse_launch_manifest() {
        let manifest: PluginManifest = toml::from_str(LAUNCH_MANIFEST).unwrap();
        assert_eq!(manifest.plugin_type, PluginType::Launch);
        assert_eq!(manifest.bin.as_deref(), Some("omni-terminal"));
    }

    #[test]
    fn from_file_reads_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("plugin.toml");
        std::fs::write(&path, BIN_MANIFEST).unwrap();
        let manifest = PluginManifest::from_file(&path).unwrap();
        assert_eq!(manifest.name, "eden");
    }

    #[test]
    fn from_file_missing_returns_error() {
        let result = PluginManifest::from_file(std::path::Path::new("/nonexistent/plugin.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn invalid_type_returns_error() {
        let bad = r#"
name = "bad"
version = "0.1.0"
description = "bad"
type = "invalid"
"#;
        let result: Result<PluginManifest, _> = toml::from_str(bad);
        assert!(result.is_err());
    }

    #[test]
    fn expand_env_vars_substitutes_known_vars() {
        // HOME is always set in test environments
        let result = super::expand_env_vars("${HOME}/plugins");
        assert!(!result.contains("${HOME}"));
        assert!(result.ends_with("/plugins"));
        assert!(result.len() > "/plugins".len());
    }

    #[test]
    fn expand_env_vars_missing_var_becomes_empty() {
        let result = super::expand_env_vars("${NONEXISTENT_VAR_12345}/api");
        assert_eq!(result, "/api");
    }

    #[test]
    fn expand_env_vars_no_vars_unchanged() {
        let result = super::expand_env_vars("https://example.com/api");
        assert_eq!(result, "https://example.com/api");
    }

    #[test]
    fn missing_required_fields_returns_error() {
        let bad = r#"
name = "bad"
"#;
        let result: Result<PluginManifest, _> = toml::from_str(bad);
        assert!(result.is_err());
    }
}
