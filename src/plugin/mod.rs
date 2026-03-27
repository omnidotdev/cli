pub mod discovery;
pub mod exec;
pub mod install;
pub mod manifest;
pub mod registry;

pub use discovery::{DiscoveredPlugin, PluginDiscovery, PluginSource};
pub use exec::{resolve_bin_and_args, run_plugin_command};
pub use install::install_plugin;
pub use manifest::{CommandDef, PackageHints, PluginManifest, PluginType};
