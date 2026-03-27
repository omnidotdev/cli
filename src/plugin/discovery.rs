use std::path::PathBuf;

use crate::plugin::PluginManifest;

/// Where the plugin was discovered
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    /// Found in the plugins config directory
    PluginDir,
    /// Found on PATH via `which`
    Path,
}

/// A discovered plugin with optional manifest
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub manifest: Option<PluginManifest>,
    pub source: PluginSource,
}

/// Discovers plugins from the config directory and PATH
pub struct PluginDiscovery {
    plugins_dir: PathBuf,
}

impl PluginDiscovery {
    #[must_use]
    pub const fn new(plugins_dir: PathBuf) -> Self {
        Self { plugins_dir }
    }

    /// Return the default plugins directory
    ///
    /// # Errors
    ///
    /// Returns an error if the base directories cannot be determined
    pub fn default_dir() -> anyhow::Result<PathBuf> {
        let base = directories::BaseDirs::new()
            .ok_or_else(|| anyhow::anyhow!("cannot determine base directories"))?;
        Ok(base.config_dir().join("omni").join("plugins"))
    }

    /// Find a plugin by name
    ///
    /// Checks the plugin directory first, then falls back to PATH
    ///
    /// # Errors
    ///
    /// Returns an error if a manifest file exists but cannot be parsed
    pub fn find(&self, name: &str) -> anyhow::Result<Option<DiscoveredPlugin>> {
        let manifest_path = self.plugins_dir.join(name).join("plugin.toml");

        if manifest_path.exists() {
            let manifest = PluginManifest::from_file(&manifest_path)?;
            return Ok(Some(DiscoveredPlugin {
                name: name.to_string(),
                manifest: Some(manifest),
                source: PluginSource::PluginDir,
            }));
        }

        // Fall back to PATH with `omni-` prefix convention (like git/cargo)
        let prefixed = format!("omni-{name}");
        if which::which(&prefixed).is_ok() {
            return Ok(Some(DiscoveredPlugin {
                name: name.to_string(),
                manifest: None,
                source: PluginSource::Path,
            }));
        }

        Ok(None)
    }

    /// Discover all plugins in the plugins directory
    ///
    /// # Errors
    ///
    /// Returns an error if a manifest file exists but cannot be parsed
    pub fn discover_all(&self) -> anyhow::Result<Vec<DiscoveredPlugin>> {
        if !self.plugins_dir.exists() {
            return Ok(Vec::new());
        }

        let mut plugins = Vec::new();
        let entries = std::fs::read_dir(&self.plugins_dir)?;

        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let manifest_path = entry.path().join("plugin.toml");

            if manifest_path.exists() {
                let manifest = PluginManifest::from_file(&manifest_path)?;
                plugins.push(DiscoveredPlugin {
                    name,
                    manifest: Some(manifest),
                    source: PluginSource::PluginDir,
                });
            }
        }

        plugins.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(plugins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(dir: &std::path::Path, name: &str, content: &str) {
        let plugin_dir = dir.join(name);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.toml"), content).unwrap();
    }

    const EDEN_MANIFEST: &str = r#"
name = "eden"
version = "0.1.0"
description = "Developer onboarding preflight checks"
type = "bin"
bin = "eden"
"#;

    const RUNA_MANIFEST: &str = r#"
name = "runa"
version = "0.1.0"
description = "Streamlined project management"
type = "api"
endpoint = "https://runa.omni.dev/api"
"#;

    #[test]
    fn discover_from_plugins_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_manifest(dir.path(), "eden", EDEN_MANIFEST);
        write_manifest(dir.path(), "runa", RUNA_MANIFEST);

        let discovery = PluginDiscovery::new(dir.path().to_path_buf());
        let plugins = discovery.discover_all().unwrap();
        assert_eq!(plugins.len(), 2);
    }

    #[test]
    fn find_by_name_from_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_manifest(dir.path(), "eden", EDEN_MANIFEST);

        let discovery = PluginDiscovery::new(dir.path().to_path_buf());
        let plugin = discovery.find("eden").unwrap();
        assert!(plugin.is_some());
        let p = plugin.unwrap();
        assert_eq!(p.name, "eden");
        assert!(p.manifest.is_some());
        assert_eq!(p.source, PluginSource::PluginDir);
    }

    #[test]
    fn find_not_installed_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let discovery = PluginDiscovery::new(dir.path().to_path_buf());
        let plugin = discovery.find("nonexistent").unwrap();
        assert!(plugin.is_none());
    }

    #[test]
    fn find_does_not_fall_back_to_unprefixed_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let discovery = PluginDiscovery::new(dir.path().to_path_buf());
        // "ls" exists on PATH but "omni-ls" does not
        let plugin = discovery.find("ls").unwrap();
        assert!(plugin.is_none());
    }

    #[test]
    fn plugin_dir_takes_priority_over_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest = r#"
name = "ls"
version = "0.1.0"
description = "Custom ls plugin"
type = "bin"
bin = "ls"
"#;
        write_manifest(dir.path(), "ls", manifest);

        let discovery = PluginDiscovery::new(dir.path().to_path_buf());
        let plugin = discovery.find("ls").unwrap();
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().source, PluginSource::PluginDir);
    }

    #[test]
    fn empty_plugins_dir_returns_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let discovery = PluginDiscovery::new(dir.path().to_path_buf());
        let plugins = discovery.discover_all().unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn nonexistent_dir_returns_empty() {
        let discovery = PluginDiscovery::new(PathBuf::from("/nonexistent/plugins"));
        let plugins = discovery.discover_all().unwrap();
        assert!(plugins.is_empty());
    }
}
