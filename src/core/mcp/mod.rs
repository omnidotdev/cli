//! Model Context Protocol (MCP) client implementation
//!
//! Wraps agent-core's MCP module, providing CLI-specific config parsing
//! and a sync-compatible interface for the tool registry

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// Re-export agent-core MCP types
pub use agent_core::mcp::{
    McpServerConfig as AgentMcpServerConfig, McpServerManager, McpTool, McpToolResult,
};

/// MCP configuration as read from CLI config files (e.g. `.claude.json`)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    /// MCP servers keyed by name
    #[serde(default)]
    pub servers: HashMap<String, McpServerEntry>,
}

/// A single MCP server entry from the CLI config file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    /// Server type
    #[serde(rename = "type")]
    pub server_type: ServerType,

    /// Whether the server is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Command to run (for local servers)
    #[serde(default)]
    pub command: Vec<String>,

    /// URL (for remote servers)
    #[serde(default)]
    pub url: Option<String>,

    /// Environment variables
    #[serde(default)]
    pub environment: HashMap<String, String>,

    /// Connection timeout in milliseconds
    #[serde(default)]
    pub timeout: Option<u64>,
}

const fn default_enabled() -> bool {
    true
}

/// Server type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerType {
    /// Local server via stdio
    Local,
    /// Remote server via HTTP
    Remote,
}

/// Server connection status (used for CLI display)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    /// Server is connected and ready
    Connected,
    /// Server is disabled
    Disabled,
    /// Server failed to connect
    Failed(String),
}

/// MCP client wrapping agent-core's `McpServerManager`
///
/// Handles CLI config format conversion and provides both sync and async
/// access patterns for the tool registry
pub struct McpClient {
    manager: std::sync::Arc<McpServerManager>,
    /// Track per-server status for display
    statuses: HashMap<String, ServerStatus>,
}

impl McpClient {
    /// Create a new empty MCP client
    #[must_use]
    pub fn new() -> Self {
        Self {
            manager: std::sync::Arc::new(McpServerManager::new()),
            statuses: HashMap::new(),
        }
    }

    /// Get a cloneable reference to the underlying manager
    ///
    /// Useful for calling async methods without holding a sync lock guard
    #[must_use]
    pub fn manager(&self) -> std::sync::Arc<McpServerManager> {
        std::sync::Arc::clone(&self.manager)
    }

    /// Create a client from CLI config and start all enabled servers
    ///
    /// Must be called from within a tokio runtime. Uses `block_in_place`
    /// to safely bridge sync/async
    #[must_use]
    pub fn from_config(config: &McpConfig) -> Self {
        let mut client = Self::new();

        let configs = convert_configs(config);

        if configs.is_empty() {
            return client;
        }

        // Start all servers, bridging async safely
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                client.manager.start_all(&configs).await;
            });
        });

        // Check which servers actually started
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let running = client.manager.server_names().await;
                for (name, entry) in &config.servers {
                    if !entry.enabled {
                        client
                            .statuses
                            .insert(name.clone(), ServerStatus::Disabled);
                    } else if running.contains(name) {
                        client
                            .statuses
                            .insert(name.clone(), ServerStatus::Connected);
                    } else {
                        client.statuses.insert(
                            name.clone(),
                            ServerStatus::Failed("failed to start".to_string()),
                        );
                    }
                }
            });
        });

        client
    }

    /// Connect all enabled servers from config (sync bridge)
    pub fn connect_all(&mut self) {
        // Servers are already started in `from_config`; this is a no-op
        // for compatibility with the old API
    }

    /// Get status of all servers
    #[must_use]
    pub fn status(&self) -> HashMap<String, ServerStatus> {
        self.statuses.clone()
    }

    /// Get all available tools from connected servers (sync bridge)
    ///
    /// Returns `(qualified_name, tool)` pairs where qualified name
    /// is `server_name::tool_name`
    #[must_use]
    pub fn tools(&self) -> Vec<(String, McpTool)> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let tools = self.manager.all_tools().await;
                tools
                    .into_iter()
                    .map(|tool| {
                        // Convert from agent-core's scoped format to CLI's `server::tool` format
                        let qualified_name =
                            format!("{}::{}", tool.server_name, tool.name.rsplit('/').next().unwrap_or(&tool.name));
                        (qualified_name, tool)
                    })
                    .collect()
            })
        })
    }

    /// Call a tool by qualified name (`server::toolname`) — async
    ///
    /// # Errors
    ///
    /// Returns error if the tool call fails
    pub async fn call_tool_async(
        &self,
        qualified_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        // Parse `server::toolname` format
        let (server_name, tool_name) = qualified_name
            .split_once("::")
            .ok_or_else(|| anyhow::anyhow!("Invalid tool name format: {qualified_name}"))?;

        // Call via manager using its scoped format
        let scoped = format!("mcp_{server_name}/{tool_name}");
        let result = self
            .manager
            .call_tool(&scoped, arguments)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if result.is_error {
            anyhow::bail!("Tool error: {}", result.text);
        }

        Ok(result.text)
    }

    /// Call a tool by qualified name — sync bridge
    ///
    /// # Errors
    ///
    /// Returns error if the tool call fails
    pub fn call_tool(
        &mut self,
        qualified_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.call_tool_async(qualified_name, arguments))
        })
    }

    /// Check if any servers are connected
    #[must_use]
    pub fn has_connections(&self) -> bool {
        self.statuses
            .values()
            .any(|s| *s == ServerStatus::Connected)
    }

    /// Get the number of connected servers
    #[must_use]
    pub fn connected_count(&self) -> usize {
        self.statuses
            .values()
            .filter(|s| **s == ServerStatus::Connected)
            .count()
    }

    /// Stop all servers
    pub async fn stop_all(&self) {
        self.manager.stop_all().await;
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Best-effort cleanup — if we're in a runtime, stop servers
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let manager = std::sync::Arc::clone(&self.manager);
            handle.spawn(async move {
                manager.stop_all().await;
            });
        }
    }
}

/// Convert CLI config entries to agent-core configs, filtering disabled/remote
fn convert_configs(config: &McpConfig) -> Vec<AgentMcpServerConfig> {
    config
        .servers
        .iter()
        .filter_map(|(name, entry)| {
            if !entry.enabled {
                return None;
            }

            if entry.server_type != ServerType::Local {
                tracing::warn!(server = %name, "remote MCP servers not yet supported, skipping");
                return None;
            }

            if entry.command.is_empty() {
                tracing::warn!(server = %name, "no command specified, skipping");
                return None;
            }

            let command = entry.command[0].clone();
            let args = entry.command[1..].to_vec();

            Some(AgentMcpServerConfig {
                name: name.clone(),
                command,
                args,
                env: entry.environment.clone(),
            })
        })
        .collect()
}

// Re-export McpServerConfig name for backward compat in config parsing
pub use McpServerEntry as McpServerConfig;
