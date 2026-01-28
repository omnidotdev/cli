//! Claude agent implementation.

mod conversation;
mod error;
pub mod permission;
mod plan;
mod provider;
pub mod providers;
mod tools;
mod types;

use std::path::PathBuf;

pub use conversation::Conversation;
pub use error::{AgentError, Result};
pub use permission::{
    AskUserResponse, InterfaceMessage, PermissionAction, PermissionActor, PermissionClient,
    PermissionContext, PermissionError, PermissionMessage, PermissionResponse,
};
pub use plan::PlanManager;
pub use provider::{CompletionEvent, CompletionRequest, CompletionStream, LlmProvider};
pub use providers::{AnthropicProvider, OpenAiProvider};
pub use tools::ToolRegistry;
pub use types::{
    ChatEvent, Content, ContentBlock, Message, MessagesRequest, Role, StopReason, StreamEvent, Tool,
};

use std::collections::HashMap;

use super::session::{
    AssistantMessage as SessionAssistantMessage, Message as SessionMessage, Part, SessionManager,
    TextPart, UserMessage as SessionUserMessage, extract_title, titling_prompt,
};

/// Agent operating mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentMode {
    /// Full tool access for implementation.
    #[default]
    Build,
    /// Read-only exploration for planning.
    Plan,
}

use futures::StreamExt;

/// Agent that orchestrates conversation with an LLM.
pub struct Agent {
    provider: Box<dyn LlmProvider>,
    conversation: Conversation,
    tools: ToolRegistry,
    model: String,
    max_tokens: u32,
    permission_client: Option<PermissionClient>,
    mode: AgentMode,
    plan_path: Option<PathBuf>,
    plan_manager: PlanManager,
    /// Session manager for persistence (optional)
    session_manager: Option<SessionManager>,
    /// Current session ID
    current_session_id: Option<String>,
}

impl Agent {
    /// Create a new agent with a provider.
    pub fn new(provider: Box<dyn LlmProvider>, model: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            provider,
            conversation: Conversation::new(),
            tools: ToolRegistry::new(),
            model: model.into(),
            max_tokens,
            permission_client: None,
            mode: AgentMode::default(),
            plan_path: None,
            plan_manager: PlanManager::new(),
            session_manager: None,
            current_session_id: None,
        }
    }

    /// Create an agent with a system prompt.
    pub fn with_system(
        provider: Box<dyn LlmProvider>,
        model: impl Into<String>,
        max_tokens: u32,
        system: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            conversation: Conversation::with_system(system),
            tools: ToolRegistry::new(),
            model: model.into(),
            max_tokens,
            permission_client: None,
            mode: AgentMode::default(),
            plan_path: None,
            plan_manager: PlanManager::new(),
            session_manager: None,
            current_session_id: None,
        }
    }

    /// Create an agent with automatic project context gathering.
    ///
    /// Gathers context from the current directory including:
    /// - Working directory and platform info
    /// - Git status (branch, modified files, recent commits)
    /// - Project type detection (Rust, Node, Python, Go)
    /// - Instruction files (CLAUDE.md from project and ~/.claude/)
    /// - Current model name
    pub fn with_context(
        provider: Box<dyn LlmProvider>,
        model: impl Into<String>,
        max_tokens: u32,
        persona_prompt: Option<&str>,
    ) -> Self {
        use crate::core::context::ProjectContext;

        let model_str: String = model.into();
        let context = ProjectContext::gather();
        let context_str = context.to_prompt_context();

        // Build system prompt with model identity at the very start
        let model_identity = format!("You are {model_str}, accessed through the Omni CLI.");

        let system_prompt = match persona_prompt {
            Some(persona) => format!("{model_identity}\n\n{persona}\n\n{context_str}"),
            None => format!("{model_identity}\n\n{context_str}"),
        };

        Self::with_system(provider, model_str, max_tokens, system_prompt)
    }

    /// Set the permission client for tool execution.
    pub fn set_permission_client(&mut self, client: PermissionClient) {
        self.permission_client = Some(client);
    }

    /// Enable session persistence for this agent
    ///
    /// Creates or loads a session for the current project
    pub fn enable_sessions(&mut self) -> Result<()> {
        let manager =
            SessionManager::for_current_project().map_err(|e| AgentError::Config(e.to_string()))?;

        let session = manager
            .get_or_create_current()
            .map_err(|e| AgentError::Config(e.to_string()))?;

        self.current_session_id = Some(session.id);
        self.session_manager = Some(manager);
        Ok(())
    }

    /// Switch to a different session by ID.
    ///
    /// Clears the current conversation and sets the new session as active.
    /// Does not load messages - the TUI handles display separately.
    ///
    /// # Errors
    ///
    /// Returns error if session not found or storage fails.
    pub fn switch_session(&mut self, session_id: &str) -> Result<()> {
        let Some(ref manager) = self.session_manager else {
            return Err(AgentError::Config("no session manager".to_string()));
        };

        // Verify session exists
        manager
            .get_session(session_id)
            .map_err(|e| AgentError::Config(e.to_string()))?;

        // Clear conversation and switch
        self.conversation.clear();
        self.current_session_id = Some(session_id.to_string());

        tracing::info!(session_id, "switched session");
        Ok(())
    }

    /// Create a new session and switch to it.
    ///
    /// # Errors
    ///
    /// Returns error if session creation fails.
    pub fn new_session(&mut self) -> Result<String> {
        let Some(ref manager) = self.session_manager else {
            return Err(AgentError::Config("no session manager".to_string()));
        };

        let session = manager
            .create_session()
            .map_err(|e| AgentError::Config(e.to_string()))?;

        self.conversation.clear();
        self.current_session_id = Some(session.id.clone());

        tracing::info!(session_id = %session.id, "created new session");
        Ok(session.id)
    }

    /// Get the session manager reference.
    #[must_use]
    pub const fn session_manager(&self) -> Option<&SessionManager> {
        self.session_manager.as_ref()
    }

    /// Get the current session ID if sessions are enabled
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.current_session_id.as_deref()
    }

    /// Check if title generation is needed and return the first user message if so
    ///
    /// Returns `Some(first_message)` if title generation should be triggered.
    #[must_use]
    pub fn needs_title_generation(&self) -> Option<String> {
        let Some(ref manager) = self.session_manager else {
            return None;
        };
        let Some(ref session_id) = self.current_session_id else {
            return None;
        };

        // Check if session has default title
        let session = manager.get_session(session_id).ok()?;
        if !session.has_default_title() {
            return None;
        }

        // Get first user message
        let messages = manager.list_messages(session_id).ok()?;
        for msg in messages {
            if let SessionMessage::User(_) = msg {
                // Get the text parts
                let parts = manager.list_parts(msg.id()).ok()?;
                for part in parts {
                    if let Part::Text(text_part) = part {
                        return Some(text_part.text);
                    }
                }
            }
        }

        None
    }

    /// Generate and set a title for the current session based on the first message
    ///
    /// # Errors
    ///
    /// Returns error if title generation or session update fails.
    pub async fn generate_title(&self, first_message: &str) -> Result<String> {
        let Some(ref manager) = self.session_manager else {
            return Err(AgentError::Config("no session manager".to_string()));
        };
        let Some(ref session_id) = self.current_session_id else {
            return Err(AgentError::Config("no session".to_string()));
        };

        // Generate title prompt
        let prompt = titling_prompt(first_message);

        // Make a simple completion request (no tools, no streaming needed)
        let request = CompletionRequest {
            model: "claude-3-5-haiku-latest".to_string(), // Use haiku for cheap/fast titling
            max_tokens: 50,
            messages: vec![Message {
                role: Role::User,
                content: Content::Text(prompt),
            }],
            system: Some("You are a helpful assistant that generates concise titles.".to_string()),
            tools: None,
        };

        let stream = self.provider.stream(request).await?;
        futures::pin_mut!(stream);

        let mut title_text = String::new();
        while let Some(event) = stream.next().await {
            if let Ok(CompletionEvent::TextDelta(text)) = event {
                title_text.push_str(&text);
            }
        }

        // Extract and clean the title
        let title = extract_title(&title_text);

        // Update session title
        manager
            .set_session_title(session_id, &title)
            .map_err(|e| AgentError::Config(e.to_string()))?;

        Ok(title)
    }

    /// Persist a user message to the current session
    fn persist_user_message(&self, text: &str) {
        let Some(ref manager) = self.session_manager else {
            return;
        };
        let Some(ref session_id) = self.current_session_id else {
            return;
        };

        let mode_str = match self.mode {
            AgentMode::Build => "build",
            AgentMode::Plan => "plan",
        };

        // Create session message
        let msg = SessionMessage::User(SessionUserMessage::new(
            session_id,
            mode_str,
            "anthropic", // TODO: get from provider
            &self.model,
        ));

        // Save message
        if let Err(e) = manager.save_message(session_id, &msg) {
            tracing::warn!("failed to persist user message: {e}");
            return;
        }

        // Create text part
        let part = Part::Text(TextPart::new(msg.id(), session_id, text));
        if let Err(e) = manager.save_part(msg.id(), &part) {
            tracing::warn!("failed to persist user message part: {e}");
        }

        // Touch session to update timestamp
        if let Err(e) = manager.touch_session(session_id) {
            tracing::warn!("failed to update session timestamp: {e}");
        }
    }

    /// Persist an assistant message to the current session
    fn persist_assistant_message(&self, text: &str) {
        let Some(ref manager) = self.session_manager else {
            return;
        };
        let Some(ref session_id) = self.current_session_id else {
            return;
        };

        let mode_str = match self.mode {
            AgentMode::Build => "build",
            AgentMode::Plan => "plan",
        };

        // Create session message (parent_id empty for now)
        let msg = SessionMessage::Assistant(SessionAssistantMessage::new(
            session_id,
            "", // parent_id - TODO: link to user message
            mode_str,
            "anthropic",
            &self.model,
        ));

        // Save message
        if let Err(e) = manager.save_message(session_id, &msg) {
            tracing::warn!("failed to persist assistant message: {e}");
            return;
        }

        // Create text part
        let part = Part::Text(TextPart::new(msg.id(), session_id, text));
        if let Err(e) = manager.save_part(msg.id(), &part) {
            tracing::warn!("failed to persist assistant message part: {e}");
        }

        // Touch session
        if let Err(e) = manager.touch_session(session_id) {
            tracing::warn!("failed to update session timestamp: {e}");
        }
    }

    /// Send a message and get a streaming response.
    ///
    /// This handles the full agent loop including tool execution.
    ///
    /// # Errors
    ///
    /// Returns error if API call or tool execution fails.
    pub async fn chat<F>(&mut self, message: &str, mut on_text: F) -> Result<String>
    where
        F: FnMut(&str),
    {
        // Wrap the text callback to convert to events
        self.chat_with_events(message, |event| {
            if let ChatEvent::Text(text) = event {
                on_text(&text);
            }
        })
        .await
    }

    /// Send a message and get a streaming response with structured events
    ///
    /// This handles the full agent loop including tool execution and emits
    /// events for both text and tool calls, useful for rich UI rendering.
    ///
    /// # Errors
    ///
    /// Returns error if API call or tool execution fails.
    pub async fn chat_with_events<F>(&mut self, message: &str, mut on_event: F) -> Result<String>
    where
        F: FnMut(ChatEvent),
    {
        self.conversation.add_user_message(message);
        self.persist_user_message(message);

        loop {
            let (content_blocks, stop_reason) = self.stream_response_events(&mut on_event).await?;

            if !content_blocks.is_empty() {
                self.conversation
                    .add_assistant_blocks(content_blocks.clone());
            }

            if stop_reason == Some(StopReason::ToolUse) {
                self.handle_tool_use_events(&content_blocks, &mut on_event)
                    .await?;
            } else {
                let text = content_blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                // Persist final assistant response
                if !text.is_empty() {
                    self.persist_assistant_message(&text);
                }

                return Ok(text);
            }
        }
    }

    #[allow(dead_code)]
    async fn stream_response<F>(
        &self,
        on_text: &mut F,
    ) -> Result<(Vec<ContentBlock>, Option<StopReason>)>
    where
        F: FnMut(&str),
    {
        let request = CompletionRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: self.conversation.messages().to_vec(),
            system: self.conversation.system().map(String::from),
            tools: Some(self.tools.definitions(self.mode)),
        };

        let stream = self.provider.stream(request).await?;
        futures::pin_mut!(stream);

        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_tool_inputs: HashMap<usize, String> = HashMap::new();
        let mut stop_reason = None;

        while let Some(event) = stream.next().await {
            let event = event?;

            match event {
                CompletionEvent::TextDelta(text) => {
                    on_text(&text);
                    // Append to last text block or create one
                    if let Some(ContentBlock::Text { text: t }) = content_blocks.last_mut() {
                        t.push_str(&text);
                    } else {
                        content_blocks.push(ContentBlock::Text { text });
                    }
                }
                CompletionEvent::ToolUseStart { index, id, name } => {
                    while content_blocks.len() <= index {
                        content_blocks.push(ContentBlock::Text {
                            text: String::new(),
                        });
                    }
                    content_blocks[index] = ContentBlock::ToolUse {
                        id,
                        name,
                        input: serde_json::Value::Null,
                    };
                }
                CompletionEvent::ToolInputDelta {
                    index,
                    partial_json,
                } => {
                    current_tool_inputs
                        .entry(index)
                        .or_default()
                        .push_str(&partial_json);
                }
                CompletionEvent::ContentBlockDone { index, block } => {
                    // Finalize tool input if present
                    if let Some(ContentBlock::ToolUse { input, .. }) = content_blocks.get_mut(index)
                    {
                        if let Some(json_str) = current_tool_inputs.remove(&index) {
                            *input =
                                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
                        }
                    }
                    // Use the finalized block from the event if it's a tool use
                    if let ContentBlock::ToolUse {
                        input: event_input, ..
                    } = &block
                    {
                        if let Some(ContentBlock::ToolUse { id, name, input }) =
                            content_blocks.get_mut(index)
                        {
                            // Prefer the event's input if our input is still null
                            if input.is_null() {
                                *input = event_input.clone();
                            }
                            let _ = (id, name); // Suppress unused warning.
                        }
                    }
                }
                CompletionEvent::Done {
                    stop_reason: sr, ..
                } => {
                    stop_reason = sr;
                }
                CompletionEvent::Error(msg) => {
                    return Err(AgentError::Api {
                        status: 0,
                        message: msg,
                    });
                }
            }
        }

        Ok((content_blocks, stop_reason))
    }

    #[allow(dead_code)]
    async fn handle_tool_use<F>(
        &mut self,
        content_blocks: &[ContentBlock],
        on_text: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&str),
    {
        for block in content_blocks {
            if let ContentBlock::ToolUse { id, name, input } = block {
                on_text(&format!("\n[Calling tool: {name}]\n"));

                let result = self
                    .tools
                    .execute(
                        name,
                        input.clone(),
                        self.permission_client.as_ref(),
                        self.mode,
                        &self.plan_manager,
                    )
                    .await;

                let (content, is_error) = match result {
                    Ok(output) => {
                        // Check for mode switch markers
                        if output == "[MODE_SWITCH:PLAN]" {
                            self.switch_mode(AgentMode::Plan, None);
                            on_text("[Switched to plan mode]\n");
                            (
                                "Switched to plan mode. You can now explore and plan.".to_string(),
                                false,
                            )
                        } else if output == "[MODE_SWITCH:BUILD]" {
                            self.switch_mode(AgentMode::Build, None);
                            on_text("[Switched to build mode]\n");
                            let msg = if let Some(path) = &self.plan_path {
                                format!(
                                    "Switched to build mode. Plan available at: {}",
                                    path.display()
                                )
                            } else {
                                "Switched to build mode.".to_string()
                            };
                            (msg, false)
                        } else {
                            on_text(&format!("[Tool result: {} chars]\n", output.len()));
                            (output, false)
                        }
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        on_text(&format!("[Tool error: {error_msg}]\n"));
                        (error_msg, true)
                    }
                };

                self.conversation
                    .add_tool_result(id.clone(), content, is_error);
            }
        }

        Ok(())
    }

    async fn stream_response_events<F>(
        &self,
        on_event: &mut F,
    ) -> Result<(Vec<ContentBlock>, Option<StopReason>)>
    where
        F: FnMut(ChatEvent),
    {
        let request = CompletionRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: self.conversation.messages().to_vec(),
            system: self.conversation.system().map(String::from),
            tools: Some(self.tools.definitions(self.mode)),
        };

        let stream = self.provider.stream(request).await?;
        futures::pin_mut!(stream);

        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_tool_inputs: HashMap<usize, String> = HashMap::new();
        let mut stop_reason = None;

        while let Some(event) = stream.next().await {
            let event = event?;

            match event {
                CompletionEvent::TextDelta(text) => {
                    on_event(ChatEvent::Text(text.clone()));
                    if let Some(ContentBlock::Text { text: t }) = content_blocks.last_mut() {
                        t.push_str(&text);
                    } else {
                        content_blocks.push(ContentBlock::Text { text });
                    }
                }
                CompletionEvent::ToolUseStart { index, id, name } => {
                    while content_blocks.len() <= index {
                        content_blocks.push(ContentBlock::Text {
                            text: String::new(),
                        });
                    }
                    content_blocks[index] = ContentBlock::ToolUse {
                        id,
                        name,
                        input: serde_json::Value::Null,
                    };
                }
                CompletionEvent::ToolInputDelta {
                    index,
                    partial_json,
                } => {
                    current_tool_inputs
                        .entry(index)
                        .or_default()
                        .push_str(&partial_json);
                }
                CompletionEvent::ContentBlockDone { index, block } => {
                    if let Some(ContentBlock::ToolUse { input, .. }) = content_blocks.get_mut(index)
                    {
                        if let Some(json_str) = current_tool_inputs.remove(&index) {
                            *input =
                                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
                        }
                    }
                    if let ContentBlock::ToolUse {
                        input: event_input, ..
                    } = &block
                    {
                        if let Some(ContentBlock::ToolUse { id, name, input }) =
                            content_blocks.get_mut(index)
                        {
                            if input.is_null() {
                                *input = event_input.clone();
                            }
                            let _ = (id, name);
                        }
                    }
                }
                CompletionEvent::Done {
                    stop_reason: sr,
                    usage,
                } => {
                    stop_reason = sr;
                    // Emit usage event if we have usage data
                    if let Some(u) = usage {
                        // Cost calculation (Anthropic Claude Sonnet pricing per million tokens)
                        // Input: $3/M, Output: $15/M (approximate)
                        let cost = f64::from(u.input_tokens)
                            .mul_add(3.0, f64::from(u.output_tokens) * 15.0)
                            / 1_000_000.0;
                        on_event(ChatEvent::Usage {
                            input_tokens: u.input_tokens,
                            output_tokens: u.output_tokens,
                            cost_usd: cost,
                        });
                    }
                }
                CompletionEvent::Error(msg) => {
                    return Err(AgentError::Api {
                        status: 0,
                        message: msg,
                    });
                }
            }
        }

        Ok((content_blocks, stop_reason))
    }

    async fn handle_tool_use_events<F>(
        &mut self,
        content_blocks: &[ContentBlock],
        on_event: &mut F,
    ) -> Result<()>
    where
        F: FnMut(ChatEvent),
    {
        for block in content_blocks {
            if let ContentBlock::ToolUse { id, name, input } = block {
                // Format invocation for display
                let invocation = format_tool_invocation(name, input);

                let result = self
                    .tools
                    .execute(
                        name,
                        input.clone(),
                        self.permission_client.as_ref(),
                        self.mode,
                        &self.plan_manager,
                    )
                    .await;

                let (content, is_error) = match result {
                    Ok(output) => {
                        if output == "[MODE_SWITCH:PLAN]" {
                            self.switch_mode(AgentMode::Plan, None);
                            ("Switched to plan mode".to_string(), false)
                        } else if output == "[MODE_SWITCH:BUILD]" {
                            self.switch_mode(AgentMode::Build, None);
                            ("Switched to build mode".to_string(), false)
                        } else {
                            (output, false)
                        }
                    }
                    Err(e) => (e.to_string(), true),
                };

                // Emit tool event
                on_event(ChatEvent::ToolCall {
                    name: name.clone(),
                    invocation,
                    output: content.clone(),
                    is_error,
                });

                self.conversation
                    .add_tool_result(id.clone(), content, is_error);
            }
        }

        Ok(())
    }

    /// Clear conversation history.
    pub fn clear(&mut self) {
        self.conversation.clear();
    }

    /// Save conversation history to the default path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save_history(&self) -> Result<()> {
        let path =
            crate::config::Config::history_path().map_err(|e| AgentError::Config(e.to_string()))?;
        self.conversation
            .save(&path)
            .map_err(|e| AgentError::Config(e.to_string()))?;
        Ok(())
    }

    /// Load conversation history from the default path.
    ///
    /// Preserves the current system prompt while loading message history.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read.
    pub fn load_history(&mut self) -> Result<()> {
        let path =
            crate::config::Config::history_path().map_err(|e| AgentError::Config(e.to_string()))?;

        // Preserve current system prompt (fresh context)
        let current_system = self.conversation.system().map(String::from);

        self.conversation =
            Conversation::load(&path).map_err(|e| AgentError::Config(e.to_string()))?;

        // Restore fresh system prompt
        if let Some(system) = current_system {
            self.conversation.set_system(system);
        }

        Ok(())
    }

    /// Check if conversation has any history.
    #[must_use]
    pub fn has_history(&self) -> bool {
        !self.conversation.is_empty()
    }

    /// Get the current agent mode.
    #[must_use]
    pub const fn mode(&self) -> AgentMode {
        self.mode
    }

    /// Get the current model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Set the model name.
    ///
    /// Also updates the system prompt and adds a context message to help
    /// the new model understand it has taken over the conversation.
    pub fn set_model(&mut self, model: impl Into<String>) {
        let old_model = std::mem::replace(&mut self.model, model.into());
        self.update_model_in_system_prompt();

        // Add context message if there's conversation history and model changed
        if old_model != self.model && !self.conversation.messages().is_empty() {
            self.conversation.add_user_message(format!(
                "[Model switched from {} to {}]",
                old_model, self.model
            ));
        }

        tracing::info!(model = %self.model, "switched model");
    }

    /// Update the model info in the system prompt.
    fn update_model_in_system_prompt(&mut self) {
        let model_identity = format!("You are {}, accessed through the Omni CLI.", self.model);

        if let Some(existing) = self.conversation.system() {
            // Replace existing model identity at start of prompt
            if existing.starts_with("You are ") {
                // Find end of first line
                if let Some(end) = existing.find('\n') {
                    let mut updated = existing.to_string();
                    updated.replace_range(..end, &model_identity);
                    self.conversation.set_system(updated);
                    return;
                }
            }

            // No existing model identity - prepend it
            self.conversation
                .set_system(format!("{model_identity}\n\n{existing}"));
        }
    }

    /// Set the LLM provider.
    pub fn set_provider(&mut self, provider: Box<dyn LlmProvider>) {
        tracing::info!(provider = %provider.name(), "switched provider");
        self.provider = provider;
    }

    /// Get the current provider name.
    #[must_use]
    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    /// Get the current plan file path, if any.
    #[must_use]
    pub const fn plan_path(&self) -> Option<&PathBuf> {
        self.plan_path.as_ref()
    }

    /// Switch agent mode.
    ///
    /// When switching to Plan mode, generates a new plan file path and injects plan instructions.
    /// When switching to Build mode, injects plan context for implementation.
    pub fn switch_mode(&mut self, mode: AgentMode, slug: Option<&str>) {
        if mode == self.mode {
            return;
        }

        match mode {
            AgentMode::Plan => {
                let slug = slug.unwrap_or("plan");
                self.plan_path = Some(self.plan_manager.new_plan_path(slug));
                self.inject_plan_mode_context();
            }
            AgentMode::Build => {
                self.inject_build_mode_context();
            }
        }

        self.mode = mode;
    }

    /// Inject plan mode instructions into the system prompt.
    fn inject_plan_mode_context(&mut self) {
        let plan_path = self
            .plan_path
            .as_ref()
            .map_or_else(|| "plan.md".to_string(), |p| p.display().to_string());

        let plan_context = format!(
            r"
## Plan Mode

You are in PLAN MODE. Your role is to explore, analyze, and design - not implement.

Constraints:
- You can read files and run read-only shell commands
- You can NOT write files except to your plan file
- You can NOT run write-affecting commands (git commit, rm, mkdir, etc.)

Workflow:
1. Understand the request thoroughly
2. Explore relevant code with read_file and shell commands
3. Ask clarifying questions via ask_user
4. Write your plan to the plan file
5. Call plan_exit when ready to implement

Plan file location: {plan_path}
"
        );

        // Append to existing system prompt
        if let Some(existing) = self.conversation.system() {
            self.conversation
                .set_system(format!("{existing}\n{plan_context}"));
        } else {
            self.conversation.set_system(plan_context);
        }
    }

    /// Inject build mode context with plan reference.
    fn inject_build_mode_context(&mut self) {
        if let Some(plan_path) = &self.plan_path {
            let build_context = format!(
                r"
## Active Plan

You have an approved implementation plan at: {}
Follow this plan. Refer back to it as you work.
",
                plan_path.display()
            );

            // Append to existing system prompt
            if let Some(existing) = self.conversation.system() {
                self.conversation
                    .set_system(format!("{existing}\n{build_context}"));
            } else {
                self.conversation.set_system(build_context);
            }
        }
    }

    /// Get a reference to the plan manager.
    #[must_use]
    pub const fn plan_manager(&self) -> &PlanManager {
        &self.plan_manager
    }
}

/// Format tool input for display in the UI
fn format_tool_invocation(name: &str, input: &serde_json::Value) -> String {
    const MAX_LEN: usize = 60;

    let raw = match name {
        "shell" => input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "read_file" => input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "write_file" | "edit_file" => input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Glob" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Grep" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => input
            .as_object()
            .and_then(|obj| obj.values().find_map(|v| v.as_str()))
            .unwrap_or("")
            .to_string(),
    };

    if raw.len() > MAX_LEN {
        format!("{}...", &raw[..MAX_LEN - 3])
    } else {
        raw
    }
}
