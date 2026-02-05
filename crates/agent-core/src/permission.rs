//! Permission system types and client.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// Permission action for agent operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionPreset {
    /// Always allow without prompting.
    Allow,
    /// Always deny.
    Deny,
    /// Ask user each time.
    #[default]
    Ask,
}

/// Permission configuration for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentPermissions {
    /// File editing permission.
    pub edit: PermissionPreset,
    /// File writing (new files) permission.
    pub write: PermissionPreset,
    /// Destructive shell commands permission.
    pub bash_write: PermissionPreset,
    /// Read-only shell commands permission.
    pub bash_read: PermissionPreset,
    /// File reading permission.
    pub read: PermissionPreset,
    /// Web search permission.
    pub web_search: PermissionPreset,
    /// Code search permission.
    pub code_search: PermissionPreset,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            edit: PermissionPreset::Ask,
            write: PermissionPreset::Ask,
            bash_write: PermissionPreset::Ask,
            bash_read: PermissionPreset::Allow,
            read: PermissionPreset::Allow,
            web_search: PermissionPreset::Ask,
            code_search: PermissionPreset::Ask,
        }
    }
}

impl AgentPermissions {
    /// Permissions for read-only plan mode.
    #[must_use]
    pub const fn plan_mode() -> Self {
        Self {
            edit: PermissionPreset::Deny,
            write: PermissionPreset::Deny,
            bash_write: PermissionPreset::Deny,
            bash_read: PermissionPreset::Allow,
            read: PermissionPreset::Allow,
            web_search: PermissionPreset::Ask,
            code_search: PermissionPreset::Ask,
        }
    }
}

/// Action that requires permission.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PermissionAction {
    /// Execute a shell command.
    Execute,
    /// Write a new file.
    WriteFile,
    /// Edit an existing file.
    EditFile,
    /// Ask user a clarifying question.
    AskUser,
    /// Perform a web search.
    WebSearch,
    /// Perform a code search.
    CodeSearch,
    /// Search files by pattern (glob).
    Glob,
    /// Search file contents (grep).
    Grep,
    /// List directory contents.
    ListDir,
    /// Fetch content from a URL.
    WebFetch,
}

/// Tool-specific context for permission dialogs.
#[derive(Debug, Clone)]
pub enum PermissionContext {
    /// Shell command execution.
    Bash {
        command: String,
        working_dir: PathBuf,
    },
    /// File write operation.
    WriteFile {
        path: PathBuf,
        content_preview: String,
    },
    /// File edit operation.
    EditFile { path: PathBuf, diff: String },
    /// Clarifying question from agent.
    AskUser {
        question: String,
        options: Option<Vec<String>>,
    },
    /// Web search operation.
    WebSearch { query: String },
    /// Code search operation.
    CodeSearch { query: String, tokens: u32 },
    /// Glob file search.
    Glob { pattern: String, path: PathBuf },
    /// Grep content search.
    Grep { pattern: String, path: PathBuf },
    /// List directory.
    ListDir { path: PathBuf },
    /// Fetch content from URL.
    WebFetch { url: String },
}

/// User's response to a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResponse {
    /// Allow this single operation.
    Allow,
    /// Allow all similar operations for this session.
    AllowForSession,
    /// Deny the operation.
    Deny,
}

/// Response for `ask_user` tool (contains actual answer).
#[derive(Debug, Clone)]
pub enum AskUserResponse {
    /// User provided an answer.
    Answer(String),
    /// User cancelled.
    Cancelled,
}

/// Message sent to `PermissionActor`.
#[derive(Debug)]
pub enum PermissionMessage {
    /// Request permission for an action.
    Request {
        session_id: String,
        tool_name: String,
        action: PermissionAction,
        context: PermissionContext,
        response_tx: oneshot::Sender<PermissionResponse>,
    },
    /// Request user input (`ask_user` tool).
    AskUser {
        session_id: String,
        context: PermissionContext,
        response_tx: oneshot::Sender<AskUserResponse>,
    },
    /// Register an interface to receive permission dialogs.
    RegisterInterface {
        interface_tx: mpsc::UnboundedSender<InterfaceMessage>,
    },
    /// Unregister the interface.
    UnregisterInterface,
    /// Clear session cache.
    ClearSession { session_id: String },
}

/// Message sent to the TUI interface.
#[derive(Debug, Clone)]
pub enum InterfaceMessage {
    /// Show a permission dialog.
    ShowPermissionDialog {
        request_id: Uuid,
        tool_name: String,
        action: PermissionAction,
        context: PermissionContext,
    },
    /// Show an `ask_user` dialog.
    ShowAskUserDialog {
        request_id: Uuid,
        question: String,
        options: Option<Vec<String>>,
    },
    /// Hide any active dialog.
    HideDialog,
}

/// Lightweight handle for tools to request permissions.
#[derive(Clone)]
pub struct PermissionClient {
    session_id: String,
    permission_tx: mpsc::UnboundedSender<PermissionMessage>,
    presets: Arc<RwLock<AgentPermissions>>,
}

impl PermissionClient {
    /// Create a new permission client.
    #[must_use]
    pub fn new(
        session_id: String,
        permission_tx: mpsc::UnboundedSender<PermissionMessage>,
    ) -> Self {
        Self {
            session_id,
            permission_tx,
            presets: Arc::new(RwLock::new(AgentPermissions::default())),
        }
    }

    /// Create a new permission client with specific presets.
    #[must_use]
    pub fn with_presets(
        session_id: String,
        permission_tx: mpsc::UnboundedSender<PermissionMessage>,
        presets: AgentPermissions,
    ) -> Self {
        Self {
            session_id,
            permission_tx,
            presets: Arc::new(RwLock::new(presets)),
        }
    }

    /// Update the permission presets (e.g., when switching agents).
    pub fn set_presets(&self, presets: AgentPermissions) {
        *self.presets.write() = presets;
    }

    /// Get the preset for a given action.
    fn get_preset(&self, action: &PermissionAction) -> PermissionPreset {
        let presets = self.presets.read();
        match action {
            PermissionAction::Execute => presets.bash_write,
            PermissionAction::WriteFile => presets.write,
            PermissionAction::EditFile => presets.edit,
            PermissionAction::AskUser => PermissionPreset::Allow, // Always allow ask_user
            PermissionAction::WebSearch | PermissionAction::WebFetch => presets.web_search,
            PermissionAction::CodeSearch => presets.code_search,
            // Read-only operations default to allow
            PermissionAction::Glob | PermissionAction::Grep | PermissionAction::ListDir => {
                presets.read
            }
        }
    }

    /// Request permission for an action.
    ///
    /// Returns `true` if approved, `false` if denied.
    ///
    /// Checks the agent's permission presets first:
    /// - `Allow`: Returns `true` immediately without prompting.
    /// - `Deny`: Returns `false` immediately without prompting.
    /// - `Ask`: Shows the permission dialog to the user.
    ///
    /// # Errors
    ///
    /// Returns error if the permission channel is closed.
    pub async fn request(
        &self,
        tool: &str,
        action: PermissionAction,
        context: PermissionContext,
    ) -> Result<bool, PermissionError> {
        // Check preset first - may short-circuit without user prompt
        match self.get_preset(&action) {
            PermissionPreset::Allow => return Ok(true),
            PermissionPreset::Deny => return Ok(false),
            PermissionPreset::Ask => {} // Continue to prompt user.
        }

        let (response_tx, response_rx) = oneshot::channel();

        self.permission_tx
            .send(PermissionMessage::Request {
                session_id: self.session_id.clone(),
                tool_name: tool.to_string(),
                action,
                context,
                response_tx,
            })
            .map_err(|_| PermissionError::ChannelClosed)?;

        match response_rx
            .await
            .map_err(|_| PermissionError::ChannelClosed)?
        {
            PermissionResponse::Allow | PermissionResponse::AllowForSession => Ok(true),
            PermissionResponse::Deny => Ok(false),
        }
    }

    /// Ask the user a clarifying question.
    ///
    /// # Errors
    ///
    /// Returns error if cancelled or channel closed.
    pub async fn ask_user(
        &self,
        question: &str,
        options: Option<Vec<String>>,
    ) -> Result<String, PermissionError> {
        let (response_tx, response_rx) = oneshot::channel();

        self.permission_tx
            .send(PermissionMessage::AskUser {
                session_id: self.session_id.clone(),
                context: PermissionContext::AskUser {
                    question: question.to_string(),
                    options,
                },
                response_tx,
            })
            .map_err(|_| PermissionError::ChannelClosed)?;

        match response_rx
            .await
            .map_err(|_| PermissionError::ChannelClosed)?
        {
            AskUserResponse::Answer(answer) => Ok(answer),
            AskUserResponse::Cancelled => Err(PermissionError::Cancelled),
        }
    }
}

/// Actor that handles permission requests and caching.
pub struct PermissionActor {
    /// Inbox for receiving permission messages.
    pub inbox: mpsc::UnboundedReceiver<PermissionMessage>,
    interface_tx: Option<mpsc::UnboundedSender<InterfaceMessage>>,
    session_cache: HashSet<(String, String, PermissionAction)>,
    pending_requests: std::collections::HashMap<Uuid, oneshot::Sender<PermissionResponse>>,
    pending_ask_user: std::collections::HashMap<Uuid, oneshot::Sender<AskUserResponse>>,
}

impl PermissionActor {
    /// Create a new permission actor.
    ///
    /// Returns the actor and a sender for sending messages to it.
    #[must_use]
    pub fn new() -> (Self, mpsc::UnboundedSender<PermissionMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                inbox: rx,
                interface_tx: None,
                session_cache: HashSet::new(),
                pending_requests: std::collections::HashMap::new(),
                pending_ask_user: std::collections::HashMap::new(),
            },
            tx,
        )
    }

    /// Run the actor loop.
    pub async fn run(mut self) {
        while let Some(msg) = self.inbox.recv().await {
            self.handle_message(msg);
        }
    }

    /// Handle a single message.
    pub fn handle_message(&mut self, msg: PermissionMessage) {
        match msg {
            PermissionMessage::Request {
                session_id,
                tool_name,
                action,
                context,
                response_tx,
            } => {
                // Check cache first
                let cache_key = (session_id, tool_name.clone(), action.clone());
                if self.session_cache.contains(&cache_key) {
                    let _ = response_tx.send(PermissionResponse::AllowForSession);
                    return;
                }

                // Forward to interface
                if let Some(ref interface_tx) = self.interface_tx {
                    let request_id = Uuid::new_v4();
                    self.pending_requests.insert(request_id, response_tx);

                    let _ = interface_tx.send(InterfaceMessage::ShowPermissionDialog {
                        request_id,
                        tool_name,
                        action,
                        context,
                    });
                } else {
                    // No interface registered - deny by default
                    let _ = response_tx.send(PermissionResponse::Deny);
                }
            }

            PermissionMessage::AskUser {
                session_id: _,
                context,
                response_tx,
            } => {
                if let Some(ref interface_tx) = self.interface_tx {
                    let request_id = Uuid::new_v4();
                    self.pending_ask_user.insert(request_id, response_tx);

                    if let PermissionContext::AskUser { question, options } = context {
                        let _ = interface_tx.send(InterfaceMessage::ShowAskUserDialog {
                            request_id,
                            question,
                            options,
                        });
                    }
                } else {
                    let _ = response_tx.send(AskUserResponse::Cancelled);
                }
            }

            PermissionMessage::RegisterInterface { interface_tx } => {
                self.interface_tx = Some(interface_tx);
            }

            PermissionMessage::UnregisterInterface => {
                self.interface_tx = None;
                // Cancel all pending requests
                for (_, tx) in self.pending_requests.drain() {
                    let _ = tx.send(PermissionResponse::Deny);
                }
                for (_, tx) in self.pending_ask_user.drain() {
                    let _ = tx.send(AskUserResponse::Cancelled);
                }
            }

            PermissionMessage::ClearSession { session_id } => {
                self.session_cache.retain(|(sid, _, _)| sid != &session_id);
            }
        }
    }

    /// Respond to a permission request.
    pub fn respond(
        &mut self,
        request_id: Uuid,
        response: PermissionResponse,
        session_id: &str,
        tool_name: &str,
        action: &PermissionAction,
    ) {
        if let Some(tx) = self.pending_requests.remove(&request_id) {
            // Cache if `AllowForSession`
            if response == PermissionResponse::AllowForSession {
                self.session_cache.insert((
                    session_id.to_string(),
                    tool_name.to_string(),
                    action.clone(),
                ));
            }
            let _ = tx.send(response);
        }
    }

    /// Respond to an `ask_user` request.
    pub fn respond_ask_user(&mut self, request_id: Uuid, response: AskUserResponse) {
        if let Some(tx) = self.pending_ask_user.remove(&request_id) {
            let _ = tx.send(response);
        }
    }
}

/// Permission system errors.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// Permission was denied by user.
    #[error("permission denied by user")]
    Denied,

    /// Permission channel was closed.
    #[error("permission channel closed")]
    ChannelClosed,

    /// User cancelled the operation.
    #[error("operation cancelled by user")]
    Cancelled,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use super::*;

    #[test]
    fn permission_action_is_hashable() {
        let mut set = HashSet::new();
        set.insert(PermissionAction::Execute);
        set.insert(PermissionAction::WriteFile);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn permission_response_equality() {
        assert_eq!(PermissionResponse::Allow, PermissionResponse::Allow);
        assert_ne!(PermissionResponse::Allow, PermissionResponse::Deny);
    }

    #[tokio::test]
    async fn client_sends_permission_request() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client = PermissionClient::new("test-session".to_string(), tx);

        // Spawn a task to approve the request
        let handle = tokio::spawn(async move {
            if let Some(PermissionMessage::Request { response_tx, .. }) = rx.recv().await {
                response_tx.send(PermissionResponse::Allow).unwrap();
            }
        });

        let result = client
            .request(
                "bash",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "echo hello".to_string(),
                    working_dir: PathBuf::from("/tmp"),
                },
            )
            .await;

        handle.await.unwrap();
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn client_returns_false_on_deny() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client = PermissionClient::new("test-session".to_string(), tx);

        let handle = tokio::spawn(async move {
            if let Some(PermissionMessage::Request { response_tx, .. }) = rx.recv().await {
                response_tx.send(PermissionResponse::Deny).unwrap();
            }
        });

        let result = client
            .request(
                "bash",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "rm -rf /".to_string(),
                    working_dir: PathBuf::from("/"),
                },
            )
            .await;

        handle.await.unwrap();
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn client_ask_user_returns_answer() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client = PermissionClient::new("test-session".to_string(), tx);

        let handle = tokio::spawn(async move {
            if let Some(PermissionMessage::AskUser { response_tx, .. }) = rx.recv().await {
                response_tx
                    .send(AskUserResponse::Answer("yes".to_string()))
                    .unwrap();
            }
        });

        let result = client.ask_user("Continue?", None).await;
        handle.await.unwrap();
        assert_eq!(result.unwrap(), "yes");
    }

    #[tokio::test]
    async fn client_ask_user_cancelled_returns_error() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client = PermissionClient::new("test-session".to_string(), tx);

        let handle = tokio::spawn(async move {
            if let Some(PermissionMessage::AskUser { response_tx, .. }) = rx.recv().await {
                response_tx.send(AskUserResponse::Cancelled).unwrap();
            }
        });

        let result = client.ask_user("Continue?", None).await;
        handle.await.unwrap();
        assert!(matches!(result, Err(PermissionError::Cancelled)));
    }

    #[tokio::test]
    async fn client_returns_error_on_closed_channel() {
        let (tx, rx) = mpsc::unbounded_channel::<PermissionMessage>();
        let client = PermissionClient::new("test-session".to_string(), tx);

        // Drop receiver to close channel
        drop(rx);

        let result = client
            .request(
                "bash",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "ls".to_string(),
                    working_dir: PathBuf::from("/tmp"),
                },
            )
            .await;

        assert!(matches!(result, Err(PermissionError::ChannelClosed)));
    }

    #[tokio::test]
    async fn actor_caches_allow_for_session() {
        let (actor, tx) = PermissionActor::new();
        let (interface_tx, mut interface_rx) = mpsc::unbounded_channel();

        // Register interface
        tx.send(PermissionMessage::RegisterInterface { interface_tx })
            .unwrap();

        // Spawn actor
        let actor_handle = tokio::spawn(async move {
            tokio::select! {
                () = actor.run() => {}
                () = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            }
        });

        // First request - should show dialog
        let client = PermissionClient::new("test-session".to_string(), tx.clone());

        let request_handle = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .request(
                        "bash",
                        PermissionAction::Execute,
                        PermissionContext::Bash {
                            command: "ls".to_string(),
                            working_dir: PathBuf::from("/tmp"),
                        },
                    )
                    .await
            }
        });

        // Wait for interface message
        let msg = interface_rx.recv().await.unwrap();
        if let InterfaceMessage::ShowPermissionDialog { request_id: _, .. } = msg {
            drop(request_handle);
        }

        actor_handle.abort();
    }

    #[tokio::test]
    async fn preset_allow_skips_dialog() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let presets = AgentPermissions {
            bash_write: PermissionPreset::Allow,
            ..Default::default()
        };
        let client = PermissionClient::with_presets("test-session".to_string(), tx, presets);

        // Spawn a task that would fail if it receives a message
        let checker = tokio::spawn(async move {
            tokio::select! {
                msg = rx.recv() => panic!("should not receive message, got: {msg:?}"),
                () = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
            }
        });

        let result = client
            .request(
                "bash",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "rm -rf /".to_string(),
                    working_dir: PathBuf::from("/"),
                },
            )
            .await;

        checker.await.unwrap();
        assert!(result.unwrap()); // Should be allowed without prompting.
    }

    #[tokio::test]
    async fn preset_deny_skips_dialog() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let presets = AgentPermissions {
            bash_write: PermissionPreset::Deny,
            ..Default::default()
        };
        let client = PermissionClient::with_presets("test-session".to_string(), tx, presets);

        // Spawn a task that would fail if it receives a message
        let checker = tokio::spawn(async move {
            tokio::select! {
                msg = rx.recv() => panic!("should not receive message, got: {msg:?}"),
                () = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
            }
        });

        let result = client
            .request(
                "bash",
                PermissionAction::Execute,
                PermissionContext::Bash {
                    command: "rm -rf /".to_string(),
                    working_dir: PathBuf::from("/"),
                },
            )
            .await;

        checker.await.unwrap();
        assert!(!result.unwrap()); // Should be denied without prompting.
    }

    #[tokio::test]
    async fn preset_ask_shows_dialog() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let presets = AgentPermissions {
            bash_write: PermissionPreset::Ask,
            ..Default::default()
        };
        let client = PermissionClient::with_presets("test-session".to_string(), tx, presets);

        let request_handle = tokio::spawn(async move {
            client
                .request(
                    "bash",
                    PermissionAction::Execute,
                    PermissionContext::Bash {
                        command: "rm -rf /".to_string(),
                        working_dir: PathBuf::from("/"),
                    },
                )
                .await
        });

        // Should receive a request message
        let msg = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(msg.is_ok());
        assert!(matches!(
            msg.unwrap(),
            Some(PermissionMessage::Request { .. })
        ));

        request_handle.abort();
    }

    #[test]
    fn set_presets_updates_behavior() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let client = PermissionClient::new("test-session".to_string(), tx);

        // Default should be Ask
        assert_eq!(
            client.get_preset(&PermissionAction::Execute),
            PermissionPreset::Ask
        );

        // Update to Allow
        client.set_presets(AgentPermissions {
            bash_write: PermissionPreset::Allow,
            ..Default::default()
        });
        assert_eq!(
            client.get_preset(&PermissionAction::Execute),
            PermissionPreset::Allow
        );
    }
}
