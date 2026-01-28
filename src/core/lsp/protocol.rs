//! LSP protocol types
//!
//! Minimal subset of LSP types needed for code intelligence

use serde::{Deserialize, Serialize};

/// Position in a text document (0-indexed)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Position {
    /// Line number
    pub line: u32,
    /// Character offset
    pub character: u32,
}

/// Range in a text document
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Range {
    /// Start position
    pub start: Position,
    /// End position
    pub end: Position,
}

/// Location in a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// Document URI
    pub uri: String,
    /// Range within document
    pub range: Range,
}

/// Text document identifier
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentIdentifier {
    /// Document URI
    pub uri: String,
}

/// Versioned text document identifier
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionedTextDocumentIdentifier {
    /// Document URI
    pub uri: String,
    /// Version number
    pub version: i32,
}

/// Text document item (for didOpen)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    /// Document URI
    pub uri: String,
    /// Language ID
    pub language_id: String,
    /// Version number
    pub version: i32,
    /// Document text
    pub text: String,
}

/// Text document position params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentPositionParams {
    /// Text document
    pub text_document: TextDocumentIdentifier,
    /// Position within document
    pub position: Position,
}

/// Reference context
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceContext {
    /// Include declaration
    pub include_declaration: bool,
}

/// Reference params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceParams {
    /// Text document
    pub text_document: TextDocumentIdentifier,
    /// Position
    pub position: Position,
    /// Context
    pub context: ReferenceContext,
}

/// Hover result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    /// Hover contents
    pub contents: HoverContents,
    /// Optional range
    pub range: Option<Range>,
}

/// Hover contents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    /// Plain string
    String(String),
    /// Marked string
    MarkedString(MarkedString),
    /// Array of marked strings
    Array(Vec<MarkedString>),
    /// Markup content
    Markup(MarkupContent),
}

/// Marked string (deprecated but still used)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarkedString {
    /// Plain string
    String(String),
    /// Language + value
    LanguageString { language: String, value: String },
}

/// Markup content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkupContent {
    /// Kind (plaintext or markdown)
    pub kind: String,
    /// Content value
    pub value: String,
}

/// Symbol kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File => write!(f, "file"),
            Self::Module => write!(f, "module"),
            Self::Namespace => write!(f, "namespace"),
            Self::Package => write!(f, "package"),
            Self::Class => write!(f, "class"),
            Self::Method => write!(f, "method"),
            Self::Property => write!(f, "property"),
            Self::Field => write!(f, "field"),
            Self::Constructor => write!(f, "constructor"),
            Self::Enum => write!(f, "enum"),
            Self::Interface => write!(f, "interface"),
            Self::Function => write!(f, "function"),
            Self::Variable => write!(f, "variable"),
            Self::Constant => write!(f, "constant"),
            Self::String => write!(f, "string"),
            Self::Number => write!(f, "number"),
            Self::Boolean => write!(f, "boolean"),
            Self::Array => write!(f, "array"),
            Self::Object => write!(f, "object"),
            Self::Key => write!(f, "key"),
            Self::Null => write!(f, "null"),
            Self::EnumMember => write!(f, "enum_member"),
            Self::Struct => write!(f, "struct"),
            Self::Event => write!(f, "event"),
            Self::Operator => write!(f, "operator"),
            Self::TypeParameter => write!(f, "type_parameter"),
        }
    }
}

/// Document symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    /// Symbol name
    pub name: String,
    /// Symbol detail
    pub detail: Option<String>,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Full range
    pub range: Range,
    /// Selection range
    pub selection_range: Range,
    /// Children
    #[serde(default)]
    pub children: Vec<Self>,
}

/// Symbol information (workspace symbol response)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInformation {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Location
    pub location: Location,
    /// Container name
    pub container_name: Option<String>,
}

/// Diagnostic severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Information => write!(f, "info"),
            Self::Hint => write!(f, "hint"),
        }
    }
}

/// Diagnostic
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// Range
    pub range: Range,
    /// Severity
    pub severity: Option<DiagnosticSeverity>,
    /// Code
    pub code: Option<serde_json::Value>,
    /// Source
    pub source: Option<String>,
    /// Message
    pub message: String,
}

/// Initialize params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Process ID
    pub process_id: Option<u32>,
    /// Root path (deprecated)
    pub root_path: Option<String>,
    /// Root URI
    pub root_uri: Option<String>,
    /// Capabilities
    pub capabilities: ClientCapabilities,
    /// Workspace folders
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_folders: Option<Vec<WorkspaceFolder>>,
}

/// Client capabilities (minimal)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    /// Text document capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_document: Option<TextDocumentClientCapabilities>,
}

/// Text document client capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentClientCapabilities {
    /// Hover capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<HoverClientCapabilities>,
}

/// Hover client capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverClientCapabilities {
    /// Dynamic registration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_registration: Option<bool>,
    /// Content format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_format: Option<Vec<String>>,
}

/// Workspace folder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFolder {
    /// URI
    pub uri: String,
    /// Name
    pub name: String,
}

/// Initialize result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Server capabilities
    pub capabilities: ServerCapabilities,
}

/// Server capabilities (minimal)
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// Hover provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_provider: Option<bool>,
    /// Definition provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_provider: Option<bool>,
    /// Implementation provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation_provider: Option<bool>,
    /// References provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references_provider: Option<bool>,
    /// Document symbol provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_symbol_provider: Option<bool>,
    /// Workspace symbol provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_symbol_provider: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_serializes() {
        let pos = Position {
            line: 10,
            character: 5,
        };
        let json = serde_json::to_string(&pos).unwrap();
        assert!(json.contains("\"line\":10"));
        assert!(json.contains("\"character\":5"));
    }

    #[test]
    fn symbol_kind_display() {
        assert_eq!(SymbolKind::Function.to_string(), "function");
        assert_eq!(SymbolKind::Class.to_string(), "class");
    }

    #[test]
    fn diagnostic_severity_display() {
        assert_eq!(DiagnosticSeverity::Error.to_string(), "error");
        assert_eq!(DiagnosticSeverity::Warning.to_string(), "warning");
    }
}
