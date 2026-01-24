//! Web and code search tools using Exa MCP API

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Exa MCP API configuration
const API_BASE_URL: &str = "https://mcp.exa.ai";
const API_ENDPOINT: &str = "/mcp";
const DEFAULT_SEARCH_RESULTS: u32 = 8;
const DEFAULT_CODE_TOKENS: u32 = 5000;
const SEARCH_TIMEOUT_SECS: u64 = 25;
const CODE_TIMEOUT_SECS: u64 = 30;

/// Search-related errors
#[derive(Debug, Error)]
pub enum SearchError {
    /// HTTP request failed
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),

    /// API returned an error
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    /// Request timed out
    #[error("request timed out")]
    Timeout,

    /// Failed to parse response
    #[error("failed to parse response: {0}")]
    Parse(String),

    /// No results found
    #[error("no results found")]
    NoResults,
}

/// Result type for search operations
pub type Result<T> = std::result::Result<T, SearchError>;

/// MCP JSON-RPC request
#[derive(Debug, Serialize)]
struct McpRequest {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: McpParams,
}

/// MCP request parameters
#[derive(Debug, Serialize)]
struct McpParams {
    name: &'static str,
    arguments: serde_json::Value,
}

/// MCP JSON-RPC response
#[derive(Debug, Deserialize)]
struct McpResponse {
    result: McpResult,
}

/// MCP result content
#[derive(Debug, Deserialize)]
struct McpResult {
    content: Vec<McpContent>,
}

/// MCP content block
#[derive(Debug, Deserialize)]
struct McpContent {
    text: String,
}

/// Web search parameters
#[derive(Debug, Clone, Serialize)]
pub struct WebSearchParams {
    /// Search query
    pub query: String,

    /// Number of results (default: 8)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_results: Option<u32>,

    /// Live crawl mode: "fallback" or "preferred"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livecrawl: Option<String>,

    /// Search type: "auto", "fast", or "deep"
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_type: Option<String>,

    /// Max characters for context
    #[serde(rename = "contextMaxCharacters")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_max_chars: Option<u32>,
}

impl WebSearchParams {
    /// Create new web search params with just a query
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            num_results: None,
            livecrawl: None,
            search_type: None,
            context_max_chars: None,
        }
    }
}

/// Code search parameters
#[derive(Debug, Clone, Serialize)]
pub struct CodeSearchParams {
    /// Search query for APIs, libraries, SDKs
    pub query: String,

    /// Number of tokens to return (1000-50000, default: 5000)
    #[serde(rename = "tokensNum")]
    pub tokens_num: u32,
}

impl CodeSearchParams {
    /// Create new code search params
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            tokens_num: DEFAULT_CODE_TOKENS,
        }
    }

    /// Set the number of tokens
    #[must_use]
    pub fn with_tokens(mut self, tokens: u32) -> Self {
        self.tokens_num = tokens.clamp(1000, 50000);
        self
    }
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The search output text
    pub output: String,

    /// Title describing the search
    pub title: String,
}

/// Perform a web search using Exa MCP API
///
/// # Errors
///
/// Returns error if request fails, times out, or no results found
pub async fn web_search(params: WebSearchParams) -> Result<SearchResult> {
    let client = reqwest::Client::new();

    let mut args = serde_json::json!({
        "query": params.query,
        "type": params.search_type.unwrap_or_else(|| "auto".to_string()),
        "numResults": params.num_results.unwrap_or(DEFAULT_SEARCH_RESULTS),
        "livecrawl": params.livecrawl.unwrap_or_else(|| "fallback".to_string()),
    });

    if let Some(max_chars) = params.context_max_chars {
        args["contextMaxCharacters"] = serde_json::json!(max_chars);
    }

    let request = McpRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: McpParams {
            name: "web_search_exa",
            arguments: args,
        },
    };

    let query = params.query.clone();

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(SEARCH_TIMEOUT_SECS),
        client
            .post(format!("{API_BASE_URL}{API_ENDPOINT}"))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .json(&request)
            .send(),
    )
    .await
    .map_err(|_| SearchError::Timeout)?
    .map_err(SearchError::Request)?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let message = response.text().await.unwrap_or_default();
        return Err(SearchError::Api { status, message });
    }

    let text = response.text().await.map_err(SearchError::Request)?;
    parse_sse_response(&text, &query, "Web search")
}

/// Perform a code search using Exa MCP API
///
/// # Errors
///
/// Returns error if request fails, times out, or no results found
pub async fn code_search(params: CodeSearchParams) -> Result<SearchResult> {
    let client = reqwest::Client::new();

    let request = McpRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: McpParams {
            name: "get_code_context_exa",
            arguments: serde_json::json!({
                "query": params.query,
                "tokensNum": params.tokens_num,
            }),
        },
    };

    let query = params.query.clone();

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(CODE_TIMEOUT_SECS),
        client
            .post(format!("{API_BASE_URL}{API_ENDPOINT}"))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .json(&request)
            .send(),
    )
    .await
    .map_err(|_| SearchError::Timeout)?
    .map_err(SearchError::Request)?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let message = response.text().await.unwrap_or_default();
        return Err(SearchError::Api { status, message });
    }

    let text = response.text().await.map_err(SearchError::Request)?;
    parse_sse_response(&text, &query, "Code search")
}

/// Parse SSE response from Exa MCP API
fn parse_sse_response(text: &str, query: &str, prefix: &str) -> Result<SearchResult> {
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(response) = serde_json::from_str::<McpResponse>(data) {
                if let Some(content) = response.result.content.first() {
                    return Ok(SearchResult {
                        output: content.text.clone(),
                        title: format!("{prefix}: {query}"),
                    });
                }
            }
        }
    }

    Err(SearchError::NoResults)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_search_params_defaults() {
        let params = WebSearchParams::new("test query");
        assert_eq!(params.query, "test query");
        assert!(params.num_results.is_none());
        assert!(params.livecrawl.is_none());
        assert!(params.search_type.is_none());
    }

    #[test]
    fn code_search_params_defaults() {
        let params = CodeSearchParams::new("React hooks");
        assert_eq!(params.query, "React hooks");
        assert_eq!(params.tokens_num, DEFAULT_CODE_TOKENS);
    }

    #[test]
    fn code_search_params_clamps_tokens() {
        let params = CodeSearchParams::new("test").with_tokens(100);
        assert_eq!(params.tokens_num, 1000);

        let params = CodeSearchParams::new("test").with_tokens(100_000);
        assert_eq!(params.tokens_num, 50000);
    }

    #[test]
    fn parse_sse_response_extracts_content() {
        let response = r#"data: {"jsonrpc":"2.0","result":{"content":[{"type":"text","text":"Hello world"}]}}"#;
        let result = parse_sse_response(response, "test", "Search").unwrap();
        assert_eq!(result.output, "Hello world");
        assert_eq!(result.title, "Search: test");
    }

    #[test]
    fn parse_sse_response_returns_no_results() {
        let response = "data: {\"jsonrpc\":\"2.0\",\"result\":{\"content\":[]}}";
        let result = parse_sse_response(response, "test", "Search");
        assert!(matches!(result, Err(SearchError::NoResults)));
    }
}
