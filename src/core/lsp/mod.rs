//! LSP (Language Server Protocol) integration
//!
//! Provides code intelligence via language servers

mod client;
mod protocol;
mod server;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;

pub use client::LspClient;
pub use protocol::{
    Diagnostic, DiagnosticSeverity, DocumentSymbol, Hover, HoverContents, Location, MarkedString,
    Position, Range, SymbolKind,
};
pub use server::{LspServer, LspServerConfig};

/// LSP operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspOperation {
    /// Get hover information at position
    Hover,
    /// Go to definition
    GoToDefinition,
    /// Go to implementation
    GoToImplementation,
    /// Find references
    FindReferences,
    /// Get document symbols
    DocumentSymbol,
    /// Search workspace symbols
    WorkspaceSymbol,
}

impl std::fmt::Display for LspOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hover => write!(f, "hover"),
            Self::GoToDefinition => write!(f, "goToDefinition"),
            Self::GoToImplementation => write!(f, "goToImplementation"),
            Self::FindReferences => write!(f, "findReferences"),
            Self::DocumentSymbol => write!(f, "documentSymbol"),
            Self::WorkspaceSymbol => write!(f, "workspaceSymbol"),
        }
    }
}

impl std::str::FromStr for LspOperation {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hover" => Ok(Self::Hover),
            "goToDefinition" | "definition" => Ok(Self::GoToDefinition),
            "goToImplementation" | "implementation" => Ok(Self::GoToImplementation),
            "findReferences" | "references" => Ok(Self::FindReferences),
            "documentSymbol" | "symbols" => Ok(Self::DocumentSymbol),
            "workspaceSymbol" | "workspace_symbol" => Ok(Self::WorkspaceSymbol),
            _ => anyhow::bail!("unknown LSP operation: {s}"),
        }
    }
}

/// Result of an LSP operation
#[derive(Debug, Clone)]
pub enum LspResult {
    /// Hover information
    Hover(Option<Hover>),
    /// Definition locations
    Locations(Vec<Location>),
    /// Document symbols
    DocumentSymbols(Vec<DocumentSymbol>),
    /// Workspace symbols (name, kind, location)
    WorkspaceSymbols(Vec<(String, SymbolKind, Location)>),
}

/// Type alias for client cache
type ClientCache = Arc<RwLock<HashMap<(String, PathBuf), Arc<LspClient>>>>;

/// Manages LSP clients for different language servers
#[derive(Debug)]
pub struct LspManager {
    /// Active clients by (`server_id`, `project_root`)
    clients: ClientCache,
    /// Available server configurations
    servers: Vec<LspServer>,
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LspManager {
    /// Create a new LSP manager with default servers
    #[must_use]
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            servers: server::default_servers(),
        }
    }

    /// Get servers that handle a given file extension
    #[must_use]
    pub fn servers_for_file(&self, path: &Path) -> Vec<&LspServer> {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        self.servers
            .iter()
            .filter(|s| s.extensions.iter().any(|e| e == extension))
            .collect()
    }

    /// Get or create a client for a file
    ///
    /// # Errors
    ///
    /// Returns error if server cannot be started
    pub async fn get_client(&self, path: &Path) -> anyhow::Result<Arc<LspClient>> {
        let servers = self.servers_for_file(path);
        if servers.is_empty() {
            anyhow::bail!("no language server for file: {}", path.display());
        }

        // Use first matching server
        let server = servers[0];

        // Find project root
        let root = server.find_root(path)?;

        let key = (server.id.clone(), root.clone());

        // Check if client already exists
        {
            let clients = self.clients.read();
            if let Some(client) = clients.get(&key) {
                return Ok(Arc::clone(client));
            }
        }

        // Create new client
        let client = LspClient::new(server, &root).await?;
        let client = Arc::new(client);

        // Store client
        {
            let mut clients = self.clients.write();
            clients.insert(key, Arc::clone(&client));
        }

        Ok(client)
    }

    /// Execute an LSP operation
    ///
    /// # Errors
    ///
    /// Returns error if operation fails
    pub async fn execute(
        &self,
        operation: LspOperation,
        file_path: &Path,
        line: u32,
        character: u32,
        query: Option<&str>,
    ) -> anyhow::Result<LspResult> {
        let client = self.get_client(file_path).await?;

        // Ensure file is open in the server
        client.open_file(file_path).await?;

        let position = Position { line, character };
        let uri = path_to_uri(file_path);

        match operation {
            LspOperation::Hover => {
                let hover = client.hover(&uri, position).await?;
                Ok(LspResult::Hover(hover))
            }
            LspOperation::GoToDefinition => {
                let locations = client.go_to_definition(&uri, position).await?;
                Ok(LspResult::Locations(locations))
            }
            LspOperation::GoToImplementation => {
                let locations = client.go_to_implementation(&uri, position).await?;
                Ok(LspResult::Locations(locations))
            }
            LspOperation::FindReferences => {
                let locations = client.find_references(&uri, position).await?;
                Ok(LspResult::Locations(locations))
            }
            LspOperation::DocumentSymbol => {
                let symbols = client.document_symbols(&uri).await?;
                Ok(LspResult::DocumentSymbols(symbols))
            }
            LspOperation::WorkspaceSymbol => {
                let query = query.unwrap_or("");
                let symbols = client.workspace_symbols(query).await?;
                Ok(LspResult::WorkspaceSymbols(symbols))
            }
        }
    }

    /// Shutdown all clients
    pub async fn shutdown(&self) {
        let clients: Vec<_> = {
            let mut guard = self.clients.write();
            guard.drain().map(|(_, c)| c).collect()
        };

        for client in clients {
            if let Err(e) = client.shutdown().await {
                tracing::warn!(error = %e, "failed to shutdown LSP client");
            }
        }
    }
}

/// Convert a file path to a URI
#[must_use]
pub fn path_to_uri(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", path.display())
}

/// Convert a URI to a file path
#[must_use]
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_from_str() {
        assert_eq!(
            "hover".parse::<LspOperation>().unwrap(),
            LspOperation::Hover
        );
        assert_eq!(
            "goToDefinition".parse::<LspOperation>().unwrap(),
            LspOperation::GoToDefinition
        );
        assert_eq!(
            "definition".parse::<LspOperation>().unwrap(),
            LspOperation::GoToDefinition
        );
    }

    #[test]
    fn operation_display() {
        assert_eq!(LspOperation::Hover.to_string(), "hover");
        assert_eq!(LspOperation::GoToDefinition.to_string(), "goToDefinition");
    }

    #[test]
    fn path_to_uri_format() {
        let path = PathBuf::from("/home/user/project/src/main.rs");
        let uri = path_to_uri(&path);
        assert!(uri.starts_with("file://"));
        assert!(uri.contains("main.rs"));
    }

    #[test]
    fn uri_to_path_extracts() {
        let uri = "file:///home/user/project/src/main.rs";
        let path = uri_to_path(uri).unwrap();
        assert_eq!(path, PathBuf::from("/home/user/project/src/main.rs"));
    }

    #[test]
    fn manager_finds_servers() {
        let manager = LspManager::new();
        let rs_servers = manager.servers_for_file(Path::new("src/main.rs"));
        assert!(!rs_servers.is_empty());
    }
}
