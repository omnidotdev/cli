//! TUI application state.

use rand::prelude::IndexedRandom;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::components::{ModelSelectionDialog, SessionListDialog};
use super::message::{format_tool_invocation, DisplayMessage};
use super::state::ViewState;

/// Type alias for model fetch results receiver.
type ModelsFetchRx = mpsc::UnboundedReceiver<Vec<(String, Vec<crate::config::ModelInfo>)>>;

/// ASCII art logo lines (main text).
pub const LOGO_LINES: &[&str] = &[
    "  █▀▀█ █▀▄▀█ █▀▀▄ ▀█▀",
    "  █  █ █ █ █ █  █  █ ",
    "  ▀▀▀▀ ▀   ▀ ▀  ▀ ▀▀▀",
];

/// Shadow lines (offset down-right from main).
pub const LOGO_SHADOW: &[&str] = &[
    "  ░░░░ ░░░░░ ░░░░ ░░░",
    "  ░  ░ ░ ░ ░ ░  ░  ░ ",
    "  ░░░░ ░   ░ ░  ░ ░░░",
];

/// Rotating taglines for the welcome screen.
const TAGLINES: &[&str] = &[
    "let's ride",
    "let's begin",
    "let's cook",
    "let's create",
    "let's build",
    "let's go",
];

/// Ecosystem tips for the welcome screen
pub const ECOSYSTEM_TIPS: &[&str] = &[
    "Try Runa: drag-and-drop kanban boards for your work · runa.omni.dev",
    "Try Backfeed: let your community shape your roadmap · backfeed.omni.dev",
    "Omni is open source · github.com/omnidotdev",
    "You're doing great, mass hallucination or not",
    "Remember: if the code works, it works",
    "Tip: the bugs are features you haven't documented yet",
    "Friendly reminder: commit early, commit often, blame later",
];

use crate::config::{AgentConfig, AgentPermissions, Config};
use crate::core::agent::{
    AgentMode, AskUserResponse, InterfaceMessage, PermissionAction, PermissionContext,
    PermissionResponse,
};
use crate::core::session::{SessionManager, SessionTarget};
use crate::core::Agent;

/// Active text selection state.
#[derive(Debug, Clone)]
pub struct Selection {
    /// Starting screen Y coordinate.
    pub start_y: u16,
    /// Current end Y coordinate.
    pub end_y: u16,
}

impl Selection {
    /// Get the selection bounds as (`min_y`, `max_y`).
    #[must_use]
    pub fn bounds(&self) -> (u16, u16) {
        (self.start_y.min(self.end_y), self.start_y.max(self.end_y))
    }
}

/// Message from the chat task.
pub enum ChatMessage {
    /// Text chunk to append
    Text(String),
    /// Tool starting (for activity status)
    ToolStart { name: String },
    /// Tool invocation with name, args, output, and error status
    Tool {
        name: String,
        invocation: String,
        output: String,
        is_error: bool,
    },
    /// Token usage and cost information
    Usage {
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
    },
    /// Chat completed, returning the agent
    Done(Agent),
    /// Error occurred, returning the agent
    Error(String, Agent),
}

/// Active permission dialog state.
pub struct ActivePermissionDialog {
    /// Unique request ID.
    pub request_id: Uuid,
    /// Tool requesting permission.
    pub tool_name: String,
    /// Action being requested.
    pub action: PermissionAction,
    /// Context for display.
    pub context: PermissionContext,
    /// Currently selected button (0=Allow, 1=Session, 2=Deny).
    pub selected: usize,
}

/// Active `ask_user` dialog state.
pub struct ActiveAskUserDialog {
    /// Unique request ID.
    pub request_id: Uuid,
    /// Question text.
    pub question: String,
    /// Predefined options, if any.
    pub options: Option<Vec<String>>,
    /// Selected option index (if options provided).
    pub selected: usize,
    /// User input (if no options).
    pub input: String,
    /// Cursor position in input.
    pub cursor: usize,
}

/// Currently active dialog, if any.
pub enum ActiveDialog {
    Permission(ActivePermissionDialog),
    AskUser(ActiveAskUserDialog),
    SessionList(SessionListDialog),
    ModelSelection(ModelSelectionDialog),
    NoProvider,
}

/// Application state for the TUI.
#[allow(clippy::struct_excessive_bools)]
pub struct App {
    /// Current input buffer.
    pub input: String,

    /// Cursor position (byte offset).
    pub cursor: usize,

    /// Current output display (legacy, for streaming text).
    pub output: String,

    /// Scroll offset for output (line number).
    pub scroll_offset: u16,

    /// Whether auto-scroll is enabled (follows new content).
    pub auto_scroll: bool,

    /// Whether a request is in progress.
    pub loading: bool,

    /// Whether to show the welcome screen with logo.
    pub show_welcome: bool,

    /// Selected tagline for the welcome screen.
    pub tagline: &'static str,

    /// Selected ecosystem tip for the welcome screen.
    pub tip: &'static str,

    /// Selected placeholder for the input prompt.
    pub placeholder: &'static str,

    /// The agent (if configured).
    pub agent: Option<Agent>,

    /// Receiver for streaming chat messages.
    pub chat_rx: Option<mpsc::UnboundedReceiver<ChatMessage>>,

    /// Active dialog, if any.
    pub active_dialog: Option<ActiveDialog>,

    /// Receiver for interface messages from permission system.
    pub interface_rx: Option<mpsc::UnboundedReceiver<InterfaceMessage>>,

    /// Sender for permission responses.
    #[allow(clippy::type_complexity)]
    pub permission_response_tx: Option<
        mpsc::UnboundedSender<(
            uuid::Uuid,
            String,
            String,
            PermissionAction,
            PermissionResponse,
        )>,
    >,

    /// Sender for `ask_user` responses.
    pub ask_user_response_tx: Option<mpsc::UnboundedSender<(uuid::Uuid, AskUserResponse)>>,

    /// Current view state (Welcome or Session).
    pub view_state: ViewState,

    /// Conversation messages for display.
    pub messages: Vec<DisplayMessage>,

    /// Currently streaming assistant text (accumulated before adding to messages).
    pub streaming_text: String,

    /// Scroll offset for the message list.
    pub message_scroll: u16,

    /// Current model name for display.
    pub model: String,

    /// Session token usage (cumulative).
    pub session_tokens: (u32, u32),

    /// Session cost in USD (cumulative).
    pub session_cost: f64,

    /// Current agent mode (Build or Plan).
    pub agent_mode: AgentMode,

    /// Active text selection, if any.
    pub selection: Option<Selection>,

    /// Text collected from current selection (populated during render).
    pub selected_text: String,

    /// Terminal width (updated on render).
    pub term_width: u16,

    /// Terminal height (updated on render).
    pub term_height: u16,

    /// Calculated max scroll for message list.
    pub max_message_scroll: u16,

    /// Whether command dropdown is visible.
    pub show_command_dropdown: bool,

    /// Currently selected command index in dropdown.
    pub command_selection: usize,

    /// Agent configuration for permission presets.
    pub agent_config: AgentConfig,

    /// Current activity status (e.g., "Using Bash..." or "Thinking...")
    pub activity_status: Option<String>,

    /// Current provider name.
    pub provider: String,

    /// Cached provider models (fetched at startup).
    pub cached_provider_models: Option<Vec<(String, Vec<crate::config::ModelInfo>)>>,

    /// Receiver for model fetch results.
    pub models_rx: Option<ModelsFetchRx>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Create a new application state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_session_target(SessionTarget::default())
    }

    /// Create a new application state with a specific session target.
    #[must_use]
    pub fn with_session_target(target: SessionTarget) -> Self {
        let config = Config::load().unwrap_or_default();
        let model = config.agent.model.clone();

        let mut agent = config.agent.create_provider().ok().map(|provider| {
            Agent::with_context(provider, &config.agent.model, config.agent.max_tokens, None)
        });

        // Track if we're resuming a session
        let mut session_resumed = false;
        let mut display_messages = Vec::new();

        // Enable session persistence with target
        if let Some(ref mut a) = agent {
            match a.enable_sessions_with_target(target) {
                Ok(session_id) => {
                    // Check if we loaded any messages (resuming)
                    if !a.conversation_is_empty() {
                        session_resumed = true;
                        // Load display messages
                        if let Some(manager) = a.session_manager() {
                            if let Ok(msgs) = Self::load_session_messages(manager, &session_id) {
                                display_messages = msgs;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to enable sessions: {e}");
                }
            }
        }

        // Load persisted mode and apply to agent
        let persisted_mode = crate::config::Config::load_mode();
        if let Some(ref mut a) = agent {
            if persisted_mode != AgentMode::default() {
                a.switch_mode(persisted_mode, None);
            }
        }

        // Pick a random tagline, tip, and placeholder
        let tagline = TAGLINES
            .choose(&mut rand::rng())
            .copied()
            .unwrap_or("let's create");
        let tip = if config.tui.tips {
            ECOSYSTEM_TIPS
                .choose(&mut rand::rng())
                .copied()
                .unwrap_or("")
        } else {
            ""
        };
        let placeholder = if agent.is_none() {
            "configure a provider to get started..."
        } else {
            super::components::PLACEHOLDERS
                .choose(&mut rand::rng())
                .copied()
                .unwrap_or("ask anything...")
        };

        let output = if agent.is_none() {
            "No provider configured. Set an API key or add credentials to ~/.config/omni/config.toml".to_string()
        } else if session_resumed {
            "Session resumed.".to_string()
        } else {
            String::new()
        };

        // If resuming with messages, start in session view
        let (view_state, show_welcome) = if session_resumed && !display_messages.is_empty() {
            (ViewState::Session, false)
        } else {
            (ViewState::Welcome, true)
        };

        let provider = agent
            .as_ref()
            .map(|a| a.provider_name().to_string())
            .unwrap_or_default();

        Self {
            input: String::new(),
            cursor: 0,
            output,
            scroll_offset: 0,
            auto_scroll: true,
            loading: false,
            show_welcome,
            tagline,
            tip,
            placeholder,
            agent,
            chat_rx: None,
            active_dialog: None,
            interface_rx: None,
            permission_response_tx: None,
            ask_user_response_tx: None,
            view_state,
            messages: display_messages,
            streaming_text: String::new(),
            message_scroll: 0,
            model,
            session_tokens: (0, 0),
            session_cost: 0.0,
            agent_mode: persisted_mode,
            selection: None,
            selected_text: String::new(),
            term_width: 80,
            term_height: 24,
            max_message_scroll: 0,
            show_command_dropdown: false,
            command_selection: 0,
            agent_config: config.agent,
            activity_status: None,
            provider,
            cached_provider_models: None,
            models_rx: None,
        }
    }

    /// Get permission presets for the current agent mode.
    #[must_use]
    pub fn current_permissions(&self) -> AgentPermissions {
        let agent_name = match self.agent_mode {
            AgentMode::Build => "build",
            AgentMode::Plan => "plan",
        };
        self.agent_config
            .agents
            .get(agent_name)
            .map(|a| a.permissions.clone())
            .unwrap_or_default()
    }

    /// Sync agent mode from the agent (call after mode changes)
    pub fn sync_agent_mode(&mut self) {
        if let Some(agent) = &self.agent {
            self.agent_mode = agent.mode();
            // Persist mode to disk
            if let Err(e) = crate::config::Config::save_mode(self.agent_mode) {
                tracing::warn!("failed to save mode: {e}");
            }
        }
    }

    /// Delete from cursor to beginning of line.
    pub fn delete_to_start(&mut self) {
        self.input.drain(..self.cursor);
        self.cursor = 0;
    }

    /// Delete from cursor to end of line.
    pub fn delete_to_end(&mut self) {
        self.input.truncate(self.cursor);
    }

    /// Delete word before cursor.
    pub fn delete_word(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Find start of word (skip trailing spaces, then skip word chars)
        let before = &self.input[..self.cursor];
        let trimmed = before.trim_end();

        let word_start = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map_or(0, |i| i + 1);

        self.input.drain(word_start..self.cursor);
        self.cursor = word_start;
    }

    /// Move cursor left by one character.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            // Find previous char boundary
            self.cursor = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
        }
    }

    /// Move cursor right by one character.
    pub fn move_right(&mut self) {
        if self.cursor < self.input.len() {
            // Find next char boundary
            self.cursor = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map_or(self.input.len(), |(i, _)| self.cursor + i);
        }
    }

    /// Move cursor left by one word.
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let before = &self.input[..self.cursor];
        let mut chars: Vec<(usize, char)> = before.char_indices().collect();
        chars.reverse();

        // Skip whitespace
        while let Some(&(_, c)) = chars.first() {
            if !c.is_whitespace() {
                break;
            }
            chars.remove(0);
        }

        // Skip word characters
        while let Some(&(i, c)) = chars.first() {
            if c.is_whitespace() {
                self.cursor = i + c.len_utf8();
                return;
            }
            chars.remove(0);
        }

        self.cursor = 0;
    }

    /// Move cursor right by one word.
    pub fn move_word_right(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }

        let after = &self.input[self.cursor..];
        let mut chars = after.char_indices().peekable();

        // Skip current word characters
        while let Some(&(_, c)) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            chars.next();
        }

        // Skip whitespace
        while let Some(&(_, c)) = chars.peek() {
            if !c.is_whitespace() {
                break;
            }
            chars.next();
        }

        self.cursor = chars
            .peek()
            .map_or(self.input.len(), |&(i, _)| self.cursor + i);
    }

    /// Get the current cursor position as (`line_index`, `column`)
    ///
    /// Line index is 0-based, column is the character count from line start
    #[must_use]
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let before_cursor = &self.input[..self.cursor];
        let line_index = before_cursor.matches('\n').count();
        let line_start = before_cursor.rfind('\n').map_or(0, |i| i + 1);
        let column = before_cursor[line_start..].chars().count();
        (line_index, column)
    }

    /// Check if input contains multiple lines.
    #[must_use]
    pub fn is_multiline(&self) -> bool {
        self.input.contains('\n')
    }

    /// Move cursor up one line, preserving column position.
    pub fn move_up(&mut self) {
        let (line, col) = self.cursor_line_col();
        if line == 0 {
            return;
        }

        // Find start of current line
        let current_line_start = self.input[..self.cursor].rfind('\n').map_or(0, |i| i + 1);

        // Find start of previous line
        let prev_line_start = if current_line_start > 0 {
            self.input[..current_line_start - 1]
                .rfind('\n')
                .map_or(0, |i| i + 1)
        } else {
            0
        };

        // Previous line content (without newline)
        let prev_line_end = current_line_start - 1;
        let prev_line = &self.input[prev_line_start..prev_line_end];
        let prev_line_len = prev_line.chars().count();

        // Move to same column or end of previous line
        let target_col = col.min(prev_line_len);
        self.cursor = prev_line_start
            + prev_line
                .char_indices()
                .nth(target_col)
                .map_or(prev_line.len(), |(i, _)| i);
    }

    /// Move cursor down one line, preserving column position.
    pub fn move_down(&mut self) {
        let (line, col) = self.cursor_line_col();
        let total_lines = self.input.matches('\n').count() + 1;
        if line >= total_lines - 1 {
            return;
        }

        // Find end of current line (position of newline)
        let next_newline = self.input[self.cursor..]
            .find('\n')
            .map(|i| self.cursor + i);

        let Some(newline_pos) = next_newline else {
            return;
        };

        // Next line starts after the newline
        let next_line_start = newline_pos + 1;

        // Find end of next line
        let next_line_end = self.input[next_line_start..]
            .find('\n')
            .map_or(self.input.len(), |i| next_line_start + i);

        let next_line = &self.input[next_line_start..next_line_end];
        let next_line_len = next_line.chars().count();

        // Move to same column or end of next line
        let target_col = col.min(next_line_len);
        self.cursor = next_line_start
            + next_line
                .char_indices()
                .nth(target_col)
                .map_or(next_line.len(), |(i, _)| i);
    }

    /// Insert character at cursor.
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete character before cursor.
    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            let prev = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            self.input.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    /// Clear input and reset cursor.
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    /// Show a permission dialog.
    pub fn show_permission_dialog(
        &mut self,
        request_id: Uuid,
        tool_name: String,
        action: PermissionAction,
        context: PermissionContext,
    ) {
        self.active_dialog = Some(ActiveDialog::Permission(ActivePermissionDialog {
            request_id,
            tool_name,
            action,
            context,
            selected: 0,
        }));
    }

    /// Show an `ask_user` dialog.
    pub fn show_ask_user_dialog(
        &mut self,
        request_id: Uuid,
        question: String,
        options: Option<Vec<String>>,
    ) {
        self.active_dialog = Some(ActiveDialog::AskUser(ActiveAskUserDialog {
            request_id,
            question,
            options,
            selected: 0,
            input: String::new(),
            cursor: 0,
        }));
    }

    /// Hide any active dialog.
    pub fn hide_dialog(&mut self) {
        self.active_dialog = None;
    }

    /// Show the session list dialog.
    ///
    /// Does nothing if a chat is in progress (agent is taken during streaming).
    pub fn show_session_list(&mut self) {
        // Don't allow session switching while streaming
        if self.agent.is_none() {
            return;
        }

        match SessionManager::for_current_project() {
            Ok(manager) => match SessionListDialog::from_manager(&manager) {
                Ok(dialog) => {
                    self.active_dialog = Some(ActiveDialog::SessionList(dialog));
                }
                Err(e) => {
                    tracing::warn!("failed to load sessions: {e}");
                }
            },
            Err(e) => {
                tracing::warn!("failed to initialize session manager: {e}");
            }
        }
    }

    pub fn show_model_selection_dialog(&mut self) {
        use crate::core::keychain;
        use std::collections::HashMap;

        if self.agent.is_none() {
            self.active_dialog = Some(ActiveDialog::NoProvider);
            return;
        }

        if let Some(ref models) = self.cached_provider_models {
            if !models.is_empty() {
                let dialog = ModelSelectionDialog::new(models.clone());
                self.active_dialog = Some(ActiveDialog::ModelSelection(dialog));
                return;
            }
        }

        let mut provider_models: HashMap<String, Vec<crate::config::ModelInfo>> = HashMap::new();

        for model in &self.agent_config.models {
            let provider_name = &model.provider;

            // Check env var first (avoids Keychain prompt when env is set)
            let has_env_key = self
                .agent_config
                .providers
                .get(provider_name)
                .and_then(|p| p.api_key_env.as_ref())
                .is_some_and(|env| std::env::var(env).is_ok());
            // Only check keychain if no env var exists
            let has_keychain_key = !has_env_key && keychain::get_api_key(provider_name).is_some();
            let is_local_provider = self
                .agent_config
                .providers
                .get(provider_name)
                .is_some_and(|p| p.base_url.is_some() && p.api_key_env.is_none());

            if has_env_key || has_keychain_key || is_local_provider {
                provider_models
                    .entry(provider_name.clone())
                    .or_default()
                    .push(model.clone());
            }
        }

        if provider_models.is_empty() {
            let config_provider = &self.agent_config.provider;
            let has_env = self
                .agent_config
                .providers
                .get(config_provider)
                .and_then(|p| p.api_key_env.as_ref())
                .is_some_and(|env| std::env::var(env).is_ok());
            let has_keychain = !has_env && keychain::get_api_key(config_provider).is_some();

            if has_env || has_keychain {
                provider_models.insert(
                    config_provider.clone(),
                    vec![crate::config::ModelInfo {
                        id: self.model.clone(),
                        provider: config_provider.clone(),
                    }],
                );
            }
        }

        if provider_models.is_empty() {
            self.active_dialog = Some(ActiveDialog::NoProvider);
            return;
        }

        let mut sorted: Vec<(String, Vec<crate::config::ModelInfo>)> =
            provider_models.into_iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let dialog = ModelSelectionDialog::new(sorted);
        self.active_dialog = Some(ActiveDialog::ModelSelection(dialog));
    }

    #[must_use]
    pub const fn has_dialog(&self) -> bool {
        self.active_dialog.is_some()
    }

    /// Scroll up by the given number of lines.
    pub const fn scroll_up(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.auto_scroll = false;
    }

    /// Scroll down by the given number of lines.
    pub fn scroll_down(&mut self, lines: u16, max_scroll: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines).min(max_scroll);
        // Re-enable auto-scroll if we're at the bottom
        if self.scroll_offset >= max_scroll {
            self.auto_scroll = true;
        }
    }

    /// Scroll to the bottom and enable auto-scroll.
    pub const fn scroll_to_bottom(&mut self, max_scroll: u16) {
        self.scroll_offset = max_scroll;
        self.auto_scroll = true;
    }

    /// Count the number of lines in the output.
    #[must_use]
    pub fn output_line_count(&self) -> usize {
        self.output.lines().count()
    }

    /// Transition from Welcome to Session view.
    pub const fn enter_session(&mut self) {
        self.view_state = ViewState::Session;
        self.show_welcome = false;
    }

    /// Add a user message to the conversation.
    pub fn add_user_message(&mut self, text: impl Into<String>) {
        self.messages.push(DisplayMessage::user(text));
    }

    /// Add an assistant message to the conversation.
    pub fn add_assistant_message(&mut self, text: impl Into<String>) {
        self.messages.push(DisplayMessage::assistant(text));
    }

    /// Add a tool message to the conversation.
    pub fn add_tool_message(&mut self, message: DisplayMessage) {
        self.messages.push(message);
    }

    /// Finalize streaming text into an assistant message.
    pub fn finalize_streaming(&mut self) {
        if !self.streaming_text.is_empty() {
            let text = std::mem::take(&mut self.streaming_text);
            self.add_assistant_message(text);
        }
    }

    /// Clear the conversation.
    pub fn clear_conversation(&mut self) {
        self.messages.clear();
        self.streaming_text.clear();
        self.message_scroll = 0;
    }

    /// Scroll the message list up.
    pub const fn scroll_messages_up(&mut self, lines: u16) {
        self.message_scroll = self.message_scroll.saturating_sub(lines);
        self.auto_scroll = false;
    }

    /// Scroll the message list down.
    pub fn scroll_messages_down(&mut self, lines: u16) {
        self.message_scroll = self
            .message_scroll
            .saturating_add(lines)
            .min(self.max_message_scroll);
        if self.message_scroll >= self.max_message_scroll {
            self.auto_scroll = true;
        }
    }

    /// Update terminal dimensions and recalculate max scroll.
    #[allow(clippy::cast_possible_truncation)]
    pub fn update_dimensions(&mut self, width: u16, height: u16, content_height: u16) {
        self.term_width = width;
        self.term_height = height;
        // Calculate visible message area height (subtract prompt area)
        // Must match render_session: (input_lines + 3).clamp(4, 13)
        let input_lines = self.input.lines().count().max(1) as u16;
        let input_lines = if self.input.ends_with('\n') {
            input_lines + 1
        } else {
            input_lines
        };
        let prompt_height = (input_lines + 3).clamp(4, 13);
        let visible_height = height.saturating_sub(prompt_height);
        self.max_message_scroll = content_height.saturating_sub(visible_height);

        // Auto-scroll to bottom when enabled
        if self.auto_scroll {
            self.message_scroll = self.max_message_scroll;
        }
    }

    /// Load messages from a session into display format.
    ///
    /// # Errors
    ///
    /// Returns error if session loading fails.
    pub fn load_session_messages(
        manager: &SessionManager,
        session_id: &str,
    ) -> anyhow::Result<Vec<DisplayMessage>> {
        use crate::core::session::{Message as SessionMessage, Part, ToolState};

        let mut display_messages = Vec::new();
        let messages = manager.list_messages(session_id)?;

        for msg in messages {
            let parts = manager.list_parts(msg.id())?;

            match msg {
                SessionMessage::User(_) => {
                    // Collect text parts into user message
                    let text: String = parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::Text(t) => Some(t.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    if !text.is_empty() {
                        display_messages.push(DisplayMessage::User {
                            text,
                            timestamp: None,
                        });
                    }
                }
                SessionMessage::Assistant(_) => {
                    // Process parts in order
                    let mut assistant_text = String::new();

                    for part in parts {
                        match part {
                            Part::Text(t) => {
                                assistant_text.push_str(&t.text);
                            }
                            Part::Tool(t) => {
                                // Flush accumulated assistant text first
                                if !assistant_text.is_empty() {
                                    display_messages.push(DisplayMessage::assistant(
                                        std::mem::take(&mut assistant_text),
                                    ));
                                }

                                // Add tool message
                                let invocation = format_tool_invocation(
                                    &t.tool,
                                    match &t.state {
                                        ToolState::Pending { input, .. }
                                        | ToolState::Running { input, .. }
                                        | ToolState::Completed { input, .. }
                                        | ToolState::Error { input, .. } => input,
                                    },
                                );

                                let (output, is_error) = match &t.state {
                                    ToolState::Completed {
                                        output, compacted, ..
                                    } => {
                                        if compacted.is_some() {
                                            ("[content cleared]".to_string(), false)
                                        } else {
                                            (output.clone(), false)
                                        }
                                    }
                                    ToolState::Error { error, .. } => (error.clone(), true),
                                    ToolState::Pending { .. } | ToolState::Running { .. } => {
                                        ("...".to_string(), false)
                                    }
                                };

                                display_messages.push(DisplayMessage::tool(
                                    &t.tool, invocation, output, is_error,
                                ));
                            }
                            Part::Reasoning(_) => {
                                // Skip reasoning parts in display for now
                            }
                        }
                    }

                    // Flush remaining assistant text
                    if !assistant_text.is_empty() {
                        display_messages.push(DisplayMessage::assistant(assistant_text));
                    }
                }
            }
        }

        Ok(display_messages)
    }
}
