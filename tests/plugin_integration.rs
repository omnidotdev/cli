use tempfile::TempDir;

use omni_cli::plugin::{PluginDiscovery, PluginType, run_plugin_command};

fn setup_plugin_dir(dir: &TempDir, name: &str, manifest: &str) {
    let plugin_dir = dir.path().join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();
}

#[test]
fn discover_and_list_plugins() {
    let dir = TempDir::new().unwrap();
    setup_plugin_dir(
        &dir,
        "eden",
        r#"
name = "eden"
version = "0.1.0"
description = "Dev onboarding"
type = "bin"
bin = "echo"
"#,
    );
    setup_plugin_dir(
        &dir,
        "runa",
        r#"
name = "runa"
version = "0.1.0"
description = "Project management"
type = "api"
endpoint = "https://runa.omni.dev/api"
"#,
    );

    let discovery = PluginDiscovery::new(dir.path().to_path_buf());
    let plugins = discovery.discover_all().unwrap();

    assert_eq!(plugins.len(), 2);
    assert_eq!(plugins[0].name, "eden");
    assert_eq!(plugins[1].name, "runa");
}

#[test]
fn discover_find_and_execute_bin_plugin() {
    let dir = TempDir::new().unwrap();
    setup_plugin_dir(
        &dir,
        "test-plugin",
        r#"
name = "test-plugin"
version = "0.1.0"
description = "Test plugin"
type = "bin"
bin = "echo"
"#,
    );

    let discovery = PluginDiscovery::new(dir.path().to_path_buf());
    let plugin = discovery.find("test-plugin").unwrap().unwrap();
    let manifest = plugin.manifest.unwrap();

    assert_eq!(manifest.plugin_type, PluginType::Bin);

    let code = run_plugin_command(&manifest, &["hello"]).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn path_fallback_when_no_manifest() {
    let dir = TempDir::new().unwrap();
    let discovery = PluginDiscovery::new(dir.path().to_path_buf());

    // "echo" should be found on PATH
    let plugin = discovery.find("echo").unwrap();
    assert!(plugin.is_some());
    assert!(plugin.unwrap().manifest.is_none());
}
