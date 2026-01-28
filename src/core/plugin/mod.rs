//! Plugin system for extensible capabilities
//!
//! Provides hooks for custom tools, providers, and event handling

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: Option<String>,
    /// Plugin author
    pub author: Option<String>,
}

/// Tool definition for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema (JSON Schema)
    pub input_schema: serde_json::Value,
}

/// Tool execution result
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Result output
    pub output: String,
    /// Whether the tool execution errored
    pub is_error: bool,
}

/// Plugin hook for customizing behavior
#[async_trait]
pub trait PluginHooks: Send + Sync {
    /// Called when plugin is loaded
    async fn on_load(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called when plugin is unloaded
    async fn on_unload(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Get custom tools provided by this plugin
    fn tools(&self) -> Vec<ToolDefinition> {
        Vec::new()
    }

    /// Execute a custom tool
    async fn execute_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
    ) -> anyhow::Result<ToolResult> {
        anyhow::bail!("Tool not found")
    }

    /// Called before a message is sent
    async fn on_message_before(&self, _message: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Called after a response is received
    async fn on_message_after(&self, _response: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called before a tool is executed
    async fn on_tool_before(
        &self,
        _tool: &str,
        _args: &serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        Ok(None)
    }

    /// Called after a tool is executed
    async fn on_tool_after(&self, _tool: &str, _result: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginConfig {
    /// Plugin-specific settings
    #[serde(flatten)]
    pub settings: HashMap<String, serde_json::Value>,
}

/// Plugin registry for managing loaded plugins
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn PluginHooks>>,
    configs: HashMap<String, PluginConfig>,
}

impl PluginRegistry {
    /// Create a new empty registry
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            configs: HashMap::new(),
        }
    }

    /// Register a plugin
    pub fn register(&mut self, name: impl Into<String>, plugin: Box<dyn PluginHooks>) {
        let name = name.into();
        tracing::info!(plugin = %name, "registered plugin");
        self.plugins.insert(name, plugin);
    }

    /// Unregister a plugin
    pub fn unregister(&mut self, name: &str) -> Option<Box<dyn PluginHooks>> {
        tracing::info!(plugin = %name, "unregistered plugin");
        self.plugins.remove(name)
    }

    /// Set configuration for a plugin
    pub fn set_config(&mut self, name: impl Into<String>, config: PluginConfig) {
        self.configs.insert(name.into(), config);
    }

    /// Get all registered plugin names
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.plugins.keys().map(String::as_str).collect()
    }

    /// Get all tools from all plugins
    #[must_use]
    pub fn all_tools(&self) -> Vec<(String, ToolDefinition)> {
        let mut tools = Vec::new();
        for (plugin_name, plugin) in &self.plugins {
            for tool in plugin.tools() {
                // Use :: as delimiter to avoid conflicts with underscores in names
                let qualified_name = format!("{plugin_name}::{}", tool.name);
                tools.push((qualified_name, tool));
            }
        }
        tools
    }

    /// Execute a tool by qualified name
    ///
    /// # Errors
    ///
    /// Returns error if tool not found or execution fails
    pub async fn execute_tool(
        &self,
        qualified_name: &str,
        args: serde_json::Value,
    ) -> anyhow::Result<ToolResult> {
        let parts: Vec<&str> = qualified_name.splitn(2, "::").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid tool name format: {qualified_name}");
        }

        let plugin_name = parts[0];
        let tool_name = parts[1];

        let plugin = self
            .plugins
            .get(plugin_name)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {plugin_name}"))?;

        plugin.execute_tool(tool_name, args).await
    }

    /// Load plugins on startup
    ///
    /// # Errors
    ///
    /// Returns error if plugin loading fails
    pub async fn load_all(&self) -> anyhow::Result<()> {
        for (name, plugin) in &self.plugins {
            if let Err(e) = plugin.on_load().await {
                tracing::error!(plugin = %name, error = %e, "failed to load plugin");
            }
        }
        Ok(())
    }

    /// Unload all plugins
    ///
    /// # Errors
    ///
    /// Returns error if plugin unloading fails
    pub async fn unload_all(&self) -> anyhow::Result<()> {
        for (name, plugin) in &self.plugins {
            if let Err(e) = plugin.on_unload().await {
                tracing::error!(plugin = %name, error = %e, "failed to unload plugin");
            }
        }
        Ok(())
    }

    /// Trigger message before hooks
    ///
    /// # Errors
    ///
    /// Returns error if hook fails
    pub async fn trigger_message_before(&self, message: &str) -> anyhow::Result<String> {
        let mut result = message.to_string();
        for plugin in self.plugins.values() {
            if let Some(modified) = plugin.on_message_before(&result).await? {
                result = modified;
            }
        }
        Ok(result)
    }

    /// Trigger message after hooks
    ///
    /// # Errors
    ///
    /// Returns error if hook fails
    pub async fn trigger_message_after(&self, response: &str) -> anyhow::Result<()> {
        for plugin in self.plugins.values() {
            plugin.on_message_after(response).await?;
        }
        Ok(())
    }

    /// Trigger tool before hooks
    ///
    /// # Errors
    ///
    /// Returns error if hook fails
    pub async fn trigger_tool_before(
        &self,
        tool: &str,
        args: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let mut result = args.clone();
        for plugin in self.plugins.values() {
            if let Some(modified) = plugin.on_tool_before(tool, &result).await? {
                result = modified;
            }
        }
        Ok(result)
    }

    /// Trigger tool after hooks
    ///
    /// # Errors
    ///
    /// Returns error if hook fails
    pub async fn trigger_tool_after(&self, tool: &str, result: &str) -> anyhow::Result<()> {
        for plugin in self.plugins.values() {
            plugin.on_tool_after(tool, result).await?;
        }
        Ok(())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin loader for discovering and loading plugins
pub struct PluginLoader {
    /// Directory containing plugins
    plugin_dir: PathBuf,
}

impl PluginLoader {
    /// Create a new plugin loader
    ///
    /// # Errors
    ///
    /// Returns error if plugin directory cannot be determined
    pub fn new() -> anyhow::Result<Self> {
        let plugin_dir = crate::config::Config::data_dir()?.join("plugins");

        Ok(Self { plugin_dir })
    }

    /// Get the plugin directory path
    #[must_use]
    pub const fn plugin_dir(&self) -> &PathBuf {
        &self.plugin_dir
    }

    /// List available plugins
    ///
    /// # Errors
    ///
    /// Returns error if directory reading fails
    pub fn list_available(&self) -> anyhow::Result<Vec<PluginInfo>> {
        let mut plugins = Vec::new();

        if !self.plugin_dir.exists() {
            return Ok(plugins);
        }

        for entry in std::fs::read_dir(&self.plugin_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Look for plugin.json or plugin.toml
            let manifest_path = path.join("plugin.json");
            if manifest_path.exists() {
                let content = std::fs::read_to_string(&manifest_path)?;
                if let Ok(info) = serde_json::from_str::<PluginInfo>(&content) {
                    plugins.push(info);
                }
            }
        }

        Ok(plugins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin;

    #[async_trait]
    impl PluginHooks for TestPlugin {
        fn tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "test_tool".to_string(),
                description: "A test tool".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            }]
        }

        async fn execute_tool(
            &self,
            name: &str,
            _args: serde_json::Value,
        ) -> anyhow::Result<ToolResult> {
            if name == "test_tool" {
                Ok(ToolResult {
                    output: "test output".to_string(),
                    is_error: false,
                })
            } else {
                anyhow::bail!("Tool not found")
            }
        }
    }

    #[test]
    fn registry_registers_plugin() {
        let mut registry = PluginRegistry::new();
        registry.register("test", Box::new(TestPlugin));
        assert!(registry.names().contains(&"test"));
    }

    #[test]
    fn registry_collects_tools() {
        let mut registry = PluginRegistry::new();
        registry.register("test", Box::new(TestPlugin));
        let tools = registry.all_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, "test::test_tool");
    }

    #[tokio::test]
    async fn registry_executes_tool() {
        let mut registry = PluginRegistry::new();
        registry.register("test", Box::new(TestPlugin));
        let result = registry
            .execute_tool("test::test_tool", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result.output, "test output");
        assert!(!result.is_error);
    }
}
