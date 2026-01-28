//! Tool definitions and execution.

use std::path::PathBuf;
use std::process::Stdio;

use similar::{ChangeTag, TextDiff};
use tokio::process::Command;

use super::AgentMode;
use super::error::{AgentError, Result};
use super::permission::{PermissionAction, PermissionClient, PermissionContext};
use super::plan::PlanManager;
use super::types::Tool;
use crate::core::search::{self, CodeSearchParams, WebSearchParams};

/// Check if a shell command is read-only (safe to execute without permission).
#[must_use]
pub fn is_read_only(command: &str) -> bool {
    let command = command.trim();
    let first_word = command.split_whitespace().next().unwrap_or("");

    // Read-only commands
    let read_only_commands = [
        "ls", "cat", "head", "tail", "less", "more", "grep", "rg", "find", "fd", "pwd", "echo",
        "printf", "wc", "sort", "uniq", "diff", "file", "stat", "which", "whereis", "type", "man",
        "help", "date", "cal", "uptime", "whoami", "id", "groups", "env", "printenv", "hostname",
        "uname", "df", "du", "free", "top", "htop", "ps", "pgrep",
    ];

    if read_only_commands.contains(&first_word) {
        return true;
    }

    // Get the rest of the command after the first word
    let rest = command.strip_prefix(first_word).unwrap_or("").trim();

    // Git read-only subcommands
    if first_word == "git" {
        // Simple subcommands (single word)
        let simple_git = ["status", "log", "diff", "show", "branch", "remote", "tag"];
        let subcommand = rest.split_whitespace().next().unwrap_or("");
        if simple_git.contains(&subcommand) {
            return true;
        }
        // Multi-word: only "stash list" is read-only
        if rest.starts_with("stash list") {
            return true;
        }
        return false;
    }

    // Cargo read-only subcommands
    if first_word == "cargo" {
        // Simple subcommands (single word)
        let simple_cargo = ["check", "clippy", "doc", "tree", "metadata"];
        let subcommand = rest.split_whitespace().next().unwrap_or("");
        if simple_cargo.contains(&subcommand) {
            return true;
        }
        // fmt is only read-only with --check or --write=false flags
        if subcommand == "fmt" && (rest.contains("--check") || rest.contains("--write=false")) {
            return true;
        }
        // For "test --no-run" we need to check if --no-run is present anywhere
        if subcommand == "test" && rest.contains("--no-run") {
            return true;
        }
        return false;
    }

    false
}

/// A single todo item for agent task tracking.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    /// Unique identifier.
    pub id: String,
    /// Task content/description.
    pub content: String,
    /// Status: pending, `in_progress`, completed.
    pub status: String,
    /// Optional priority: high, medium, low.
    pub priority: Option<String>,
}

/// Registry of available tools.
#[derive(Debug)]
pub struct ToolRegistry {
    /// In-memory todo storage for agent task tracking.
    todos: std::sync::Arc<parking_lot::RwLock<Vec<TodoItem>>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            todos: std::sync::Arc::new(parking_lot::RwLock::new(Vec::new())),
        }
    }
}

impl ToolRegistry {
    /// Create a new tool registry with default tools.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get tool definitions for the given mode.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn definitions(&self, mode: AgentMode) -> Vec<Tool> {
        let mut tools = vec![
            Tool {
                name: "shell".to_string(),
                description: "Execute a shell command and return the output.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        }
                    },
                    "required": ["command"]
                }),
            },
            Tool {
                name: "read_file".to_string(),
                description: "Read the contents of a file.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read"
                        }
                    },
                    "required": ["path"]
                }),
            },
            Tool {
                name: "write_file".to_string(),
                description: "Write content to a file.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
            Tool {
                name: "ask_user".to_string(),
                description:
                    "Ask the user a clarifying question when you need more information to proceed."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question to ask the user"
                        },
                        "options": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional predefined choices for the user"
                        }
                    },
                    "required": ["question"]
                }),
            },
            Tool {
                name: "web_search".to_string(),
                description:
                    "Search the web for up-to-date information. Use for current events, recent documentation, or when you need information beyond your knowledge cutoff."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        },
                        "num_results": {
                            "type": "integer",
                            "description": "Number of results to return (default: 8)"
                        },
                        "search_type": {
                            "type": "string",
                            "enum": ["auto", "fast", "deep"],
                            "description": "Search type: auto (balanced), fast (quick), deep (comprehensive)"
                        }
                    },
                    "required": ["query"]
                }),
            },
            Tool {
                name: "code_search".to_string(),
                description:
                    "Search for code examples, API documentation, and library usage. Use for finding how to use specific APIs, libraries, or SDKs."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query for APIs, libraries, SDKs (e.g., 'React useState hook examples', 'Rust tokio async patterns')"
                        },
                        "tokens": {
                            "type": "integer",
                            "description": "Number of tokens to return (1000-50000, default: 5000). Use lower for focused queries, higher for comprehensive docs."
                        }
                    },
                    "required": ["query"]
                }),
            },
            Tool {
                name: "edit_file".to_string(),
                description:
                    "Edit a file by replacing a specific string with new content. The old_string must match exactly (including whitespace). Use this instead of write_file for modifying existing files."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The exact text to find and replace"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The text to replace it with"
                        },
                        "replace_all": {
                            "type": "boolean",
                            "description": "Replace all occurrences (default: false, only replaces first unique match)"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
            Tool {
                name: "glob".to_string(),
                description:
                    "Find files matching a glob pattern. Returns paths sorted by modification time (newest first)."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern (e.g., '**/*.rs', 'src/**/*.ts')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search in (default: current directory)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            Tool {
                name: "grep".to_string(),
                description:
                    "Search file contents using a regex pattern. Returns matching lines with file paths and line numbers."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory or file to search in (default: current directory)"
                        },
                        "include": {
                            "type": "string",
                            "description": "File pattern to include (e.g., '*.rs', '*.{ts,tsx}')"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            Tool {
                name: "list_dir".to_string(),
                description:
                    "List contents of a directory. Shows files and subdirectories with basic info."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path to list (default: current directory)"
                        }
                    },
                    "required": []
                }),
            },
            Tool {
                name: "web_fetch".to_string(),
                description:
                    "Fetch content from a URL and process it. Converts HTML to readable text. Use for reading documentation, web pages, or API responses."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch content from"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "Optional prompt to filter/summarize the content"
                        }
                    },
                    "required": ["url"]
                }),
            },
            Tool {
                name: "todo_read".to_string(),
                description:
                    "Read the current list of todos/tasks. Returns all tasks with their ID, content, status, and priority."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "todo_write".to_string(),
                description:
                    "Create, update, or delete a todo/task. Use to track progress on multi-step tasks."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["create", "update", "delete"],
                            "description": "Action to perform"
                        },
                        "id": {
                            "type": "string",
                            "description": "Task ID (required for update/delete)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Task content/description (required for create)"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["pending", "in_progress", "completed"],
                            "description": "Task status"
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["high", "medium", "low"],
                            "description": "Task priority"
                        }
                    },
                    "required": ["action"]
                }),
            },
            Tool {
                name: "apply_patch".to_string(),
                description:
                    "Apply a unified diff patch to one or more files. Use for applying changes from external sources or reverting changes."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "patch": {
                            "type": "string",
                            "description": "The unified diff patch content"
                        },
                        "path": {
                            "type": "string",
                            "description": "Base directory to apply patch in (default: current directory)"
                        }
                    },
                    "required": ["patch"]
                }),
            },
            Tool {
                name: "multi_edit".to_string(),
                description:
                    "Edit multiple files in a single operation. More efficient than multiple edit_file calls."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "edits": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "path": {
                                        "type": "string",
                                        "description": "Path to the file to edit"
                                    },
                                    "old_string": {
                                        "type": "string",
                                        "description": "The exact text to find and replace"
                                    },
                                    "new_string": {
                                        "type": "string",
                                        "description": "The text to replace it with"
                                    }
                                },
                                "required": ["path", "old_string", "new_string"]
                            },
                            "description": "List of edits to apply"
                        }
                    },
                    "required": ["edits"]
                }),
            },
            Tool {
                name: "github_pr".to_string(),
                description:
                    "Create a GitHub pull request. Requires gh CLI to be installed and authenticated."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "PR title"
                        },
                        "body": {
                            "type": "string",
                            "description": "PR body/description (markdown)"
                        },
                        "base": {
                            "type": "string",
                            "description": "Base branch (default: repo default branch)"
                        },
                        "draft": {
                            "type": "boolean",
                            "description": "Create as draft PR"
                        }
                    },
                    "required": ["title"]
                }),
            },
            Tool {
                name: "github_issue".to_string(),
                description:
                    "Create or view GitHub issues. Requires gh CLI to be installed and authenticated."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["create", "view", "list", "close"],
                            "description": "Action to perform"
                        },
                        "title": {
                            "type": "string",
                            "description": "Issue title (for create)"
                        },
                        "body": {
                            "type": "string",
                            "description": "Issue body (for create)"
                        },
                        "number": {
                            "type": "integer",
                            "description": "Issue number (for view/close)"
                        },
                        "labels": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Labels to add (for create)"
                        }
                    },
                    "required": ["action"]
                }),
            },
            Tool {
                name: "github_pr_review".to_string(),
                description:
                    "View or comment on pull requests. Requires gh CLI to be installed and authenticated."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["view", "diff", "checks", "comment", "list"],
                            "description": "Action to perform"
                        },
                        "number": {
                            "type": "integer",
                            "description": "PR number (required for view/diff/checks/comment)"
                        },
                        "body": {
                            "type": "string",
                            "description": "Comment body (for comment action)"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["open", "closed", "merged", "all"],
                            "description": "Filter by state (for list action)"
                        }
                    },
                    "required": ["action"]
                }),
            },
            Tool {
                name: "sandbox_exec".to_string(),
                description:
                    "Execute code in an isolated sandbox environment. Uses Docker if available for full isolation, otherwise falls back to restricted execution with timeout and network limits. Best for running untrusted code or testing."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute in the sandbox"
                        },
                        "language": {
                            "type": "string",
                            "enum": ["bash", "python", "node", "ruby"],
                            "description": "Language/runtime to use (default: bash)"
                        },
                        "code": {
                            "type": "string",
                            "description": "Code to execute (alternative to command)"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds (default: 30, max: 300)"
                        },
                        "network": {
                            "type": "boolean",
                            "description": "Allow network access (default: false)"
                        },
                        "workdir": {
                            "type": "string",
                            "description": "Working directory to mount read-only"
                        }
                    },
                    "required": []
                }),
            },
        ];

        // Add mode-specific tools
        match mode {
            AgentMode::Build => {
                tools.push(Tool {
                    name: "plan_enter".to_string(),
                    description: "Switch to plan mode for complex tasks that benefit from planning before implementation. In plan mode, you can explore and analyze the codebase but cannot make changes.".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "reason": {
                                "type": "string",
                                "description": "Why planning would help for this task"
                            }
                        },
                        "required": ["reason"]
                    }),
                });
            }
            AgentMode::Plan => {
                tools.push(Tool {
                    name: "plan_exit".to_string(),
                    description: "Exit plan mode and return to build mode for implementation. Call this when the plan is ready and approved.".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {},
                        "required": []
                    }),
                });
            }
        }

        tools
    }

    /// Execute a tool by name.
    ///
    /// # Errors
    ///
    /// Returns error if tool is unknown, permission denied, or execution fails.
    pub async fn execute(
        &self,
        name: &str,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
        plan_manager: &PlanManager,
    ) -> Result<String> {
        match name {
            "shell" => self.execute_shell(input, permissions, mode).await,
            "read_file" => self.execute_read_file(input).await,
            "write_file" => {
                self.execute_write_file(input, permissions, mode, plan_manager)
                    .await
            }
            "edit_file" => self.execute_edit_file(input, permissions, mode).await,
            "ask_user" => self.execute_ask_user(input, permissions).await,
            "plan_enter" => self.execute_plan_enter(input, permissions, mode).await,
            "plan_exit" => self.execute_plan_exit(permissions, mode).await,
            "web_search" => self.execute_web_search(input, permissions).await,
            "code_search" => self.execute_code_search(input, permissions).await,
            "glob" => self.execute_glob(input).await,
            "grep" => self.execute_grep(input).await,
            "list_dir" => self.execute_list_dir(input).await,
            "web_fetch" => self.execute_web_fetch(input, permissions).await,
            "todo_read" => self.execute_todo_read(),
            "todo_write" => self.execute_todo_write(input),
            "apply_patch" => self.execute_apply_patch(input, permissions, mode).await,
            "multi_edit" => self.execute_multi_edit(input, permissions, mode).await,
            "github_pr" => self.execute_github_pr(input, permissions, mode).await,
            "github_issue" => self.execute_github_issue(input, permissions, mode).await,
            "github_pr_review" => {
                self.execute_github_pr_review(input, permissions, mode)
                    .await
            }
            "sandbox_exec" => self.execute_sandbox(input, permissions, mode).await,
            _ => Err(AgentError::ToolExecution(format!("unknown tool: {name}"))),
        }
    }

    async fn execute_shell(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing command".to_string()))?;

        tracing::info!(command = %command, "executing shell command");

        let read_only = is_read_only(command);

        // In Plan mode, only allow read-only commands
        if mode == AgentMode::Plan && !read_only {
            return Err(AgentError::ToolExecution(
                "In plan mode, only read-only commands are allowed. Use plan_exit to switch to build mode for write operations.".to_string(),
            ));
        }

        // Check if permission needed
        if !read_only {
            if let Some(perms) = permissions {
                let approved = perms
                    .request(
                        "shell",
                        PermissionAction::Execute,
                        PermissionContext::Bash {
                            command: command.to_string(),
                            working_dir: std::env::current_dir()
                                .unwrap_or_else(|_| PathBuf::from("/")),
                        },
                    )
                    .await
                    .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

                if !approved {
                    return Err(AgentError::ToolExecution(
                        "Permission denied by user. Do not retry this action.".to_string(),
                    ));
                }
            }
        }

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.to_string())
        } else {
            Ok(format!(
                "Command failed:\nstdout: {stdout}\nstderr: {stderr}"
            ))
        }
    }

    async fn execute_read_file(&self, input: serde_json::Value) -> Result<String> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing path".to_string()))?;

        tracing::info!(path = %path, "reading file");

        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))
    }

    async fn execute_write_file(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
        plan_manager: &PlanManager,
    ) -> Result<String> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing path".to_string()))?;
        let content = input["content"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing content".to_string()))?;

        let path_buf = PathBuf::from(path);

        // In Plan mode, only allow writing to plan files
        if mode == AgentMode::Plan && !plan_manager.is_plan_path(&path_buf) {
            return Err(AgentError::ToolExecution(format!(
                "In plan mode, you can only write to plan files in .omni/plans/. Use plan_exit to switch to build mode for other file operations. Attempted: {path}"
            )));
        }

        tracing::info!(path = %path, "writing file");

        // Ensure parent directory exists for plan files
        if mode == AgentMode::Plan {
            if let Some(parent) = path_buf.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| AgentError::ToolExecution(e.to_string()))?;
            }
        }

        // Request permission (skip for plan files in plan mode)
        let needs_permission = mode != AgentMode::Plan || !plan_manager.is_plan_path(&path_buf);
        if needs_permission {
            if let Some(perms) = permissions {
                const PREVIEW_MAX_CHARS: usize = 500;
                let preview = if content.chars().count() > PREVIEW_MAX_CHARS {
                    let truncated: String = content.chars().take(PREVIEW_MAX_CHARS).collect();
                    format!("{}... ({} bytes total)", truncated, content.len())
                } else {
                    content.to_string()
                };

                let approved = perms
                    .request(
                        "write_file",
                        PermissionAction::WriteFile,
                        PermissionContext::WriteFile {
                            path: path_buf.clone(),
                            content_preview: preview,
                        },
                    )
                    .await
                    .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

                if !approved {
                    return Err(AgentError::ToolExecution(
                        "Permission denied by user. Do not retry this action.".to_string(),
                    ));
                }
            }
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        Ok(format!("Wrote {} bytes to {path}", content.len()))
    }

    async fn execute_ask_user(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
    ) -> Result<String> {
        let question = input["question"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing question".to_string()))?;

        let options = input["options"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

        tracing::info!(question = %question, "asking user");

        if let Some(perms) = permissions {
            let answer = perms
                .ask_user(question, options)
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            Ok(answer)
        } else {
            Err(AgentError::ToolExecution(
                "No permission client available for ask_user".to_string(),
            ))
        }
    }

    async fn execute_plan_enter(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        if mode == AgentMode::Plan {
            return Err(AgentError::ToolExecution(
                "Already in plan mode.".to_string(),
            ));
        }

        let reason = input["reason"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing reason".to_string()))?;

        tracing::info!(reason = %reason, "requesting plan mode");

        // Ask user for confirmation via ask_user
        if let Some(perms) = permissions {
            let question = format!("Switch to plan mode?\n\nReason: {reason}");
            let options = Some(vec![
                "Yes, enter plan mode".to_string(),
                "No, stay in build mode".to_string(),
            ]);

            let answer = perms
                .ask_user(&question, options)
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if answer.to_lowercase().contains("yes") || answer.to_lowercase().contains("enter plan")
            {
                // Return special marker that agent will interpret
                Ok("[MODE_SWITCH:PLAN]".to_string())
            } else {
                Err(AgentError::ToolExecution(
                    "User declined to enter plan mode.".to_string(),
                ))
            }
        } else {
            Err(AgentError::ToolExecution(
                "No permission client available for plan_enter".to_string(),
            ))
        }
    }

    async fn execute_plan_exit(
        &self,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        if mode == AgentMode::Build {
            return Err(AgentError::ToolExecution(
                "Already in build mode.".to_string(),
            ));
        }

        tracing::info!("requesting build mode");

        // Ask user for confirmation
        if let Some(perms) = permissions {
            let question = "Exit plan mode and return to build mode for implementation?";
            let options = Some(vec![
                "Yes, start building".to_string(),
                "No, continue planning".to_string(),
            ]);

            let answer = perms
                .ask_user(question, options)
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if answer.to_lowercase().contains("yes")
                || answer.to_lowercase().contains("start building")
            {
                // Return special marker that agent will interpret
                Ok("[MODE_SWITCH:BUILD]".to_string())
            } else {
                Err(AgentError::ToolExecution(
                    "User declined to exit plan mode.".to_string(),
                ))
            }
        } else {
            Err(AgentError::ToolExecution(
                "No permission client available for plan_exit".to_string(),
            ))
        }
    }

    async fn execute_web_search(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
    ) -> Result<String> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing query".to_string()))?;

        tracing::info!(query = %query, "executing web search");

        // Request permission
        if let Some(perms) = permissions {
            let approved = perms
                .request(
                    "web_search",
                    PermissionAction::WebSearch,
                    PermissionContext::WebSearch {
                        query: query.to_string(),
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user. Do not retry this action.".to_string(),
                ));
            }
        }

        let mut params = WebSearchParams::new(query);

        if let Some(num) = input["num_results"].as_u64() {
            #[allow(clippy::cast_possible_truncation)]
            {
                params.num_results = Some(num as u32);
            }
        }

        if let Some(search_type) = input["search_type"].as_str() {
            params.search_type = Some(search_type.to_string());
        }

        match search::web_search(params).await {
            Ok(result) => Ok(result.output),
            Err(e) => Err(AgentError::ToolExecution(e.to_string())),
        }
    }

    async fn execute_code_search(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
    ) -> Result<String> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing query".to_string()))?;

        #[allow(clippy::cast_possible_truncation)]
        let tokens = input["tokens"].as_u64().map_or(5000, |t| t as u32);

        tracing::info!(query = %query, tokens = %tokens, "executing code search");

        // Request permission
        if let Some(perms) = permissions {
            let approved = perms
                .request(
                    "code_search",
                    PermissionAction::CodeSearch,
                    PermissionContext::CodeSearch {
                        query: query.to_string(),
                        tokens,
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user. Do not retry this action.".to_string(),
                ));
            }
        }

        let params = CodeSearchParams::new(query).with_tokens(tokens);

        match search::code_search(params).await {
            Ok(result) => Ok(result.output),
            Err(e) => Err(AgentError::ToolExecution(e.to_string())),
        }
    }

    async fn execute_edit_file(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing path".to_string()))?;
        let old_string = input["old_string"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing old_string".to_string()))?;
        let new_string = input["new_string"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing new_string".to_string()))?;
        let replace_all = input["replace_all"].as_bool().unwrap_or(false);

        // In Plan mode, disallow edits
        if mode == AgentMode::Plan {
            return Err(AgentError::ToolExecution(
                "In plan mode, file editing is not allowed. Use plan_exit to switch to build mode."
                    .to_string(),
            ));
        }

        if old_string == new_string {
            return Err(AgentError::ToolExecution(
                "old_string and new_string must be different".to_string(),
            ));
        }

        tracing::info!(path = %path, "editing file");

        // Read current content
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::ToolExecution(format!("failed to read file: {e}")))?;

        // Check if old_string exists
        if !content.contains(old_string) {
            return Err(AgentError::ToolExecution(
                "old_string not found in file content".to_string(),
            ));
        }

        // Check for multiple matches if not replace_all
        if !replace_all {
            let count = content.matches(old_string).count();
            if count > 1 {
                return Err(AgentError::ToolExecution(format!(
                    "Found {count} matches for old_string. Use replace_all: true to replace all, or provide more context to make the match unique."
                )));
            }
        }

        // Generate new content
        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        // Generate diff for permission dialog
        let diff = generate_diff(&content, &new_content);

        // Request permission
        if let Some(perms) = permissions {
            let approved = perms
                .request(
                    "edit_file",
                    PermissionAction::EditFile,
                    PermissionContext::EditFile {
                        path: PathBuf::from(path),
                        diff: diff.clone(),
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user. Do not retry this action.".to_string(),
                ));
            }
        }

        // Write new content
        tokio::fs::write(path, &new_content)
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        // Count changes
        let additions = new_content.lines().count();
        let deletions = content.lines().count();
        let net = additions as i64 - deletions as i64;

        Ok(format!(
            "Edit applied successfully. ({net:+} lines)\n\n{diff}"
        ))
    }

    async fn execute_glob(&self, input: serde_json::Value) -> Result<String> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing pattern".to_string()))?;
        let search_path = input["path"].as_str().map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            PathBuf::from,
        );

        tracing::info!(pattern = %pattern, path = %search_path.display(), "glob search");

        // Use fd or find for glob matching
        let output = Command::new("fd")
            .args(["--glob", pattern, "--type", "f", "--hidden", "--follow"])
            .current_dir(&search_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        let result = match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            _ => {
                // Fallback to find
                let out = Command::new("find")
                    .args([".", "-name", pattern, "-type", "f"])
                    .current_dir(&search_path)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(e.to_string()))?;
                String::from_utf8_lossy(&out.stdout).to_string()
            }
        };

        let mut files: Vec<_> = result.lines().filter(|l| !l.is_empty()).collect();
        let truncated = files.len() > 100;
        files.truncate(100);

        if files.is_empty() {
            return Ok("No files found".to_string());
        }

        let mut output = files.join("\n");
        if truncated {
            output.push_str("\n\n(Results truncated. Use a more specific pattern.)");
        }

        Ok(output)
    }

    async fn execute_grep(&self, input: serde_json::Value) -> Result<String> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing pattern".to_string()))?;
        let search_path = input["path"].as_str().map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            PathBuf::from,
        );
        let include = input["include"].as_str();

        tracing::info!(pattern = %pattern, path = %search_path.display(), "grep search");

        // Use ripgrep for fast search
        let mut cmd = Command::new("rg");
        cmd.args([
            "-n",
            "--hidden",
            "--follow",
            "--no-heading",
            "--with-filename",
            pattern,
        ]);

        if let Some(glob) = include {
            cmd.args(["--glob", glob]);
        }

        cmd.arg(&search_path);

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        let result = if let Ok(out) = output {
            if out.status.success() || out.status.code() == Some(1) {
                // Exit code 1 means no matches
                String::from_utf8_lossy(&out.stdout).to_string()
            } else {
                // Fallback to grep
                let mut grep_cmd = Command::new("grep");
                grep_cmd.args(["-rn", "--include", include.unwrap_or("*"), pattern]);
                grep_cmd.arg(&search_path);

                let grep_out = grep_cmd
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

                String::from_utf8_lossy(&grep_out.stdout).to_string()
            }
        } else {
            // rg not found, use grep
            let mut grep_cmd = Command::new("grep");
            grep_cmd.args(["-rn", pattern]);
            grep_cmd.arg(&search_path);

            let grep_out = grep_cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            String::from_utf8_lossy(&grep_out.stdout).to_string()
        };

        let lines: Vec<_> = result.lines().collect();
        let truncated = lines.len() > 100;
        let display_lines: Vec<_> = lines.into_iter().take(100).collect();

        if display_lines.is_empty() {
            return Ok("No matches found".to_string());
        }

        let mut output = display_lines.join("\n");
        if truncated {
            output.push_str("\n\n(Results truncated. Use a more specific pattern or path.)");
        }

        Ok(output)
    }

    async fn execute_list_dir(&self, input: serde_json::Value) -> Result<String> {
        let path = input["path"].as_str().map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            PathBuf::from,
        );

        tracing::info!(path = %path.display(), "listing directory");

        let mut entries = tokio::fs::read_dir(&path)
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata().await.ok();

            if let Some(meta) = metadata {
                if meta.is_dir() {
                    dirs.push(format!("{name}/"));
                } else {
                    let size = meta.len();
                    files.push(format!("{name} ({size} bytes)"));
                }
            } else {
                files.push(name);
            }
        }

        dirs.sort();
        files.sort();

        let mut output = Vec::new();
        output.push(format!("Directory: {}", path.display()));
        output.push(String::new());

        if !dirs.is_empty() {
            output.push("Directories:".to_string());
            output.extend(dirs.into_iter().map(|d| format!("  {d}")));
            output.push(String::new());
        }

        if !files.is_empty() {
            output.push("Files:".to_string());
            output.extend(files.into_iter().map(|f| format!("  {f}")));
        }

        if output.len() <= 3 {
            return Ok(format!("Directory {} is empty", path.display()));
        }

        Ok(output.join("\n"))
    }

    async fn execute_web_fetch(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
    ) -> Result<String> {
        let url = input["url"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing url".to_string()))?;

        let prompt = input["prompt"].as_str();

        tracing::info!(url = %url, "fetching web content");

        // Check permission for web fetch
        if let Some(perms) = permissions {
            let approved = perms
                .request(
                    "web_fetch",
                    PermissionAction::WebFetch,
                    PermissionContext::WebFetch {
                        url: url.to_string(),
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user. Do not retry this action.".to_string(),
                ));
            }
        }

        // Fetch the URL content
        let response = reqwest::get(url)
            .await
            .map_err(|e| AgentError::ToolExecution(format!("failed to fetch URL: {e}")))?;

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/plain")
            .to_string();

        let body = response
            .text()
            .await
            .map_err(|e| AgentError::ToolExecution(format!("failed to read response: {e}")))?;

        // Convert HTML to text if needed
        let content = if content_type.contains("html") {
            html_to_text(&body)
        } else {
            body
        };

        // Truncate if too long
        let max_len = 50_000;
        let content = if content.len() > max_len {
            format!(
                "{}\n\n[Content truncated - {} characters total]",
                &content[..max_len],
                content.len()
            )
        } else {
            content
        };

        if let Some(prompt_text) = prompt {
            Ok(format!("Prompt: {prompt_text}\n\nContent:\n{content}"))
        } else {
            Ok(content)
        }
    }

    fn execute_todo_read(&self) -> Result<String> {
        let todos = self.todos.read();

        if todos.is_empty() {
            return Ok("No todos. Use todo_write to create tasks.".to_string());
        }

        let output: Vec<String> = todos
            .iter()
            .map(|t| {
                let priority = t
                    .priority
                    .as_ref()
                    .map_or(String::new(), |p| format!(" [{p}]"));
                format!("- [{}] {} ({}){}", t.id, t.content, t.status, priority)
            })
            .collect();

        Ok(format!("Todos:\n{}", output.join("\n")))
    }

    fn execute_todo_write(&self, input: serde_json::Value) -> Result<String> {
        let action = input["action"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing action".to_string()))?;

        match action {
            "create" => {
                let content = input["content"]
                    .as_str()
                    .ok_or_else(|| AgentError::ToolExecution("missing content".to_string()))?;
                let status = input["status"].as_str().unwrap_or("pending");
                let priority = input["priority"].as_str().map(String::from);

                let id = format!("{:04}", rand::random::<u16>());
                let item = TodoItem {
                    id: id.clone(),
                    content: content.to_string(),
                    status: status.to_string(),
                    priority,
                };

                self.todos.write().push(item);
                Ok(format!("Created todo {id}: {content}"))
            }
            "update" => {
                let id = input["id"]
                    .as_str()
                    .ok_or_else(|| AgentError::ToolExecution("missing id".to_string()))?;

                let mut todos = self.todos.write();
                let item = todos
                    .iter_mut()
                    .find(|t| t.id == id)
                    .ok_or_else(|| AgentError::ToolExecution(format!("todo {id} not found")))?;

                if let Some(content) = input["content"].as_str() {
                    item.content = content.to_string();
                }
                if let Some(status) = input["status"].as_str() {
                    item.status = status.to_string();
                }
                if let Some(priority) = input["priority"].as_str() {
                    item.priority = Some(priority.to_string());
                }

                Ok(format!("Updated todo {id}"))
            }
            "delete" => {
                let id = input["id"]
                    .as_str()
                    .ok_or_else(|| AgentError::ToolExecution("missing id".to_string()))?;

                let mut todos = self.todos.write();
                let idx = todos
                    .iter()
                    .position(|t| t.id == id)
                    .ok_or_else(|| AgentError::ToolExecution(format!("todo {id} not found")))?;

                todos.remove(idx);
                Ok(format!("Deleted todo {id}"))
            }
            _ => Err(AgentError::ToolExecution(format!(
                "unknown action: {action}"
            ))),
        }
    }

    async fn execute_apply_patch(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        let patch = input["patch"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing patch".to_string()))?;
        let base_path = input["path"].as_str().map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            PathBuf::from,
        );

        // In Plan mode, disallow patches
        if mode == AgentMode::Plan {
            return Err(AgentError::ToolExecution(
                "In plan mode, applying patches is not allowed. Use plan_exit to switch to build mode."
                    .to_string(),
            ));
        }

        tracing::info!(path = %base_path.display(), "applying patch");

        // Request permission
        if let Some(perms) = permissions {
            // Show a preview of the patch
            let preview = if patch.len() > 500 {
                format!("{}...\n({} bytes total)", &patch[..500], patch.len())
            } else {
                patch.to_string()
            };

            let approved = perms
                .request(
                    "apply_patch",
                    PermissionAction::EditFile,
                    PermissionContext::EditFile {
                        path: base_path.clone(),
                        diff: preview,
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user. Do not retry this action.".to_string(),
                ));
            }
        }

        // Apply using patch command
        let mut child = Command::new("patch")
            .args(["-p1", "--no-backup-if-mismatch"])
            .current_dir(&base_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AgentError::ToolExecution(format!("failed to run patch: {e}")))?;

        // Write patch to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(patch.as_bytes())
                .await
                .map_err(|e| AgentError::ToolExecution(format!("failed to write patch: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| AgentError::ToolExecution(format!("patch failed: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!("Patch applied successfully.\n{stdout}"))
        } else {
            Err(AgentError::ToolExecution(format!(
                "Patch failed:\n{stdout}\n{stderr}"
            )))
        }
    }

    async fn execute_multi_edit(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        let edits = input["edits"]
            .as_array()
            .ok_or_else(|| AgentError::ToolExecution("missing edits array".to_string()))?;

        // In Plan mode, disallow edits
        if mode == AgentMode::Plan {
            return Err(AgentError::ToolExecution(
                "In plan mode, file editing is not allowed. Use plan_exit to switch to build mode."
                    .to_string(),
            ));
        }

        if edits.is_empty() {
            return Err(AgentError::ToolExecution(
                "edits array is empty".to_string(),
            ));
        }

        tracing::info!(count = edits.len(), "multi-edit");

        let mut results = Vec::new();
        let mut all_diffs = String::new();

        // First pass: validate all edits and generate diffs
        let mut pending_edits = Vec::new();
        for (i, edit) in edits.iter().enumerate() {
            let path = edit["path"]
                .as_str()
                .ok_or_else(|| AgentError::ToolExecution(format!("edit {i}: missing path")))?;
            let old_string = edit["old_string"].as_str().ok_or_else(|| {
                AgentError::ToolExecution(format!("edit {i}: missing old_string"))
            })?;
            let new_string = edit["new_string"].as_str().ok_or_else(|| {
                AgentError::ToolExecution(format!("edit {i}: missing new_string"))
            })?;

            if old_string == new_string {
                return Err(AgentError::ToolExecution(format!(
                    "edit {i}: old_string and new_string must be different"
                )));
            }

            // Read and validate
            let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                AgentError::ToolExecution(format!("edit {i}: failed to read {path}: {e}"))
            })?;

            if !content.contains(old_string) {
                return Err(AgentError::ToolExecution(format!(
                    "edit {i}: old_string not found in {path}"
                )));
            }

            let count = content.matches(old_string).count();
            if count > 1 {
                return Err(AgentError::ToolExecution(format!(
                    "edit {i}: found {count} matches in {path}. Provide more context to make the match unique."
                )));
            }

            let new_content = content.replacen(old_string, new_string, 1);
            let diff = generate_diff(&content, &new_content);

            use std::fmt::Write;
            let _ = write!(all_diffs, "--- {path}\n+++ {path}\n{diff}\n");
            pending_edits.push((path.to_string(), new_content));
        }

        // Request permission for all edits
        if let Some(perms) = permissions {
            let approved = perms
                .request(
                    "multi_edit",
                    PermissionAction::EditFile,
                    PermissionContext::EditFile {
                        path: PathBuf::from(format!("{} files", pending_edits.len())),
                        diff: all_diffs.clone(),
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user. Do not retry this action.".to_string(),
                ));
            }
        }

        // Apply all edits
        for (path, new_content) in pending_edits {
            tokio::fs::write(&path, &new_content)
                .await
                .map_err(|e| AgentError::ToolExecution(format!("failed to write {path}: {e}")))?;
            results.push(format!("Edited: {path}"));
        }

        Ok(format!(
            "{} files edited successfully.\n\n{}",
            results.len(),
            results.join("\n")
        ))
    }

    async fn execute_github_pr(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        if mode == AgentMode::Plan {
            return Err(AgentError::ToolExecution(
                "Cannot create PRs in plan mode".to_string(),
            ));
        }

        let title = input["title"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing title".to_string()))?;
        let body = input["body"].as_str().unwrap_or("");
        let base = input["base"].as_str();
        let draft = input["draft"].as_bool().unwrap_or(false);

        // Request permission
        if let Some(perms) = permissions {
            let mut cmd_desc = format!("gh pr create --title {title:?}");
            if !body.is_empty() {
                cmd_desc.push_str(" --body <...>");
            }
            if draft {
                cmd_desc.push_str(" --draft");
            }

            let approved = perms
                .request(
                    "github_pr",
                    PermissionAction::Execute,
                    PermissionContext::Bash {
                        command: cmd_desc,
                        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user".to_string(),
                ));
            }
        }

        let mut cmd = Command::new("gh");
        cmd.args(["pr", "create", "--title", title]);

        if !body.is_empty() {
            cmd.args(["--body", body]);
        }
        if let Some(b) = base {
            cmd.args(["--base", b]);
        }
        if draft {
            cmd.arg("--draft");
        }

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!("PR created: {stdout}"))
        } else {
            Err(AgentError::ToolExecution(format!(
                "gh pr create failed: {stderr}"
            )))
        }
    }

    async fn execute_github_issue(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        let action = input["action"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing action".to_string()))?;

        match action {
            "create" => {
                if mode == AgentMode::Plan {
                    return Err(AgentError::ToolExecution(
                        "Cannot create issues in plan mode".to_string(),
                    ));
                }

                let title = input["title"]
                    .as_str()
                    .ok_or_else(|| AgentError::ToolExecution("missing title".to_string()))?;
                let body = input["body"].as_str().unwrap_or("");
                let labels: Vec<&str> = input["labels"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                // Request permission
                if let Some(perms) = permissions {
                    let approved = perms
                        .request(
                            "github_issue",
                            PermissionAction::Execute,
                            PermissionContext::Bash {
                                command: format!("gh issue create --title {title:?}"),
                                working_dir: std::env::current_dir()
                                    .unwrap_or_else(|_| PathBuf::from("/")),
                            },
                        )
                        .await
                        .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

                    if !approved {
                        return Err(AgentError::ToolExecution(
                            "Permission denied by user".to_string(),
                        ));
                    }
                }

                let mut cmd = Command::new("gh");
                cmd.args(["issue", "create", "--title", title]);

                if !body.is_empty() {
                    cmd.args(["--body", body]);
                }
                for label in labels {
                    cmd.args(["--label", label]);
                }

                let output = cmd
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(format!("Issue created: {stdout}"))
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh issue create failed: {stderr}"
                    )))
                }
            }
            "view" => {
                let number = input["number"]
                    .as_i64()
                    .ok_or_else(|| AgentError::ToolExecution("missing number".to_string()))?;

                let output = Command::new("gh")
                    .args(["issue", "view", &number.to_string()])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh issue view failed: {stderr}"
                    )))
                }
            }
            "list" => {
                let output = Command::new("gh")
                    .args(["issue", "list"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh issue list failed: {stderr}"
                    )))
                }
            }
            "close" => {
                if mode == AgentMode::Plan {
                    return Err(AgentError::ToolExecution(
                        "Cannot close issues in plan mode".to_string(),
                    ));
                }

                let number = input["number"]
                    .as_i64()
                    .ok_or_else(|| AgentError::ToolExecution("missing number".to_string()))?;

                // Request permission
                if let Some(perms) = permissions {
                    let approved = perms
                        .request(
                            "github_issue",
                            PermissionAction::Execute,
                            PermissionContext::Bash {
                                command: format!("gh issue close {number}"),
                                working_dir: std::env::current_dir()
                                    .unwrap_or_else(|_| PathBuf::from("/")),
                            },
                        )
                        .await
                        .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

                    if !approved {
                        return Err(AgentError::ToolExecution(
                            "Permission denied by user".to_string(),
                        ));
                    }
                }

                let output = Command::new("gh")
                    .args(["issue", "close", &number.to_string()])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(format!("Issue closed: {stdout}"))
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh issue close failed: {stderr}"
                    )))
                }
            }
            _ => Err(AgentError::ToolExecution(format!(
                "unknown issue action: {action}"
            ))),
        }
    }

    async fn execute_github_pr_review(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        let action = input["action"]
            .as_str()
            .ok_or_else(|| AgentError::ToolExecution("missing action".to_string()))?;

        match action {
            "view" => {
                let number = input["number"]
                    .as_i64()
                    .ok_or_else(|| AgentError::ToolExecution("missing number".to_string()))?;

                let output = Command::new("gh")
                    .args(["pr", "view", &number.to_string()])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh pr view failed: {stderr}"
                    )))
                }
            }
            "diff" => {
                let number = input["number"]
                    .as_i64()
                    .ok_or_else(|| AgentError::ToolExecution("missing number".to_string()))?;

                let output = Command::new("gh")
                    .args(["pr", "diff", &number.to_string()])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh pr diff failed: {stderr}"
                    )))
                }
            }
            "checks" => {
                let number = input["number"]
                    .as_i64()
                    .ok_or_else(|| AgentError::ToolExecution("missing number".to_string()))?;

                let output = Command::new("gh")
                    .args(["pr", "checks", &number.to_string()])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh pr checks failed: {stderr}"
                    )))
                }
            }
            "comment" => {
                if mode == AgentMode::Plan {
                    return Err(AgentError::ToolExecution(
                        "Cannot comment on PRs in plan mode".to_string(),
                    ));
                }

                let number = input["number"]
                    .as_i64()
                    .ok_or_else(|| AgentError::ToolExecution("missing number".to_string()))?;
                let body = input["body"]
                    .as_str()
                    .ok_or_else(|| AgentError::ToolExecution("missing body".to_string()))?;

                // Request permission
                if let Some(perms) = permissions {
                    let approved = perms
                        .request(
                            "github_pr_review",
                            PermissionAction::Execute,
                            PermissionContext::Bash {
                                command: format!("gh pr comment {number}"),
                                working_dir: std::env::current_dir()
                                    .unwrap_or_else(|_| PathBuf::from("/")),
                            },
                        )
                        .await
                        .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

                    if !approved {
                        return Err(AgentError::ToolExecution(
                            "Permission denied by user".to_string(),
                        ));
                    }
                }

                let output = Command::new("gh")
                    .args(["pr", "comment", &number.to_string(), "--body", body])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(format!("Comment added: {stdout}"))
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh pr comment failed: {stderr}"
                    )))
                }
            }
            "list" => {
                let state = input["state"].as_str().unwrap_or("open");

                let output = Command::new("gh")
                    .args(["pr", "list", "--state", state])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| AgentError::ToolExecution(format!("failed to run gh: {e}")))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(AgentError::ToolExecution(format!(
                        "gh pr list failed: {stderr}"
                    )))
                }
            }
            _ => Err(AgentError::ToolExecution(format!(
                "unknown PR review action: {action}"
            ))),
        }
    }

    async fn execute_sandbox(
        &self,
        input: serde_json::Value,
        permissions: Option<&PermissionClient>,
        mode: AgentMode,
    ) -> Result<String> {
        if mode == AgentMode::Plan {
            return Err(AgentError::ToolExecution(
                "Cannot execute sandbox in plan mode".to_string(),
            ));
        }

        let language = input["language"].as_str().unwrap_or("bash");
        let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(30).clamp(1, 300);
        let network = input["network"].as_bool().unwrap_or(false);
        let workdir = input["workdir"].as_str();

        // Get the code/command to execute
        let (code, is_code) = if let Some(c) = input["code"].as_str() {
            (c.to_string(), true)
        } else if let Some(c) = input["command"].as_str() {
            (c.to_string(), false)
        } else {
            return Err(AgentError::ToolExecution(
                "either 'code' or 'command' is required".to_string(),
            ));
        };

        // Build description for permission
        let desc = if is_code {
            format!("sandbox_exec: {language} code ({} chars)", code.len())
        } else {
            format!("sandbox_exec: {code}")
        };

        // Request permission
        if let Some(perms) = permissions {
            let approved = perms
                .request(
                    "sandbox_exec",
                    PermissionAction::Execute,
                    PermissionContext::Bash {
                        command: desc,
                        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
                    },
                )
                .await
                .map_err(|e| AgentError::ToolExecution(e.to_string()))?;

            if !approved {
                return Err(AgentError::ToolExecution(
                    "Permission denied by user".to_string(),
                ));
            }
        }

        // Check if Docker is available
        let docker_available = Command::new("docker")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|s| s.success());

        if docker_available {
            self.execute_sandbox_docker(language, &code, is_code, timeout_secs, network, workdir)
                .await
        } else {
            self.execute_sandbox_fallback(language, &code, is_code, timeout_secs)
                .await
        }
    }

    async fn execute_sandbox_docker(
        &self,
        language: &str,
        code: &str,
        is_code: bool,
        timeout_secs: u64,
        network: bool,
        workdir: Option<&str>,
    ) -> Result<String> {
        // Select appropriate Docker image
        let image = match language {
            "python" => "python:3.12-slim",
            "node" => "node:20-slim",
            "ruby" => "ruby:3.3-slim",
            _ => "alpine:latest",
        };

        let mut cmd = Command::new("docker");
        cmd.args(["run", "--rm"]);

        // Resource limits
        cmd.args(["--memory", "256m"]);
        cmd.args(["--cpus", "0.5"]);
        cmd.args(["--pids-limit", "64"]);

        // Network isolation
        if !network {
            cmd.arg("--network=none");
        }

        // Security options
        cmd.args(["--security-opt", "no-new-privileges"]);
        cmd.args(["--cap-drop", "ALL"]);

        // Mount workdir read-only if specified
        if let Some(dir) = workdir {
            cmd.arg("-v");
            cmd.arg(format!("{dir}:/workspace:ro"));
            cmd.args(["-w", "/workspace"]);
        }

        cmd.arg(image);

        // Build the execution command
        if is_code {
            match language {
                "python" => {
                    cmd.args(["python", "-c", code]);
                }
                "node" => {
                    cmd.args(["node", "-e", code]);
                }
                "ruby" => {
                    cmd.args(["ruby", "-e", code]);
                }
                _ => {
                    cmd.args(["sh", "-c", code]);
                }
            }
        } else {
            cmd.args(["sh", "-c", code]);
        }

        // Run with timeout
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output(),
        )
        .await
        .map_err(|_| {
            AgentError::ToolExecution(format!("sandbox execution timed out after {timeout_secs}s"))
        })?
        .map_err(|e| AgentError::ToolExecution(format!("failed to run docker: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "[sandbox:docker] Execution successful\n\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
            ))
        } else {
            Ok(format!(
                "[sandbox:docker] Execution failed (exit {})\n\nstdout:\n{stdout}\n\nstderr:\n{stderr}",
                output.status.code().unwrap_or(-1)
            ))
        }
    }

    async fn execute_sandbox_fallback(
        &self,
        language: &str,
        code: &str,
        is_code: bool,
        timeout_secs: u64,
    ) -> Result<String> {
        // Fallback: use timeout command with restricted env
        let mut cmd = Command::new("timeout");
        cmd.arg(format!("{timeout_secs}s"));

        // Use nice to lower priority
        cmd.args(["nice", "-n", "19"]);

        if is_code {
            match language {
                "python" => {
                    cmd.args(["python3", "-c", code]);
                }
                "node" => {
                    cmd.args(["node", "-e", code]);
                }
                "ruby" => {
                    cmd.args(["ruby", "-e", code]);
                }
                _ => {
                    cmd.args(["sh", "-c", code]);
                }
            }
        } else {
            cmd.args(["sh", "-c", code]);
        }

        // Clear potentially dangerous env vars
        cmd.env_clear();
        cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin");
        cmd.env("HOME", "/tmp");
        cmd.env("TMPDIR", "/tmp");

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AgentError::ToolExecution(format!("failed to execute: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check for timeout (exit code 124)
        if output.status.code() == Some(124) {
            return Err(AgentError::ToolExecution(format!(
                "sandbox execution timed out after {timeout_secs}s"
            )));
        }

        if output.status.success() {
            Ok(format!(
                "[sandbox:fallback] Execution successful (Docker not available)\n\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
            ))
        } else {
            Ok(format!(
                "[sandbox:fallback] Execution failed (exit {})\n\nstdout:\n{stdout}\n\nstderr:\n{stderr}",
                output.status.code().unwrap_or(-1)
            ))
        }
    }
}

/// Generate a unified diff between old and new content
fn generate_diff(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        output.push_str(sign);
        output.push_str(change.value());
        if !change.value().ends_with('\n') {
            output.push('\n');
        }
    }

    output
}

/// Convert HTML to plain text by stripping tags and decoding entities
fn html_to_text(html: &str) -> String {
    // Remove script and style elements
    let text = if let Ok(re) = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>") {
        re.replace_all(html, "").into_owned()
    } else {
        html.to_string()
    };

    let text = if let Ok(re) = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>") {
        re.replace_all(&text, "").into_owned()
    } else {
        text
    };

    // Replace block elements with newlines
    let text = if let Ok(re) = regex::Regex::new(r"(?i)<(br|p|div|h[1-6]|li|tr)[^>]*>") {
        re.replace_all(&text, "\n").into_owned()
    } else {
        text
    };

    // Remove all other tags
    let text = if let Ok(re) = regex::Regex::new(r"<[^>]+>") {
        re.replace_all(&text, "").into_owned()
    } else {
        text
    };

    // Decode common HTML entities
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");

    // Collapse multiple whitespace and trim
    let text = if let Ok(re) = regex::Regex::new(r"[ \t]+") {
        re.replace_all(&text, " ").into_owned()
    } else {
        text
    };

    let text = if let Ok(re) = regex::Regex::new(r"\n{3,}") {
        re.replace_all(&text, "\n\n").into_owned()
    } else {
        text
    };

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shell_tool_executes_command() {
        let registry = ToolRegistry::new();
        let plan_manager = PlanManager::new();
        let result = registry
            .execute(
                "shell",
                serde_json::json!({"command": "echo hello"}),
                None,
                AgentMode::Build,
                &plan_manager,
            )
            .await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("hello"));
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let registry = ToolRegistry::new();
        let plan_manager = PlanManager::new();
        let result = registry
            .execute(
                "nonexistent",
                serde_json::json!({}),
                None,
                AgentMode::Build,
                &plan_manager,
            )
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn is_read_only_detects_safe_commands() {
        assert!(is_read_only("ls"));
        assert!(is_read_only("ls -la"));
        assert!(is_read_only("cat /etc/passwd"));
        assert!(is_read_only("head -n 10 file.txt"));
        assert!(is_read_only("tail -f log.txt"));
        assert!(is_read_only("grep pattern file"));
        assert!(is_read_only("find . -name '*.rs'"));
        assert!(is_read_only("pwd"));
        assert!(is_read_only("echo hello"));
        assert!(is_read_only("git status"));
        assert!(is_read_only("git log"));
        assert!(is_read_only("git diff"));
        assert!(is_read_only("cargo check"));
    }

    #[test]
    fn is_read_only_detects_write_commands() {
        assert!(!is_read_only("rm file.txt"));
        assert!(!is_read_only("rm -rf /tmp"));
        assert!(!is_read_only("mv file1 file2"));
        assert!(!is_read_only("cp file1 file2"));
        assert!(!is_read_only("mkdir newdir"));
        assert!(!is_read_only("touch newfile"));
        assert!(!is_read_only("chmod 755 file"));
        assert!(!is_read_only("chown user file"));
        assert!(!is_read_only("git commit -m 'msg'"));
        assert!(!is_read_only("git push"));
        assert!(!is_read_only("cargo build"));
        assert!(!is_read_only("npm install"));
    }

    #[test]
    fn is_read_only_handles_multi_word_subcommands() {
        // Git stash: only "stash list" is read-only
        assert!(is_read_only("git stash list"));
        assert!(!is_read_only("git stash"));
        assert!(!is_read_only("git stash drop"));
        assert!(!is_read_only("git stash pop"));
        assert!(!is_read_only("git stash push"));
        assert!(!is_read_only("git stash apply"));

        // Cargo fmt: only with --check is read-only
        assert!(is_read_only("cargo fmt --check"));
        assert!(is_read_only("cargo fmt --all --check"));
        assert!(is_read_only("cargo fmt --write=false"));
        assert!(!is_read_only("cargo fmt"));
        assert!(!is_read_only("cargo fmt --all"));

        // Cargo test: only with --no-run is read-only
        assert!(is_read_only("cargo test --no-run"));
        assert!(is_read_only("cargo test --all --no-run"));
        assert!(!is_read_only("cargo test"));
        assert!(!is_read_only("cargo test --all"));
    }
}
