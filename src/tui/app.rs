//! TUI application state.

use rand::prelude::IndexedRandom;
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::components::{
    EditBuffer, EditorView, InputAction, ModelSelectionDialog, SessionListDialog,
};
use super::message::{DisplayMessage, format_tool_invocation};
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
use crate::core::Agent;
use crate::core::agent::{
    AgentMode, AskUserResponse, InterfaceMessage, PermissionAction, PermissionContext,
    PermissionResponse, ReasoningEffort,
};
use crate::core::session::{SessionManager, SessionTarget};

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
    /// Thinking started (internal reasoning)
    ThinkingStart,
    /// Thinking chunk to append
    Thinking(String),
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

/// Expanded tool output dialog state.
pub struct ExpandedToolDialog {
    /// Name of the tool (e.g., "shell", "edit").
    pub tool_name: String,
    /// Invocation string (e.g., command or file path).
    pub invocation: String,
    /// Full output content.
    pub output: String,
    /// Current scroll offset for viewing large outputs.
    pub scroll_offset: u16,
    /// Total rendered line count (calculated once for scroll bounds).
    pub total_lines: usize,
    /// Cached rendered lines (avoids re-parsing/highlighting on every frame).
    pub cached_lines: Vec<ratatui::text::Line<'static>>,
    /// Width at which lines were cached (for invalidation on resize).
    pub cached_width: u16,
    /// Visible height for scroll calculations (updated on render).
    pub visible_height: u16,
}

/// Currently active dialog, if any.
pub enum ActiveDialog {
    Permission(ActivePermissionDialog),
    AskUser(ActiveAskUserDialog),
    SessionList(SessionListDialog),
    ModelSelection(ModelSelectionDialog),
    ToolOutput(ExpandedToolDialog),
    NoProvider,
}

/// Application state for the TUI.
#[allow(clippy::struct_excessive_bools)]
pub struct App {
    /// Text input buffer with cursor management.
    pub edit_buffer: EditBuffer,

    /// Editor view for visual cursor and scrolling.
    pub editor_view: EditorView,

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

    /// Currently streaming thinking/reasoning text (accumulated before adding to messages).
    pub streaming_thinking: String,

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

    /// Width of prompt text area for visual line navigation
    pub prompt_text_width: usize,

    /// Scroll offset for prompt content
    pub prompt_scroll_offset: usize,

    /// Prompt area bounds for mouse detection
    pub prompt_area: Option<Rect>,

    /// Command dropdown area bounds for mouse detection
    pub command_dropdown_area: Option<Rect>,

    /// Number of items currently visible in the command dropdown
    pub command_dropdown_item_count: usize,

    /// Tool message areas for click detection (Rect, `message_index`)
    pub tool_message_areas: Vec<(Rect, usize)>,

    /// Current reasoning effort level for thinking-capable models.
    pub reasoning_effort: ReasoningEffort,

    /// Track first Esc press for double-Esc cancellation.
    pub esc_pressed_once: bool,

    /// Track first backspace on empty input for double-backspace delete.
    pub backspace_on_empty_once: bool,

    /// Messages queued while agent is processing.
    pub pending_messages: Vec<String>,
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
            edit_buffer: EditBuffer::new(),
            editor_view: EditorView::new(80),
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
            streaming_thinking: String::new(),
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
            prompt_text_width: 80,
            prompt_scroll_offset: 0,
            prompt_area: None,
            command_dropdown_area: None,
            command_dropdown_item_count: 0,
            tool_message_areas: Vec::new(),
            reasoning_effort: ReasoningEffort::default(),
            esc_pressed_once: false,
            backspace_on_empty_once: false,
            pending_messages: Vec::new(),
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

    #[must_use]
    pub fn input(&self) -> &str {
        self.edit_buffer.text()
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.edit_buffer.cursor()
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.edit_buffer.set_cursor(pos);
    }

    pub fn set_input(&mut self, text: impl Into<String>) {
        let text = text.into();
        let len = text.len();
        self.edit_buffer = EditBuffer::with_text(text);
        self.edit_buffer.set_cursor(len);
    }

    pub fn take_input(&mut self) -> String {
        let text = self.edit_buffer.text().to_string();
        self.edit_buffer.clear();
        text
    }

    pub fn delete_to_start(&mut self) {
        self.edit_buffer.delete_to_start();
    }

    pub fn delete_to_end(&mut self) {
        self.edit_buffer.delete_to_end();
    }

    pub fn delete_word(&mut self) {
        self.edit_buffer.delete_word();
    }

    pub fn move_left(&mut self) {
        self.edit_buffer.move_left();
    }

    pub fn move_right(&mut self) {
        self.edit_buffer.move_right();
    }

    pub fn move_word_left(&mut self) {
        self.edit_buffer.move_word_left();
    }

    pub fn move_word_right(&mut self) {
        self.edit_buffer.move_word_right();
    }

    #[must_use]
    pub fn cursor_line_col(&self) -> (usize, usize) {
        self.edit_buffer.cursor_line_col()
    }

    #[must_use]
    pub fn is_multiline(&self) -> bool {
        self.edit_buffer.is_multiline()
    }

    pub fn move_up(&mut self) {
        self.editor_view.set_width(self.prompt_text_width);
        let layout =
            super::components::TextLayout::new(self.edit_buffer.text(), self.prompt_text_width);
        self.editor_view
            .move_up_visual(&mut self.edit_buffer, &layout);
    }

    pub fn move_down(&mut self) {
        self.editor_view.set_width(self.prompt_text_width);
        let layout =
            super::components::TextLayout::new(self.edit_buffer.text(), self.prompt_text_width);
        self.editor_view
            .move_down_visual(&mut self.edit_buffer, &layout);
    }

    pub fn insert_char(&mut self, c: char) {
        self.edit_buffer.insert_char(c);
    }

    pub fn delete_char(&mut self) {
        self.edit_buffer.delete_char_before();
    }

    pub fn delete_char_after(&mut self) {
        self.edit_buffer.delete_char_after();
    }

    pub fn clear_input(&mut self) {
        self.edit_buffer.clear();
    }

    /// Execute an input action.
    ///
    /// This is the central dispatcher for keybinding-based input operations.
    /// Note: `InsertChar` is a no-op here as it requires the char value.
    pub fn execute_action(&mut self, action: &InputAction) {
        match action {
            InputAction::MoveLeft => self.move_left(),
            InputAction::MoveRight => self.move_right(),
            InputAction::MoveUp => self.move_up(),
            InputAction::MoveDown => self.move_down(),
            InputAction::MoveWordLeft => self.move_word_left(),
            InputAction::MoveWordRight => self.move_word_right(),
            InputAction::MoveToStart => self.set_cursor(0),
            InputAction::MoveToEnd => self.set_cursor(self.edit_buffer.len()),
            InputAction::DeleteCharBefore => self.delete_char(),
            InputAction::DeleteCharAfter => self.delete_char_after(),
            InputAction::InsertNewline => self.insert_char('\n'),
            InputAction::DeleteToStart => self.delete_to_start(),
            InputAction::DeleteToEnd => self.delete_to_end(),
            InputAction::DeleteWord => self.delete_word(),
            InputAction::InsertChar => {} // Handled separately with char value
        }
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
        self.messages
            .push(DisplayMessage::user(text, self.agent_mode));
    }

    /// Add an assistant message to the conversation.
    pub fn add_assistant_message(&mut self, text: impl Into<String>) {
        self.messages.push(DisplayMessage::assistant(text));
    }

    /// Add a tool message to the conversation.
    pub fn add_tool_message(&mut self, message: DisplayMessage) {
        self.messages.push(message);
    }

    #[must_use]
    pub fn queued_message_count(&self) -> usize {
        self.pending_messages.len()
    }

    #[must_use]
    pub fn get_queued_messages(&self) -> &[String] {
        &self.pending_messages
    }

    pub fn add_queued_message(&mut self, text: String) {
        self.pending_messages.push(text);
    }

    pub fn activate_first_queued_message(&mut self) -> Option<String> {
        if self.pending_messages.is_empty() {
            return None;
        }
        Some(self.pending_messages.remove(0))
    }

    pub fn remove_last_queued_message(&mut self) {
        self.pending_messages.pop();
    }

    pub fn clear_queued_messages(&mut self) {
        self.pending_messages.clear();
    }

    /// Finalize streaming text into an assistant message.
    pub fn finalize_streaming(&mut self, cancelled: bool) {
        self.finalize_streaming_thinking();
        if cancelled && !self.streaming_text.is_empty() {
            self.streaming_text.push_str("\n\n--- Cancelled ---");
        }
        if !self.streaming_text.is_empty() {
            let text = std::mem::take(&mut self.streaming_text);
            self.add_assistant_message(text);
        }
    }

    /// Finalize streaming thinking into a reasoning message.
    pub fn finalize_streaming_thinking(&mut self) {
        if !self.streaming_thinking.is_empty() {
            let text = std::mem::take(&mut self.streaming_thinking);
            self.messages.push(DisplayMessage::Reasoning { text });
        }
    }

    /// Clear the conversation.
    pub fn clear_conversation(&mut self) {
        self.messages.clear();
        self.streaming_text.clear();
        self.streaming_thinking.clear();
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

    /// Scroll the prompt up by the given number of lines.
    pub const fn scroll_prompt_up(&mut self, lines: usize) {
        self.prompt_scroll_offset = self.prompt_scroll_offset.saturating_sub(lines);
    }

    /// Scroll the prompt down by the given number of lines.
    pub fn scroll_prompt_down(&mut self, lines: usize, max_scroll: usize) {
        self.prompt_scroll_offset = (self.prompt_scroll_offset + lines).min(max_scroll);
    }

    /// Set the prompt area bounds for mouse detection.
    pub const fn set_prompt_area(&mut self, area: Rect) {
        self.prompt_area = Some(area);
    }

    /// Set the command dropdown area bounds and item count for mouse detection.
    pub const fn set_dropdown_area(&mut self, area: Option<Rect>, item_count: usize) {
        self.command_dropdown_area = area;
        self.command_dropdown_item_count = item_count;
    }

    /// Check if a point is within the prompt area.
    #[must_use]
    pub fn is_in_prompt_area(&self, row: u16, col: u16) -> bool {
        self.prompt_area.is_some_and(|area| {
            row >= area.y
                && row < area.y + area.height
                && col >= area.x
                && col < area.x + area.width
        })
    }

    /// Check if a point is within the command dropdown area.
    /// Returns the item index if clicked on a dropdown item, None otherwise.
    #[must_use]
    pub fn is_in_dropdown_area(&self, row: u16, col: u16) -> Option<usize> {
        self.command_dropdown_area.and_then(|area| {
            if row >= area.y
                && row < area.y + area.height
                && col >= area.x
                && col < area.x + area.width
            {
                let item_index = (row - area.y) as usize;
                if item_index < self.command_dropdown_item_count {
                    Some(item_index)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Check if a point is within any tool message area.
    /// Returns the message index if clicked on a tool message, None otherwise.
    #[must_use]
    pub fn is_tool_message_at(&self, row: u16, col: u16) -> Option<usize> {
        self.tool_message_areas.iter().find_map(|(area, index)| {
            if row >= area.y
                && row < area.y + area.height
                && col >= area.x
                && col < area.x + area.width
            {
                Some(*index)
            } else {
                None
            }
        })
    }

    /// Update terminal dimensions and recalculate max scroll.
    #[allow(clippy::cast_possible_truncation)]
    pub fn update_dimensions(&mut self, width: u16, height: u16, content_height: u16) {
        self.term_width = width;
        self.term_height = height;
        // Calculate visible message area height (subtract prompt area + gap)
        // Must match render_session layout: prompt_height = (input_lines + 5).clamp(6, 11)
        // Plus 1-line gap between messages and prompt
        let estimated_width = width.saturating_sub(3).max(1) as usize;
        let input_text = self.edit_buffer.text();
        let input_lines = if input_text.is_empty() {
            1
        } else {
            // Approximate wrapped line count (simple heuristic)
            let char_count = input_text.chars().count();
            let wrapped_lines = (char_count / estimated_width.max(1)) + 1;
            (input_text.lines().count().max(wrapped_lines)).min(6) as u16
        };
        let prompt_height = (input_lines + 5).clamp(6, 11);
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
                SessionMessage::User(user_msg) => {
                    // Collect text parts and file references into user message
                    let text: String = parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::Text(t) => Some(t.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    let files: Vec<_> = parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::Text(t) => Some(t.file_references.clone()),
                            _ => None,
                        })
                        .flatten()
                        .collect();

                    if !text.is_empty() {
                        let mode = if user_msg.agent == "plan" {
                            AgentMode::Plan
                        } else {
                            AgentMode::Build
                        };
                        display_messages.push(DisplayMessage::User {
                            text,
                            timestamp: None,
                            mode,
                            files,
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
                            Part::Reasoning(r) => {
                                if !assistant_text.is_empty() {
                                    display_messages.push(DisplayMessage::assistant(
                                        std::mem::take(&mut assistant_text),
                                    ));
                                }
                                display_messages.push(DisplayMessage::Reasoning {
                                    text: r.text.clone(),
                                });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tool_message_at_hit() {
        let mut app = App::new();
        app.tool_message_areas.push((Rect::new(0, 10, 80, 1), 5));
        assert_eq!(app.is_tool_message_at(10, 40), Some(5));
    }

    #[test]
    fn test_is_tool_message_at_hit_exact_bounds() {
        let mut app = App::new();
        app.tool_message_areas.push((Rect::new(10, 20, 50, 3), 7));
        // Top-left corner
        assert_eq!(app.is_tool_message_at(20, 10), Some(7));
        // Bottom-right corner (exclusive)
        assert_eq!(app.is_tool_message_at(22, 59), Some(7));
    }

    #[test]
    fn test_is_tool_message_at_miss_outside() {
        let mut app = App::new();
        app.tool_message_areas.push((Rect::new(0, 10, 80, 1), 5));
        // Above
        assert_eq!(app.is_tool_message_at(9, 40), None);
        // Below
        assert_eq!(app.is_tool_message_at(11, 40), None);
        // Left edge (x=0 is inside [0, 80))
        assert!(app.is_tool_message_at(10, 0).is_some());
        // Right
        assert_eq!(app.is_tool_message_at(10, 80), None);
    }

    #[test]
    fn test_is_tool_message_at_miss_empty() {
        let app = App::new();
        assert_eq!(app.is_tool_message_at(10, 10), None);
    }

    #[test]
    fn test_is_tool_message_at_multiple_areas() {
        let mut app = App::new();
        app.tool_message_areas.push((Rect::new(0, 10, 80, 1), 5));
        app.tool_message_areas.push((Rect::new(0, 20, 80, 1), 8));
        app.tool_message_areas.push((Rect::new(0, 30, 80, 1), 12));

        assert_eq!(app.is_tool_message_at(10, 40), Some(5));
        assert_eq!(app.is_tool_message_at(20, 40), Some(8));
        assert_eq!(app.is_tool_message_at(30, 40), Some(12));
        assert_eq!(app.is_tool_message_at(15, 40), None);
    }

    #[test]
    fn test_esc_and_backspace_flags_default_false() {
        let app = App::new();
        assert!(!app.esc_pressed_once);
        assert!(!app.backspace_on_empty_once);
    }

    #[test]
    fn test_finalize_streaming_cancelled() {
        let mut app = App::new();
        app.streaming_text = "partial response".to_string();
        app.finalize_streaming(true);
        assert_eq!(app.messages.len(), 1);
        if let DisplayMessage::Assistant { text } = &app.messages[0] {
            assert!(text.ends_with("--- Cancelled ---"));
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_tool_dialog_scroll_bounds_normal() {
        let dialog = ExpandedToolDialog {
            tool_name: "test".to_string(),
            invocation: "test".to_string(),
            output: "line\n".repeat(100),
            scroll_offset: 0,
            total_lines: 100,
            cached_lines: vec![],
            cached_width: 80,
            visible_height: 20,
        };

        #[allow(clippy::cast_possible_truncation)]
        let max_scroll = (dialog.total_lines as u16).saturating_sub(dialog.visible_height);
        assert_eq!(max_scroll, 80);
    }

    #[test]
    fn test_tool_dialog_scroll_bounds_content_smaller_than_visible() {
        let dialog = ExpandedToolDialog {
            tool_name: "test".to_string(),
            invocation: "test".to_string(),
            output: "line\n".repeat(10),
            scroll_offset: 0,
            total_lines: 10,
            cached_lines: vec![],
            cached_width: 80,
            visible_height: 20,
        };

        #[allow(clippy::cast_possible_truncation)]
        let max_scroll = (dialog.total_lines as u16).saturating_sub(dialog.visible_height);
        assert_eq!(max_scroll, 0);
    }

    #[test]
    fn test_tool_dialog_scroll_clamp_down() {
        let mut scroll_offset: u16 = 75;
        let total_lines: u16 = 100;
        let visible_height: u16 = 20;
        let max_scroll = total_lines.saturating_sub(visible_height);

        scroll_offset = scroll_offset.saturating_add(10).min(max_scroll);
        assert_eq!(scroll_offset, 80);

        scroll_offset = scroll_offset.saturating_add(10).min(max_scroll);
        assert_eq!(scroll_offset, 80);
    }

    #[test]
    fn test_tool_dialog_scroll_clamp_up() {
        let mut scroll_offset: u16 = 5;

        scroll_offset = scroll_offset.saturating_sub(10);
        assert_eq!(scroll_offset, 0);

        scroll_offset = scroll_offset.saturating_sub(1);
        assert_eq!(scroll_offset, 0);
    }
}
