//! MCP server connection management

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::config::McpServerConfig;
use super::protocol::{
    InitializeResult, McpContent, McpRequest, McpResponse, McpTool, McpToolResult,
};

/// MCP server connection status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    /// Server is connected and ready
    Connected,
    /// Server is disabled
    Disabled,
    /// Server failed to connect
    Failed(String),
}

/// MCP server connection
pub struct McpServer {
    name: String,
    config: McpServerConfig,
    process: Option<Child>,
    request_id: AtomicU64,
    status: ServerStatus,
    capabilities: Option<InitializeResult>,
    tools: Vec<McpTool>,
    stdin: Option<Arc<Mutex<std::process::ChildStdin>>>,
    stdout_reader: Option<Arc<Mutex<BufReader<std::process::ChildStdout>>>>,
}

impl McpServer {
    /// Create a new server connection
    pub fn new(name: impl Into<String>, config: McpServerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            process: None,
            request_id: AtomicU64::new(1),
            status: ServerStatus::Disabled,
            capabilities: None,
            tools: Vec::new(),
            stdin: None,
            stdout_reader: None,
        }
    }

    /// Get the server name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the server status
    #[must_use]
    pub const fn status(&self) -> &ServerStatus {
        &self.status
    }

    /// Check if connected
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.status == ServerStatus::Connected
    }

    /// Get available tools
    #[must_use]
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// Connect to the server
    ///
    /// # Errors
    ///
    /// Returns error if connection fails
    pub fn connect(&mut self) -> anyhow::Result<()> {
        if !self.config.enabled {
            self.status = ServerStatus::Disabled;
            return Ok(());
        }

        match self.config.server_type {
            super::config::ServerType::Local => self.connect_local(),
            super::config::ServerType::Remote => {
                self.status = ServerStatus::Failed("Remote servers not yet supported".to_string());
                Ok(())
            }
        }
    }

    fn connect_local(&mut self) -> anyhow::Result<()> {
        if self.config.command.is_empty() {
            self.status = ServerStatus::Failed("No command specified".to_string());
            return Ok(());
        }

        let cmd = &self.config.command[0];
        let args = &self.config.command[1..];

        tracing::info!(server = %self.name, cmd = %cmd, "starting MCP server");

        let mut command = Command::new(cmd);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add environment variables
        for (key, value) in &self.config.environment {
            command.env(key, value);
        }

        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.status = ServerStatus::Failed(format!("Failed to spawn: {e}"));
                return Ok(());
            }
        };

        let stdin = child.stdin.take().expect("stdin should be captured");
        let stdout = child.stdout.take().expect("stdout should be captured");

        self.stdin = Some(Arc::new(Mutex::new(stdin)));
        self.stdout_reader = Some(Arc::new(Mutex::new(BufReader::new(stdout))));
        self.process = Some(child);

        // Initialize the connection
        if let Err(e) = self.initialize() {
            self.status = ServerStatus::Failed(format!("Initialize failed: {e}"));
            self.disconnect();
            return Ok(());
        }

        // Fetch tools
        if let Err(e) = self.fetch_tools() {
            tracing::warn!(server = %self.name, error = %e, "failed to fetch tools");
        }

        self.status = ServerStatus::Connected;
        tracing::info!(
            server = %self.name,
            tools = self.tools.len(),
            "MCP server connected"
        );

        Ok(())
    }

    fn initialize(&mut self) -> anyhow::Result<()> {
        let id = self.next_id();
        let request = McpRequest::initialize(id, "omni", env!("CARGO_PKG_VERSION"));

        let response = self.send_request(&request)?;
        let result: InitializeResult = serde_json::from_value(response)?;

        self.capabilities = Some(result);

        // Send initialized notification
        self.send_notification(&McpRequest::initialized())?;

        Ok(())
    }

    fn fetch_tools(&mut self) -> anyhow::Result<()> {
        let id = self.next_id();
        let request = McpRequest::list_tools(id);

        let response = self.send_request(&request)?;

        #[derive(serde::Deserialize)]
        struct ToolsResult {
            tools: Vec<McpTool>,
        }

        let result: ToolsResult = serde_json::from_value(response)?;
        self.tools = result.tools;

        Ok(())
    }

    /// Call a tool on this server
    ///
    /// # Errors
    ///
    /// Returns error if the tool call fails
    pub fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        if !self.is_connected() {
            anyhow::bail!("Server not connected");
        }

        let id = self.next_id();
        let request = McpRequest::call_tool(id, name, arguments);

        let response = self.send_request(&request)?;
        let result: McpToolResult = serde_json::from_value(response)?;

        // Convert content to string
        let output = result
            .content
            .iter()
            .map(McpContent::to_text)
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_error {
            anyhow::bail!("Tool error: {output}");
        }

        Ok(output)
    }

    fn send_request(&self, request: &McpRequest) -> anyhow::Result<serde_json::Value> {
        let json = serde_json::to_string(request)?;
        self.send_line(&json)?;

        // Read response
        let response = self.read_response()?;

        if let Some(error) = response.error {
            anyhow::bail!("{error}");
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    fn send_notification(&self, json: &str) -> anyhow::Result<()> {
        self.send_line(json)
    }

    fn send_line(&self, line: &str) -> anyhow::Result<()> {
        let stdin = self
            .stdin
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No stdin"))?;
        let mut stdin = stdin.lock().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

        writeln!(stdin, "{line}")?;
        stdin.flush()?;

        Ok(())
    }

    fn read_response(&self) -> anyhow::Result<McpResponse> {
        let reader = self
            .stdout_reader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No stdout"))?;
        let mut reader = reader
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

        let mut line = String::new();
        reader.read_line(&mut line)?;

        let response: McpResponse = serde_json::from_str(&line)?;
        Ok(response)
    }

    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Disconnect from the server
    pub fn disconnect(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        self.stdin = None;
        self.stdout_reader = None;
        self.tools.clear();
        self.status = ServerStatus::Disabled;
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        self.disconnect();
    }
}
