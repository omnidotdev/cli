//! Model Context Protocol (MCP) client implementation
//!
//! MCP enables extensible tools via external servers that communicate
//! over stdio or HTTP

mod client;
mod config;
mod protocol;
mod server;

pub use client::McpClient;
pub use config::{McpConfig, McpServerConfig, ServerType};
pub use protocol::{McpRequest, McpResponse, McpTool};
pub use server::McpServer;
