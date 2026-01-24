//! MCP configuration types

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// MCP configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    /// MCP servers
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
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

impl McpServerConfig {
    /// Create a new local server config
    #[must_use]
    pub fn local(command: Vec<String>) -> Self {
        Self {
            server_type: ServerType::Local,
            enabled: true,
            command,
            url: None,
            environment: HashMap::new(),
            timeout: None,
        }
    }

    /// Create a new remote server config
    #[must_use]
    pub fn remote(url: impl Into<String>) -> Self {
        Self {
            server_type: ServerType::Remote,
            enabled: true,
            command: Vec::new(),
            url: Some(url.into()),
            environment: HashMap::new(),
            timeout: None,
        }
    }

    /// Set environment variables
    #[must_use]
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.environment = env;
        self
    }

    /// Set timeout
    #[must_use]
    pub const fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout = Some(timeout_ms);
        self
    }
}
