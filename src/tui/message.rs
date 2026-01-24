//! TUI message types for conversation display.

use std::time::SystemTime;

/// Tool icons by category
pub mod icons {
    pub const SHELL: &str = "●";
    pub const READ: &str = "◇";
    pub const WRITE: &str = "◆";
    pub const EDIT: &str = "◆";
    pub const SEARCH: &str = "○";
    pub const DEFAULT: &str = "●";
    pub const ERROR: &str = "✗";
    #[allow(dead_code)]
    pub const SUCCESS: &str = "✓";
}

/// Get the icon for a tool
#[must_use]
pub fn tool_icon(name: &str) -> &'static str {
    match name {
        "shell" | "Bash" | "bash" => icons::SHELL,
        "read_file" | "Read" => icons::READ,
        "write_file" | "Write" => icons::WRITE,
        "edit_file" | "Edit" => icons::EDIT,
        "Glob" | "Grep" | "grep" | "find" => icons::SEARCH,
        _ => icons::DEFAULT,
    }
}

/// A message to display in the conversation.
#[derive(Debug, Clone)]
pub enum DisplayMessage {
    /// User message with teal border
    User {
        /// The user's message text
        text: String,
        /// When the message was sent
        timestamp: Option<SystemTime>,
    },
    /// Assistant response without border
    Assistant {
        /// The assistant's response text
        text: String,
    },
    /// Tool invocation with result
    Tool {
        /// Name of the tool
        name: String,
        /// Arguments/invocation summary (e.g., "cargo build 2>&1")
        invocation: String,
        /// Tool output/result
        output: String,
        /// Whether the tool encountered an error
        is_error: bool,
    },
}

impl DisplayMessage {
    /// Create a user message
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            timestamp: Some(SystemTime::now()),
        }
    }

    /// Create an assistant message
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::Assistant { text: text.into() }
    }

    /// Create a tool message with invocation and output
    #[must_use]
    pub fn tool(
        name: impl Into<String>,
        invocation: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::Tool {
            name: name.into(),
            invocation: invocation.into(),
            output: output.into(),
            is_error,
        }
    }

    /// Create an error tool message
    #[must_use]
    pub fn tool_error(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self::Tool {
            name: name.into(),
            invocation: String::new(),
            output: error.into(),
            is_error: true,
        }
    }
}

/// Format tool input for display
#[must_use]
#[allow(dead_code)]
pub fn format_tool_invocation(name: &str, input: &serde_json::Value) -> String {
    match name {
        "shell" | "Bash" | "bash" => input
            .get("command")
            .and_then(|v| v.as_str())
            .map(truncate_line)
            .unwrap_or_default(),
        "read_file" | "Read" => input
            .get("path")
            .and_then(|v| v.as_str())
            .map(shorten_path)
            .unwrap_or_default(),
        "write_file" | "Write" => input
            .get("path")
            .and_then(|v| v.as_str())
            .map(shorten_path)
            .unwrap_or_default(),
        "edit_file" | "Edit" => input
            .get("path")
            .and_then(|v| v.as_str())
            .map(shorten_path)
            .unwrap_or_default(),
        "Glob" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Grep" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(truncate_line)
            .unwrap_or_default(),
        _ => {
            // Generic: show first string field or empty
            input
                .as_object()
                .and_then(|obj| obj.values().find_map(|v| v.as_str()).map(truncate_line))
                .unwrap_or_default()
        }
    }
}

/// Format tool output for display, limiting lines
#[must_use]
#[allow(dead_code)]
pub fn format_tool_output(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        output.to_string()
    } else {
        let mut result: Vec<String> = lines
            .iter()
            .take(max_lines)
            .map(|s| (*s).to_string())
            .collect();
        result.push(format!("... ({} more lines)", lines.len() - max_lines));
        result.join("\n")
    }
}

/// Truncate a line to max length
#[allow(dead_code)]
fn truncate_line(s: &str) -> String {
    const MAX_LEN: usize = 60;
    if s.len() > MAX_LEN {
        format!("{}...", &s[..MAX_LEN - 3])
    } else {
        s.to_string()
    }
}

/// Shorten a path by removing common prefixes
#[allow(dead_code)]
fn shorten_path(path: &str) -> String {
    // Remove home directory prefix
    if let Ok(home) = std::env::var("HOME") {
        if let Some(suffix) = path.strip_prefix(&home) {
            return format!("~{suffix}");
        }
    }
    // Remove current directory prefix
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(cwd_str) = cwd.to_str() {
            if let Some(suffix) = path.strip_prefix(cwd_str) {
                let suffix = suffix.strip_prefix('/').unwrap_or(suffix);
                return if suffix.is_empty() {
                    ".".to_string()
                } else {
                    suffix.to_string()
                };
            }
        }
    }
    path.to_string()
}
