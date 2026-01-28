//! LSP client implementation
//!
//! JSON-RPC client over stdio for language servers

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

/// Type alias for pending request map
type PendingRequests = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>>;

use super::protocol::{
    ClientCapabilities, DocumentSymbol, Hover, HoverClientCapabilities, InitializeParams,
    InitializeResult, Location, Position, ReferenceContext, ReferenceParams, SymbolInformation,
    SymbolKind, TextDocumentClientCapabilities, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, WorkspaceFolder,
};
use super::server::LspServer;
use super::path_to_uri;

/// JSON-RPC request
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: i64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC notification
#[derive(Debug, Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<i64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// LSP client for a language server
pub struct LspClient {
    /// Server info
    #[allow(dead_code)]
    server: LspServer,
    /// Project root
    root: PathBuf,
    /// Child process
    process: Mutex<Child>,
    /// Stdin for writing
    stdin: Mutex<ChildStdin>,
    /// Request ID counter
    next_id: AtomicI64,
    /// Pending requests
    pending: PendingRequests,
    /// Opened files (uri -> version)
    opened_files: Mutex<HashMap<String, i32>>,
    /// Reader thread handle
    _reader_handle: std::thread::JoinHandle<()>,
}

impl std::fmt::Debug for LspClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspClient")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl LspClient {
    /// Create a new LSP client
    ///
    /// # Errors
    ///
    /// Returns error if server cannot be started or initialized
    pub async fn new(server: &LspServer, root: &Path) -> anyhow::Result<Self> {
        if server.command.is_empty() {
            anyhow::bail!("server has no command");
        }

        // Start server process
        let mut cmd = Command::new(&server.command[0]);
        if server.command.len() > 1 {
            cmd.args(&server.command[1..]);
        }

        let mut process = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .current_dir(root)
            .spawn()?;

        let stdin = process.stdin.take().ok_or_else(|| {
            anyhow::anyhow!("failed to open stdin")
        })?;

        let stdout = process.stdout.take().ok_or_else(|| {
            anyhow::anyhow!("failed to open stdout")
        })?;

        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = Arc::clone(&pending);

        // Start reader thread
        let reader_handle = std::thread::spawn(move || {
            read_responses(stdout, pending_clone);
        });

        let client = Self {
            server: server.clone(),
            root: root.to_path_buf(),
            process: Mutex::new(process),
            stdin: Mutex::new(stdin),
            next_id: AtomicI64::new(1),
            pending,
            opened_files: Mutex::new(HashMap::new()),
            _reader_handle: reader_handle,
        };

        // Initialize
        client.initialize().await?;

        Ok(client)
    }

    /// Initialize the language server
    async fn initialize(&self) -> anyhow::Result<()> {
        let root_uri = path_to_uri(&self.root);

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_path: Some(self.root.display().to_string()),
            root_uri: Some(root_uri.clone()),
            capabilities: ClientCapabilities {
                text_document: Some(TextDocumentClientCapabilities {
                    hover: Some(HoverClientCapabilities {
                        dynamic_registration: Some(false),
                        content_format: Some(vec!["markdown".to_string(), "plaintext".to_string()]),
                    }),
                }),
            },
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: self
                    .root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
                    .to_string(),
            }]),
        };

        let _result: InitializeResult = self.request("initialize", Some(params)).await?;

        // Send initialized notification
        self.notify("initialized", Some(serde_json::json!({})))?;

        Ok(())
    }

    /// Send a request and wait for response
    async fn request<P: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Option<P>,
    ) -> anyhow::Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params: params.map(|p| serde_json::to_value(p)).transpose()?,
        };

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending.lock();
            pending.insert(id, tx);
        }

        // Write request
        self.write_message(&serde_json::to_string(&request)?)?;

        // Wait for response with timeout
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), rx).await;

        match result {
            Ok(Ok(Ok(value))) => {
                let result: R = serde_json::from_value(value)?;
                Ok(result)
            }
            Ok(Ok(Err(e))) => anyhow::bail!("LSP error: {e}"),
            Ok(Err(_)) => anyhow::bail!("response channel closed"),
            Err(_) => anyhow::bail!("LSP request timed out"),
        }
    }

    /// Send a notification (no response expected)
    fn notify<P: Serialize>(&self, method: &str, params: Option<P>) -> anyhow::Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params: params.map(|p| serde_json::to_value(p)).transpose()?,
        };

        self.write_message(&serde_json::to_string(&notification)?)
    }

    /// Write a JSON-RPC message
    fn write_message(&self, content: &str) -> anyhow::Result<()> {
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        let mut stdin = self.stdin.lock();
        stdin.write_all(message.as_bytes())?;
        stdin.flush()?;

        Ok(())
    }

    /// Open a file in the language server
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be read
    pub async fn open_file(&self, path: &Path) -> anyhow::Result<()> {
        let uri = path_to_uri(path);

        // Check if already open
        {
            let opened = self.opened_files.lock();
            if opened.contains_key(&uri) {
                return Ok(());
            }
        }

        let text = tokio::fs::read_to_string(path).await?;

        let params = TextDocumentItem {
            uri: uri.clone(),
            language_id: self.server.language_id.clone(),
            version: 1,
            text,
        };

        self.notify("textDocument/didOpen", Some(serde_json::json!({
            "textDocument": params
        })))?;

        {
            let mut opened = self.opened_files.lock();
            opened.insert(uri, 1);
        }

        // Give server time to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Ok(())
    }

    /// Get hover information
    ///
    /// # Errors
    ///
    /// Returns error if request fails
    pub async fn hover(&self, uri: &str, position: Position) -> anyhow::Result<Option<Hover>> {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: uri.to_string(),
            },
            position,
        };

        let result: Option<Hover> = self.request("textDocument/hover", Some(params)).await?;
        Ok(result)
    }

    /// Go to definition
    ///
    /// # Errors
    ///
    /// Returns error if request fails
    pub async fn go_to_definition(
        &self,
        uri: &str,
        position: Position,
    ) -> anyhow::Result<Vec<Location>> {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: uri.to_string(),
            },
            position,
        };

        let result: LocationResponse =
            self.request("textDocument/definition", Some(params)).await?;
        Ok(result.into_locations())
    }

    /// Go to implementation
    ///
    /// # Errors
    ///
    /// Returns error if request fails
    pub async fn go_to_implementation(
        &self,
        uri: &str,
        position: Position,
    ) -> anyhow::Result<Vec<Location>> {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: uri.to_string(),
            },
            position,
        };

        let result: LocationResponse = self
            .request("textDocument/implementation", Some(params))
            .await?;
        Ok(result.into_locations())
    }

    /// Find references
    ///
    /// # Errors
    ///
    /// Returns error if request fails
    pub async fn find_references(
        &self,
        uri: &str,
        position: Position,
    ) -> anyhow::Result<Vec<Location>> {
        let params = ReferenceParams {
            text_document: TextDocumentIdentifier {
                uri: uri.to_string(),
            },
            position,
            context: ReferenceContext {
                include_declaration: true,
            },
        };

        let result: Option<Vec<Location>> =
            self.request("textDocument/references", Some(params)).await?;
        Ok(result.unwrap_or_default())
    }

    /// Get document symbols
    ///
    /// # Errors
    ///
    /// Returns error if request fails
    pub async fn document_symbols(&self, uri: &str) -> anyhow::Result<Vec<DocumentSymbol>> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri
            }
        });

        let result: DocumentSymbolResponse =
            self.request("textDocument/documentSymbol", Some(params)).await?;
        Ok(result.into_document_symbols())
    }

    /// Search workspace symbols
    ///
    /// # Errors
    ///
    /// Returns error if request fails
    pub async fn workspace_symbols(
        &self,
        query: &str,
    ) -> anyhow::Result<Vec<(String, SymbolKind, Location)>> {
        let params = serde_json::json!({
            "query": query
        });

        let result: Option<Vec<SymbolInformation>> =
            self.request("workspace/symbol", Some(params)).await?;

        Ok(result
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.name, s.kind, s.location))
            .collect())
    }

    /// Shutdown the language server
    ///
    /// # Errors
    ///
    /// Returns error if shutdown fails
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        // Send shutdown request
        let _: Option<()> = self.request("shutdown", None::<()>).await.ok();

        // Send exit notification
        self.notify("exit", None::<()>)?;

        // Kill process if still running
        let mut process = self.process.lock();
        let _ = process.kill();

        Ok(())
    }
}

/// Response that can be a single location or array
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LocationResponse {
    Single(Location),
    Array(Vec<Location>),
    None,
}

impl LocationResponse {
    fn into_locations(self) -> Vec<Location> {
        match self {
            Self::Single(loc) => vec![loc],
            Self::Array(locs) => locs,
            Self::None => vec![],
        }
    }
}

/// Response that can be `DocumentSymbol` or `SymbolInformation`
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DocumentSymbolResponse {
    DocumentSymbols(Vec<DocumentSymbol>),
    SymbolInformation(Vec<SymbolInformation>),
    None,
}

impl DocumentSymbolResponse {
    fn into_document_symbols(self) -> Vec<DocumentSymbol> {
        match self {
            Self::DocumentSymbols(symbols) => symbols,
            Self::SymbolInformation(infos) => {
                // Convert SymbolInformation to DocumentSymbol
                infos
                    .into_iter()
                    .map(|info| DocumentSymbol {
                        name: info.name,
                        detail: info.container_name,
                        kind: info.kind,
                        range: info.location.range,
                        selection_range: info.location.range,
                        children: vec![],
                    })
                    .collect()
            }
            Self::None => vec![],
        }
    }
}

/// Read responses from stdout in a separate thread
fn read_responses(stdout: ChildStdout, pending: PendingRequests) {
    let mut reader = BufReader::new(stdout);
    let mut headers = String::new();

    loop {
        headers.clear();

        // Read headers
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return; // EOF
            }

            if line == "\r\n" {
                break;
            }

            if let Some(len) = line.strip_prefix("Content-Length: ") {
                content_length = len.trim().parse().ok();
            }
        }

        let Some(len) = content_length else {
            continue;
        };

        // Read content
        let mut content = vec![0u8; len];
        if std::io::Read::read_exact(&mut reader, &mut content).is_err() {
            return;
        }

        let Ok(content) = String::from_utf8(content) else {
            continue;
        };

        // Parse response
        let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&content) else {
            continue;
        };

        // Handle response
        if let Some(id) = response.id {
            let mut pending = pending.lock();
            if let Some(tx) = pending.remove(&id) {
                let result = if let Some(error) = response.error {
                    Err(format!("{}: {}", error.code, error.message))
                } else {
                    Ok(response.result.unwrap_or(Value::Null))
                };
                let _ = tx.send(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_response_single() {
        let json = r#"{"uri": "file:///test.rs", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 5}}}"#;
        let response: LocationResponse = serde_json::from_str(json).unwrap();
        let locations = response.into_locations();
        assert_eq!(locations.len(), 1);
    }

    #[test]
    fn location_response_array() {
        let json = r#"[{"uri": "file:///a.rs", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 5}}}, {"uri": "file:///b.rs", "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 5}}}]"#;
        let response: LocationResponse = serde_json::from_str(json).unwrap();
        let locations = response.into_locations();
        assert_eq!(locations.len(), 2);
    }
}
