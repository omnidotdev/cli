//! MCP client for managing multiple servers

use std::collections::HashMap;

use super::config::{McpConfig, McpServerConfig};
use super::protocol::McpTool;
use super::server::{McpServer, ServerStatus};

/// MCP client that manages multiple server connections
pub struct McpClient {
    servers: HashMap<String, McpServer>,
}

impl McpClient {
    /// Create a new MCP client
    #[must_use]
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    /// Create a client from configuration
    #[must_use]
    pub fn from_config(config: &McpConfig) -> Self {
        let mut client = Self::new();

        for (name, server_config) in &config.servers {
            client.add_server(name.clone(), server_config.clone());
        }

        client
    }

    /// Add a server
    pub fn add_server(&mut self, name: String, config: McpServerConfig) {
        let server = McpServer::new(&name, config);
        self.servers.insert(name, server);
    }

    /// Remove a server
    pub fn remove_server(&mut self, name: &str) {
        if let Some(mut server) = self.servers.remove(name) {
            server.disconnect();
        }
    }

    /// Connect to all enabled servers
    pub fn connect_all(&mut self) {
        for server in self.servers.values_mut() {
            if let Err(e) = server.connect() {
                tracing::error!(server = %server.name(), error = %e, "failed to connect");
            }
        }
    }

    /// Connect to a specific server
    ///
    /// # Errors
    ///
    /// Returns error if the server doesn't exist
    pub fn connect(&mut self, name: &str) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {name}"))?;

        server.connect()
    }

    /// Disconnect from a specific server
    pub fn disconnect(&mut self, name: &str) {
        if let Some(server) = self.servers.get_mut(name) {
            server.disconnect();
        }
    }

    /// Disconnect from all servers
    pub fn disconnect_all(&mut self) {
        for server in self.servers.values_mut() {
            server.disconnect();
        }
    }

    /// Get status of all servers
    #[must_use]
    pub fn status(&self) -> HashMap<String, ServerStatus> {
        self.servers
            .iter()
            .map(|(name, server)| (name.clone(), server.status().clone()))
            .collect()
    }

    /// Get all available tools from connected servers
    #[must_use]
    pub fn tools(&self) -> Vec<(String, McpTool)> {
        let mut tools = Vec::new();

        for (server_name, server) in &self.servers {
            if server.is_connected() {
                for tool in server.tools() {
                    // Use :: as delimiter to avoid conflicts with underscores in names
                    let qualified_name = format!("{server_name}::{}", tool.name);
                    tools.push((qualified_name, tool.clone()));
                }
            }
        }

        tools
    }

    /// Call a tool by qualified name (`server::toolname`)
    ///
    /// # Errors
    ///
    /// Returns error if the tool call fails
    pub fn call_tool(
        &mut self,
        qualified_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        // Parse server name and tool name
        let parts: Vec<&str> = qualified_name.splitn(2, "::").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid tool name format: {qualified_name}");
        }

        let server_name = parts[0];
        let tool_name = parts[1];

        let server = self
            .servers
            .get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {server_name}"))?;

        server.call_tool(tool_name, arguments)
    }

    /// Check if any servers are connected
    #[must_use]
    pub fn has_connections(&self) -> bool {
        self.servers.values().any(McpServer::is_connected)
    }

    /// Get the number of connected servers
    #[must_use]
    pub fn connected_count(&self) -> usize {
        self.servers.values().filter(|s| s.is_connected()).count()
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.disconnect_all();
    }
}
