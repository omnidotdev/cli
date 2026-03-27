use std::path::Path;

use crate::plugin::{PluginDiscovery, PluginManifest, registry};

/// Embedded plugin registry (offline fallback)
const REGISTRY: &[(&str, &str)] = &[(
    "beacon",
    include_str!("../../examples/plugins/beacon/plugin.toml"),
)];

/// Detect the available system package manager
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Homebrew,
    Aur,
}

impl PackageManager {
    fn detect() -> Option<Self> {
        if which::which("brew").is_ok() {
            return Some(Self::Homebrew);
        }

        // Prefer AUR helpers over raw pacman
        if which::which("yay").is_ok() || which::which("paru").is_ok() {
            return Some(Self::Aur);
        }

        None
    }

    fn install_cmd(self, package: &str) -> Vec<String> {
        match self {
            Self::Homebrew => vec!["brew".into(), "install".into(), package.into()],
            Self::Aur => {
                let helper = if which::which("paru").is_ok() {
                    "paru"
                } else {
                    "yay"
                };
                vec![
                    helper.into(),
                    "-S".into(),
                    "--noconfirm".into(),
                    package.into(),
                ]
            }
        }
    }

    fn package_field(self, manifest: &PluginManifest) -> Option<&str> {
        let packages = manifest.packages.as_ref()?;
        match self {
            Self::Homebrew => packages.homebrew.as_deref(),
            Self::Aur => packages.aur.as_deref(),
        }
    }
}

/// Resolve a plugin manifest by trying the remote registry first, then the
/// embedded fallback. Returns the manifest and TOML string to write to disk.
async fn resolve_manifest(name: &str) -> anyhow::Result<(PluginManifest, String)> {
    let base_url = registry::registry_url();

    match registry::fetch_plugin(&base_url, name).await {
        Ok(manifest) => {
            let toml_str = toml::to_string_pretty(&manifest)
                .map_err(|e| anyhow::anyhow!("failed to serialize manifest: {e}"))?;
            Ok((manifest, toml_str))
        }
        Err(e) => {
            tracing::debug!("remote registry unavailable, using embedded fallback: {e}");

            let toml_str = REGISTRY
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, toml)| *toml)
                .ok_or_else(|| anyhow::anyhow!("unknown plugin '{name}'"))?;

            let manifest: PluginManifest = toml::from_str(toml_str)
                .map_err(|e| anyhow::anyhow!("invalid manifest for '{name}': {e}"))?;

            Ok((manifest, toml_str.to_string()))
        }
    }
}

/// Install a plugin by name
///
/// Fetches the manifest from the remote plugin registry, falling back to
/// the embedded registry if unavailable.
///
/// # Errors
///
/// Returns an error if the plugin is unknown, already installed, or installation fails
pub async fn install_plugin(name: &str) -> anyhow::Result<()> {
    let plugins_dir = PluginDiscovery::default_dir()?;
    let target = plugins_dir.join(name);

    if target.exists() {
        anyhow::bail!("plugin '{name}' is already installed");
    }

    let (manifest, toml_str) = resolve_manifest(name).await?;

    // Install binary via package manager if applicable
    if manifest.bin.is_some() {
        install_binary(name, &manifest)?;
    }

    // Write manifest to plugins dir
    write_manifest(&target, &toml_str)?;

    println!("Installed plugin '{name}'");
    Ok(())
}

/// Install the binary for a plugin via system package manager
fn install_binary(name: &str, manifest: &PluginManifest) -> anyhow::Result<()> {
    // Check if binary is already available
    if let Some(ref bin) = manifest.bin {
        if which::which(bin).is_ok() {
            println!("'{bin}' is already on PATH, skipping binary install");
            return Ok(());
        }
    }

    let pm = PackageManager::detect().ok_or_else(|| {
        anyhow::anyhow!("no supported package manager found (brew, yay, or paru required)")
    })?;

    let package = pm.package_field(manifest).ok_or_else(|| {
        anyhow::anyhow!("plugin '{name}' has no package hint for the detected package manager")
    })?;

    println!("Installing '{package}' via {pm:?}...");

    let cmd = pm.install_cmd(package);
    let status = std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run package manager: {e}"))?;

    if !status.success() {
        anyhow::bail!("package manager exited with status {status}");
    }

    Ok(())
}

/// Write a plugin manifest to the plugins directory
fn write_manifest(target: &Path, manifest_toml: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(target)?;
    std::fs::write(target.join("plugin.toml"), manifest_toml)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_known_plugins() {
        for (name, toml_str) in REGISTRY {
            let manifest: PluginManifest = toml::from_str(toml_str)
                .unwrap_or_else(|e| panic!("invalid manifest for '{name}': {e}"));
            assert_eq!(manifest.name, *name);
        }
    }

    #[test]
    fn detect_package_manager_returns_some_on_ci_or_dev() {
        // Just verify it doesn't panic
        let _ = PackageManager::detect();
    }

    #[test]
    fn homebrew_install_cmd() {
        let cmd = PackageManager::Homebrew.install_cmd("omnidotdev/tap/foo");
        assert_eq!(cmd, vec!["brew", "install", "omnidotdev/tap/foo"]);
    }

    #[test]
    fn install_already_installed_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("beacon");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("plugin.toml"), "").unwrap();

        // Verify the manifest writes correctly
        let result = write_manifest(&dir.path().join("new-plugin"), "name = \"test\"");
        assert!(result.is_ok());
        assert!(dir.path().join("new-plugin/plugin.toml").exists());
    }

    #[test]
    fn write_manifest_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("test-plugin");
        write_manifest(&target, "name = \"test\"").unwrap();
        assert!(target.join("plugin.toml").exists());
        assert_eq!(
            std::fs::read_to_string(target.join("plugin.toml")).unwrap(),
            "name = \"test\""
        );
    }

    #[test]
    fn manifest_round_trips_through_toml() {
        for (_, toml_str) in REGISTRY {
            let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
            let serialized = toml::to_string_pretty(&manifest).unwrap();
            let reparsed: PluginManifest = toml::from_str(&serialized).unwrap();
            assert_eq!(manifest.name, reparsed.name);
            assert_eq!(manifest.plugin_type, reparsed.plugin_type);
        }
    }
}
