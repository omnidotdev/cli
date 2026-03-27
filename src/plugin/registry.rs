use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

use crate::plugin::{CommandDef, PackageHints, PluginManifest, PluginType};

/// Default registry URL
const DEFAULT_REGISTRY_URL: &str = "https://manifold.omni.dev";

/// Response shape for GET /api/plugins/:name
#[derive(Deserialize)]
struct PluginResponse {
    plugin: RegistryPlugin,
}

/// Plugin as returned by the registry API
#[derive(Deserialize)]
struct RegistryPlugin {
    name: String,
    version: String,
    description: String,
    #[serde(rename = "type")]
    plugin_type: String,
    bin: Option<String>,
    endpoint: Option<String>,
    #[serde(default)]
    commands: HashMap<String, CommandDef>,
    packages: Option<PackageHints>,
}

impl TryFrom<RegistryPlugin> for PluginManifest {
    type Error = anyhow::Error;

    fn try_from(rp: RegistryPlugin) -> anyhow::Result<Self> {
        let plugin_type = match rp.plugin_type.as_str() {
            "bin" => PluginType::Bin,
            "api" => PluginType::Api,
            "launch" => PluginType::Launch,
            other => anyhow::bail!("unknown plugin type: {other}"),
        };

        Ok(Self {
            name: rp.name,
            version: rp.version,
            description: rp.description,
            plugin_type,
            bin: rp.bin,
            endpoint: rp.endpoint,
            commands: rp.commands,
            packages: rp.packages,
        })
    }
}

/// Resolve the registry base URL from env or config
#[must_use]
pub fn registry_url() -> String {
    std::env::var("OMNI_PLUGIN_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string())
}

/// Fetch a plugin manifest from the remote registry
///
/// # Errors
///
/// Returns an error if the request fails or the response is invalid
pub async fn fetch_plugin(base_url: &str, name: &str) -> anyhow::Result<PluginManifest> {
    let url = format!("{base_url}/api/plugins/{name}");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let resp = client.get(&url).send().await?;

    if resp.status() == 404 {
        anyhow::bail!("plugin '{name}' not found in registry");
    }

    if !resp.status().is_success() {
        anyhow::bail!("registry returned HTTP {}", resp.status());
    }

    let body: PluginResponse = resp.json().await?;
    body.plugin.try_into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_plugin_converts_to_manifest() {
        let rp = RegistryPlugin {
            name: "test".into(),
            version: "0.1.0".into(),
            description: "a test plugin".into(),
            plugin_type: "bin".into(),
            bin: Some("test-bin".into()),
            endpoint: None,
            commands: HashMap::new(),
            packages: Some(PackageHints {
                aur: Some("test-aur".into()),
                homebrew: Some("test/tap/test".into()),
            }),
        };

        let manifest: PluginManifest = rp.try_into().unwrap();
        assert_eq!(manifest.name, "test");
        assert_eq!(manifest.plugin_type, PluginType::Bin);
        assert_eq!(manifest.bin.as_deref(), Some("test-bin"));
        assert!(manifest.packages.is_some());
    }

    #[test]
    fn invalid_plugin_type_errors() {
        let rp = RegistryPlugin {
            name: "bad".into(),
            version: "0.1.0".into(),
            description: "bad".into(),
            plugin_type: "invalid".into(),
            bin: None,
            endpoint: None,
            commands: HashMap::new(),
            packages: None,
        };

        let result: Result<PluginManifest, _> = rp.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn default_registry_url_is_set() {
        // When env var is not set, should return the default
        let url = DEFAULT_REGISTRY_URL;
        assert!(url.starts_with("https://"));
    }
}
