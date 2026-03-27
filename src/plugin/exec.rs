use std::process::Command;

use crate::plugin::{PluginManifest, PluginType};

/// Resolve the binary name and args for a plugin command
///
/// # Errors
///
/// Returns an error if the plugin type requires a binary but none is specified
pub fn resolve_bin_and_args<'a>(
    manifest: &PluginManifest,
    args: &'a [&'a str],
) -> anyhow::Result<(String, &'a [&'a str])> {
    match manifest.plugin_type {
        PluginType::Bin | PluginType::Launch => {
            let bin = manifest
                .bin
                .as_ref()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "plugin '{}' has type {:?} but no bin field",
                        manifest.name,
                        manifest.plugin_type
                    )
                })?
                .clone();
            Ok((bin, args))
        }
        PluginType::Api => {
            anyhow::bail!(
                "plugin '{}' is an API adapter and cannot be executed as a binary",
                manifest.name
            );
        }
    }
}

/// Execute a plugin command by spawning the resolved binary
///
/// Returns the process exit code
///
/// # Errors
///
/// Returns an error if the binary cannot be found or spawned
pub fn run_plugin_command(manifest: &PluginManifest, args: &[&str]) -> anyhow::Result<i32> {
    let (bin, args) = resolve_bin_and_args(manifest, args)?;

    let status = Command::new(&bin)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to execute '{bin}': {e}"))?;

    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{PluginManifest, PluginType};

    fn make_manifest(plugin_type: PluginType, bin: Option<&str>) -> PluginManifest {
        PluginManifest {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            description: "test".to_string(),
            plugin_type,
            bin: bin.map(String::from),
            endpoint: None,
            commands: Default::default(),
            packages: None,
        }
    }

    #[test]
    fn resolve_bin_for_bin_plugin() {
        let manifest = make_manifest(PluginType::Bin, Some("echo"));
        let (bin, args) = resolve_bin_and_args(&manifest, &["hello"]).unwrap();
        assert_eq!(bin, "echo");
        assert_eq!(args, &["hello"]);
    }

    #[test]
    fn resolve_bin_for_launch_plugin() {
        let manifest = make_manifest(PluginType::Launch, Some("echo"));
        let empty: &[&str] = &[];
        let (bin, args) = resolve_bin_and_args(&manifest, empty).unwrap();
        assert_eq!(bin, "echo");
        assert!(args.is_empty());
    }

    #[test]
    fn resolve_bin_missing_returns_error() {
        let manifest = make_manifest(PluginType::Bin, None);
        let empty: &[&str] = &[];
        let result = resolve_bin_and_args(&manifest, empty);
        assert!(result.is_err());
    }

    #[test]
    fn run_bin_plugin_succeeds() {
        let manifest = make_manifest(PluginType::Bin, Some("echo"));
        let code = run_plugin_command(&manifest, &["hello"]).unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn run_missing_binary_returns_error() {
        let manifest = make_manifest(PluginType::Bin, Some("nonexistent-binary-12345"));
        let result = run_plugin_command(&manifest, &[]);
        assert!(result.is_err());
    }
}
