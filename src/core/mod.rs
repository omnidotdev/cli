//! Core business logic shared across CLI, TUI, and API.

pub mod agent;
pub mod context;
mod error;
pub mod keychain;
pub mod lsp;
pub mod mcp;
pub mod memory;
pub mod plugin;
pub mod project;
pub mod search;
pub mod secret;
pub mod session;
pub mod shell;
pub mod skill;
pub mod snapshot;
pub mod storage;
pub mod watcher;
pub mod worktree;

pub use agent::Agent;
pub use context::ProjectContext;
pub use error::{Error, Result};

/// Execute an agentic task.
///
/// This is the main entry point for agentic operations, shared by all interfaces.
///
/// # Errors
///
/// Returns an error if the task execution fails.
#[allow(clippy::unused_async)] // TODO: Will use async when fully implemented.
pub async fn execute_task(prompt: &str) -> Result<TaskResult> {
    // TODO: Implement agentic task execution
    tracing::info!(prompt = %prompt, "executing task");

    Ok(TaskResult {
        success: true,
        output: format!("Executed: {prompt}"),
    })
}

/// Result of an agentic task execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct TaskResult {
    /// Whether the task succeeded.
    pub success: bool,
    /// The task output.
    pub output: String,
}
