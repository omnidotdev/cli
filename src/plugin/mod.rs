pub mod discovery;
pub mod exec;
pub mod manifest;

pub use discovery::{DiscoveredPlugin, PluginDiscovery, PluginSource};
pub use exec::{resolve_bin_and_args, run_plugin_command};
pub use manifest::{CommandDef, PackageHints, PluginManifest, PluginType};
