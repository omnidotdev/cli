//! MCP JSON-RPC protocol types

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request
#[derive(Debug, Clone, Serialize)]
pub struct McpRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpRequest {
    /// Create a new request
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }

    /// Create an initialize request
    #[must_use]
    pub fn initialize(id: u64, client_name: &str, client_version: &str) -> Self {
        Self::new(
            id,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "roots": { "listChanged": true },
                    "sampling": {}
                },
                "clientInfo": {
                    "name": client_name,
                    "version": client_version
                }
            })),
        )
    }

    /// Create a tools/list request
    #[must_use]
    pub fn list_tools(id: u64) -> Self {
        Self::new(id, "tools/list", None)
    }

    /// Create a tools/call request
    #[must_use]
    pub fn call_tool(id: u64, name: &str, arguments: Value) -> Self {
        Self::new(
            id,
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            })),
        )
    }

    /// Create a notifications/initialized notification
    #[must_use]
    pub fn initialized() -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        })
        .to_string()
    }
}

/// JSON-RPC response
#[derive(Debug, Clone, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<McpError>,
}

impl McpResponse {
    /// Check if the response is successful
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.error.is_none()
    }

    /// Get the result or error
    pub fn into_result(self) -> Result<Value, McpError> {
        if let Some(error) = self.error {
            Err(error)
        } else {
            Ok(self.result.unwrap_or(Value::Null))
        }
    }
}

/// JSON-RPC error
#[derive(Debug, Clone, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MCP error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for McpError {}

/// MCP tool definition
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    /// Tool name
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema for input parameters
    pub input_schema: Value,
}

/// MCP tool call result
#[derive(Debug, Clone, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
}

/// MCP content block
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpContent {
    Text {
        text: String,
    },
    Image {
        data: String,
        mime_type: String,
    },
    Resource {
        uri: String,
        mime_type: Option<String>,
    },
}

impl McpContent {
    /// Convert to string representation
    #[must_use]
    pub fn to_text(&self) -> String {
        match self {
            Self::Text { text } => text.clone(),
            Self::Image { mime_type, .. } => format!("[Image: {mime_type}]"),
            Self::Resource { uri, .. } => format!("[Resource: {uri}]"),
        }
    }
}

/// Server capabilities from initialize response
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    #[serde(default)]
    pub prompts: Option<PromptsCapability>,
    #[serde(default)]
    pub resources: Option<ResourcesCapability>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PromptsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResourcesCapability {
    #[serde(default)]
    pub subscribe: bool,
    #[serde(default)]
    pub list_changed: bool,
}

/// Initialize result
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}
