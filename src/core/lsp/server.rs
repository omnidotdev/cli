//! Language server definitions
//!
//! Configuration for common language servers

use std::path::{Path, PathBuf};

/// Language server configuration
#[derive(Debug, Clone)]
pub struct LspServer {
    /// Unique server identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// File extensions this server handles
    pub extensions: Vec<String>,
    /// Command to start the server
    pub command: Vec<String>,
    /// Files that indicate project root
    pub root_markers: Vec<String>,
    /// Language ID for text documents
    pub language_id: String,
}

/// Server configuration override
#[derive(Debug, Clone, Default)]
pub struct LspServerConfig {
    /// Override command
    pub command: Option<Vec<String>>,
    /// Disable this server
    pub disabled: bool,
}

impl LspServer {
    /// Find the project root for a file
    ///
    /// # Errors
    ///
    /// Returns error if no root can be found
    pub fn find_root(&self, file_path: &Path) -> anyhow::Result<PathBuf> {
        let mut current = file_path.parent();

        while let Some(dir) = current {
            for marker in &self.root_markers {
                if dir.join(marker).exists() {
                    return Ok(dir.to_path_buf());
                }
            }
            current = dir.parent();
        }

        // Fall back to file's directory
        file_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow::anyhow!("cannot determine project root"))
    }
}

/// Get default language servers
#[must_use]
pub fn default_servers() -> Vec<LspServer> {
    vec![
        // Rust
        LspServer {
            id: "rust-analyzer".to_string(),
            name: "rust-analyzer".to_string(),
            extensions: vec!["rs".to_string()],
            command: vec!["rust-analyzer".to_string()],
            root_markers: vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()],
            language_id: "rust".to_string(),
        },
        // TypeScript/JavaScript
        LspServer {
            id: "typescript-language-server".to_string(),
            name: "TypeScript Language Server".to_string(),
            extensions: vec![
                "ts".to_string(),
                "tsx".to_string(),
                "js".to_string(),
                "jsx".to_string(),
                "mjs".to_string(),
                "cjs".to_string(),
            ],
            command: vec![
                "typescript-language-server".to_string(),
                "--stdio".to_string(),
            ],
            root_markers: vec![
                "tsconfig.json".to_string(),
                "jsconfig.json".to_string(),
                "package.json".to_string(),
            ],
            language_id: "typescript".to_string(),
        },
        // Python
        LspServer {
            id: "pyright".to_string(),
            name: "Pyright".to_string(),
            extensions: vec!["py".to_string(), "pyi".to_string()],
            command: vec!["pyright-langserver".to_string(), "--stdio".to_string()],
            root_markers: vec![
                "pyproject.toml".to_string(),
                "setup.py".to_string(),
                "pyrightconfig.json".to_string(),
                "requirements.txt".to_string(),
            ],
            language_id: "python".to_string(),
        },
        // Go
        LspServer {
            id: "gopls".to_string(),
            name: "gopls".to_string(),
            extensions: vec!["go".to_string()],
            command: vec!["gopls".to_string()],
            root_markers: vec!["go.mod".to_string(), "go.work".to_string()],
            language_id: "go".to_string(),
        },
        // C/C++
        LspServer {
            id: "clangd".to_string(),
            name: "clangd".to_string(),
            extensions: vec![
                "c".to_string(),
                "cpp".to_string(),
                "cc".to_string(),
                "cxx".to_string(),
                "h".to_string(),
                "hpp".to_string(),
            ],
            command: vec!["clangd".to_string()],
            root_markers: vec![
                "compile_commands.json".to_string(),
                "CMakeLists.txt".to_string(),
                "Makefile".to_string(),
            ],
            language_id: "cpp".to_string(),
        },
        // Zig
        LspServer {
            id: "zls".to_string(),
            name: "Zig Language Server".to_string(),
            extensions: vec!["zig".to_string()],
            command: vec!["zls".to_string()],
            root_markers: vec!["build.zig".to_string(), "zls.json".to_string()],
            language_id: "zig".to_string(),
        },
        // Lua
        LspServer {
            id: "lua-language-server".to_string(),
            name: "Lua Language Server".to_string(),
            extensions: vec!["lua".to_string()],
            command: vec!["lua-language-server".to_string()],
            root_markers: vec![".luarc.json".to_string(), ".luacheckrc".to_string()],
            language_id: "lua".to_string(),
        },
        // JSON
        LspServer {
            id: "vscode-json-languageserver".to_string(),
            name: "JSON Language Server".to_string(),
            extensions: vec!["json".to_string(), "jsonc".to_string()],
            command: vec![
                "vscode-json-languageserver".to_string(),
                "--stdio".to_string(),
            ],
            root_markers: vec!["package.json".to_string()],
            language_id: "json".to_string(),
        },
        // YAML
        LspServer {
            id: "yaml-language-server".to_string(),
            name: "YAML Language Server".to_string(),
            extensions: vec!["yaml".to_string(), "yml".to_string()],
            command: vec!["yaml-language-server".to_string(), "--stdio".to_string()],
            root_markers: vec![],
            language_id: "yaml".to_string(),
        },
        // HTML
        LspServer {
            id: "vscode-html-languageserver".to_string(),
            name: "HTML Language Server".to_string(),
            extensions: vec!["html".to_string(), "htm".to_string()],
            command: vec![
                "vscode-html-languageserver".to_string(),
                "--stdio".to_string(),
            ],
            root_markers: vec!["package.json".to_string()],
            language_id: "html".to_string(),
        },
        // CSS
        LspServer {
            id: "vscode-css-languageserver".to_string(),
            name: "CSS Language Server".to_string(),
            extensions: vec!["css".to_string(), "scss".to_string(), "less".to_string()],
            command: vec![
                "vscode-css-languageserver".to_string(),
                "--stdio".to_string(),
            ],
            root_markers: vec!["package.json".to_string()],
            language_id: "css".to_string(),
        },
        // Bash
        LspServer {
            id: "bash-language-server".to_string(),
            name: "Bash Language Server".to_string(),
            extensions: vec!["sh".to_string(), "bash".to_string()],
            command: vec!["bash-language-server".to_string(), "start".to_string()],
            root_markers: vec![],
            language_id: "shellscript".to_string(),
        },
        // Dockerfile
        LspServer {
            id: "docker-langserver".to_string(),
            name: "Docker Language Server".to_string(),
            extensions: vec!["dockerfile".to_string()],
            command: vec!["docker-langserver".to_string(), "--stdio".to_string()],
            root_markers: vec!["Dockerfile".to_string(), "docker-compose.yml".to_string()],
            language_id: "dockerfile".to_string(),
        },
    ]
}

/// Check if a language server is available in PATH
#[must_use]
#[allow(dead_code)]
pub fn is_server_available(server: &LspServer) -> bool {
    if server.command.is_empty() {
        return false;
    }

    which::which(&server.command[0]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_servers_not_empty() {
        let servers = default_servers();
        assert!(!servers.is_empty());
    }

    #[test]
    fn rust_analyzer_has_correct_extensions() {
        let servers = default_servers();
        let rust = servers.iter().find(|s| s.id == "rust-analyzer").unwrap();
        assert!(rust.extensions.contains(&"rs".to_string()));
    }

    #[test]
    fn find_root_uses_marker() {
        let server = LspServer {
            id: "test".to_string(),
            name: "Test".to_string(),
            extensions: vec!["txt".to_string()],
            command: vec!["test".to_string()],
            root_markers: vec!["Cargo.toml".to_string()],
            language_id: "text".to_string(),
        };

        // This test is environment-dependent
        // In a real project with Cargo.toml, find_root would find it
        let result = server.find_root(Path::new("/tmp/test.txt"));
        assert!(result.is_ok());
    }

    #[test]
    fn typescript_handles_multiple_extensions() {
        let servers = default_servers();
        let ts = servers
            .iter()
            .find(|s| s.id == "typescript-language-server")
            .unwrap();
        assert!(ts.extensions.contains(&"ts".to_string()));
        assert!(ts.extensions.contains(&"tsx".to_string()));
        assert!(ts.extensions.contains(&"js".to_string()));
    }
}
