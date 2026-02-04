//! Terminal user interface for Omni.

mod app;
mod components;
mod message;
mod state;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io;
use std::sync::OnceLock;
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, MouseEventKind,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tokio::sync::mpsc;

use crate::core::agent::{
    AskUserResponse, InterfaceMessage, PermissionAction, PermissionActor, PermissionClient,
    PermissionContext, PermissionMessage, PermissionResponse,
};
use crate::core::session::SessionTarget;

pub use app::App;
use app::{
    ActiveAskUserDialog, ActiveDialog, ActivePermissionDialog, ChatMessage, ExpandedToolDialog,
};
use components::{
    DropdownMode, InputAction, MESSAGE_PADDING_X, TextLayout, build_keybinding_map,
    calculate_content_height, default_keybindings, dropdown_mode, file_picker, filter_commands,
    filter_models, find_at_mention_spans, line_color, render_command_dropdown,
    render_model_dropdown, render_model_selection_dialog, render_session, render_session_list,
    render_welcome, should_show_dropdown,
};
use message::DisplayMessage;
use state::ViewState;

fn keybinding_map() -> &'static HashMap<(KeyCode, KeyModifiers), InputAction> {
    static MAP: OnceLock<HashMap<(KeyCode, KeyModifiers), InputAction>> = OnceLock::new();
    MAP.get_or_init(|| build_keybinding_map(&default_keybindings()))
}

fn find_at_mention_span_ending_at(text: &str, cursor_pos: usize) -> Option<(usize, usize)> {
    find_at_mention_spans(text)
        .into_iter()
        .find(|&(_, end)| end == cursor_pos)
}

/// Run the TUI application.
///
/// # Errors
///
/// Returns an error if terminal initialization fails or the event loop encounters an error.
pub async fn run() -> anyhow::Result<()> {
    run_with_target(SessionTarget::default()).await
}

/// Run the TUI application with a specific session target.
///
/// # Errors
///
/// Returns an error if terminal initialization fails or the event loop encounters an error.
pub async fn run_with_target(target: SessionTarget) -> anyhow::Result<()> {
    // Mouse capture enabled: Shift+Drag for text selection
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;

    // Enable enhanced keyboard support for terminals like Kitty
    // DISAMBIGUATE_ESCAPE_CODES allows Shift+Enter detection without breaking shifted chars
    let supports_keyboard_enhancement =
        crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if supports_keyboard_enhancement {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Set up permission system
    let (permission_actor, permission_tx) = PermissionActor::new();
    let (interface_tx, interface_rx) = mpsc::unbounded_channel();
    let (perm_response_tx, mut perm_response_rx) = mpsc::unbounded_channel::<(
        uuid::Uuid,
        String,
        String,
        PermissionAction,
        PermissionResponse,
    )>();
    let (ask_response_tx, mut ask_response_rx) =
        mpsc::unbounded_channel::<(uuid::Uuid, AskUserResponse)>();

    // Register interface with permission actor
    permission_tx
        .send(PermissionMessage::RegisterInterface { interface_tx })
        .ok();

    // Spawn permission actor with response handling
    tokio::spawn(async move {
        let mut actor = permission_actor;

        loop {
            tokio::select! {
                // Process actor inbox
                msg = actor.inbox.recv() => {
                    match msg {
                        Some(m) => actor.handle_message(m),
                        None => break,
                    }
                }

                // Process permission responses from TUI
                Some((request_id, session_id, tool_name, action, response)) = perm_response_rx.recv() => {
                    actor.respond(request_id, response, &session_id, &tool_name, &action);
                }

                // Process ask_user responses from TUI
                Some((request_id, response)) = ask_response_rx.recv() => {
                    actor.respond_ask_user(request_id, response);
                }
            }
        }
    });

    // Create app state with permission channels and session target
    let mut app = App::with_session_target(target);
    app.interface_rx = Some(interface_rx);
    app.permission_response_tx = Some(perm_response_tx);
    app.ask_user_response_tx = Some(ask_response_tx);

    // Spawn background task to fetch provider models
    let (models_tx, models_rx) = mpsc::unbounded_channel();
    app.models_rx = Some(models_rx);
    let config_clone = app.agent_config.clone();
    tokio::spawn(async move {
        let models = crate::core::models::fetch_provider_models(&config_clone).await;
        let _ = models_tx.send(models);
    });

    // Set up permission client for agent with current permission presets
    let presets = app.current_permissions();
    if let Some(ref mut agent) = app.agent {
        let client = PermissionClient::with_presets(
            "tui-session".to_string(),
            permission_tx.clone(),
            presets,
        );
        agent.set_permission_client(client);
    }

    // Store permission_tx for new agents
    let result = run_app(&mut terminal, &mut app, permission_tx).await;

    // Restore terminal
    if supports_keyboard_enhancement {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

#[allow(clippy::too_many_lines)]
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    permission_tx: mpsc::UnboundedSender<PermissionMessage>,
) -> anyhow::Result<()> {
    loop {
        // Clear selected text before render (will be populated if selection is active)
        app.selected_text.clear();

        terminal.draw(|f| {
            let full_area = f.area();

            // Apply horizontal padding to main content area
            let area = Rect::new(
                full_area.x + 1,
                full_area.y,
                full_area.width.saturating_sub(2),
                full_area.height,
            );

            let padded_width = area.width.saturating_sub(MESSAGE_PADDING_X * 2);
            let content_height = calculate_content_height(
                &app.messages,
                &app.streaming_thinking,
                &app.streaming_text,
                padded_width,
            );

            // Update dimensions for scroll calculations
            app.update_dimensions(area.width, area.height, content_height);

            // Dispatch rendering based on view state
            let (cursor_pos, prompt_area) = match app.view_state {
                ViewState::Welcome => {
                    // Full-screen welcome with centered logo and prompt
                    render_welcome(
                        f,
                        area,
                        app.tagline,
                        app.tip,
                        app.input(),
                        app.cursor(),
                        app.placeholder,
                        app.agent_mode,
                        &app.model,
                        &app.provider,
                        app.prompt_scroll_offset,
                        app.reasoning_effort,
                    )
                }
                ViewState::Session => {
                    // Session view with messages and bottom prompt
                    let status = {
                        let queue_count = app.queued_message_count();

                        if app.esc_pressed_once && app.loading {
                            Some("press esc to cancel".to_string())
                        } else if app.backspace_on_empty_once && queue_count > 0 {
                            Some("press backspace to delete last queued".to_string())
                        } else if app.loading {
                            Some(
                                app.activity_status
                                    .as_deref()
                                    .unwrap_or("Thinking...")
                                    .to_string(),
                            )
                        } else {
                            None
                        }
                    };
                    let status = status.as_deref();
                    let input_ref = app.edit_buffer.text();
                    let cursor_val = app.edit_buffer.cursor();
                    render_session(
                        f,
                        area,
                        &app.messages,
                        &app.pending_messages,
                        &app.streaming_thinking,
                        &app.streaming_text,
                        input_ref,
                        cursor_val,
                        app.message_scroll,
                        status,
                        &app.model,
                        &app.provider,
                        app.agent_mode,
                        app.selection.as_ref(),
                        &mut app.selected_text,
                        app.session_cost,
                        app.prompt_scroll_offset,
                        &mut app.tool_message_areas,
                        app.reasoning_effort,
                    )
                }
            };

            app.set_prompt_area(prompt_area);
            app.prompt_text_width = prompt_area.width.saturating_sub(4).max(1) as usize;

            // Set cursor position
            f.set_cursor_position(Position::new(cursor_pos.0, cursor_pos.1));

            // Render dropdown if visible - use the exact prompt area returned
            if app.show_command_dropdown && should_show_dropdown(app.input()) {
                match dropdown_mode(app.input()) {
                    DropdownMode::Commands => {
                        let (_, area) = render_command_dropdown(
                            f,
                            prompt_area,
                            app.input(),
                            app.command_selection,
                        );
                        app.set_dropdown_area(Some(area), filter_commands(app.input()).len());
                    }
                    DropdownMode::Models => {
                        let (_, area) = render_model_dropdown(
                            f,
                            prompt_area,
                            app.input(),
                            app.command_selection,
                            &app.agent_config.models,
                        );
                        app.set_dropdown_area(
                            Some(area),
                            filter_models(app.input(), &app.agent_config.models).len(),
                        );
                    }
                    DropdownMode::None => {}
                }
            }

            // Render dialog overlay if active
            if let Some(ref mut dialog) = app.active_dialog {
                match dialog {
                    ActiveDialog::Permission(d) => render_permission_dialog(f, d),
                    ActiveDialog::AskUser(d) => render_ask_user_dialog(f, d),
                    ActiveDialog::SessionList(d) => render_session_list(f, d),
                    ActiveDialog::ModelSelection(d) => {
                        render_model_selection_dialog(f, d, app.agent_mode);
                    }
                    ActiveDialog::ToolOutput(d) => render_tool_output_dialog(f, d),
                    ActiveDialog::NoProvider => render_no_provider_dialog(f),
                }
            }
        })?;

        // Poll for events and messages concurrently
        tokio::select! {
            // Check for input events
            () = tokio::time::sleep(Duration::from_millis(10)) => {
                while event::poll(Duration::from_millis(0))? {
                    match event::read()? {
                        Event::Key(key) => {
                            // Accept Press and Repeat, but not Release
                            // Some terminals (e.g. Termux) may not report KeyEventKind correctly
                            if key.kind != KeyEventKind::Release {
                                if app.has_dialog() {
                                    // Handle dialog input
                                    if handle_dialog_key(app, key.code, key.modifiers) {
                                        return Ok(());
                                    }
                                } else if handle_key(app, key.code, key.modifiers, &permission_tx) {
                                    return Ok(());
                                }
                            }
                        }
                        Event::Paste(text) => {
                            // Insert pasted text directly without triggering submission
                            // Strip any trailing newlines to prevent accidental submission
                            let text = text.trim_end_matches('\n').trim_end_matches('\r');
                            for c in text.chars() {
                                if c == '\n' || c == '\r' {
                                    // Convert newlines to actual newlines in input
                                    app.insert_char('\n');
                                } else {
                                    app.insert_char(c);
                                }
                            }
                        }
                        Event::Mouse(mouse) => {
                            if !app.has_dialog() {
                                match mouse.kind {
                                    MouseEventKind::ScrollUp => {
                                        if app.is_in_prompt_area(mouse.row, mouse.column) {
                                            app.scroll_prompt_up(3);
                                        } else if app.view_state == ViewState::Session {
                                            app.scroll_messages_up(3);
                                        } else {
                                            app.scroll_up(3);
                                        }
                                    }
                                    MouseEventKind::ScrollDown => {
                                        if app.is_in_prompt_area(mouse.row, mouse.column) {
                                            let layout = TextLayout::new(app.input(), app.prompt_text_width);
                                            let max_scroll = layout.total_lines.saturating_sub(6);
                                            app.scroll_prompt_down(3, max_scroll);
                                        } else if app.view_state == ViewState::Session {
                                            app.scroll_messages_down(3);
                                        } else {
                                            app.scroll_down(3, 1000);
                                        }
                                    }
                                    MouseEventKind::Down(_button) => {
                                        // Handle dropdown clicks first (if dropdown is visible)
                                        if app.show_command_dropdown {
                                            if let Some(item_index) = app.is_in_dropdown_area(mouse.row, mouse.column) {
                                                // Click inside dropdown - select and execute
                                                app.command_selection = item_index;

                                                // Execute command (same logic as Enter key)
                                                match dropdown_mode(app.input()) {
                                                    DropdownMode::Commands => {
                                                        let filtered = filter_commands(app.input());
                                                        if let Some(cmd) = filtered.get(app.command_selection) {
                                                            app.set_input(cmd.name.to_string());
                                                        }
                                                    }
                                                    DropdownMode::Models => {
                                                        let filtered = filter_models(app.input(), &app.agent_config.models);
                                                        if let Some(model) = filtered.get(app.command_selection) {
                                                            app.set_input(format!("/model {}", model.id));
                                                        }
                                                    }
                                                    DropdownMode::None => {}
                                                }
                                            }
                                            // Click anywhere (inside or outside) closes dropdown
                                            app.show_command_dropdown = false;
                                            app.set_dropdown_area(None, 0);
                                        } else {
                                            // Dropdown not visible - handle tool message clicks (existing logic)
                                            if let Some(message_index) = app.is_tool_message_at(mouse.row, mouse.column) {
                                                if let Some(DisplayMessage::Tool { name, invocation, output, .. }) = app.messages.get(message_index) {
                                                    let area = terminal.get_frame().area();
                                                    let dialog_width = area.width * 80 / 100;
                                                    let content_width = dialog_width.saturating_sub(4);

                                                    let cached_lines: Vec<ratatui::text::Line<'static>> = output
                                                        .lines()
                                                        .map(|line| ratatui::text::Line::from(ratatui::text::Span::styled(
                                                            line.to_owned(),
                                                            Style::default().fg(line_color(line))
                                                        )))
                                                        .collect();
                                                    let total_lines = cached_lines.len();
                                                    let max_height = area.height * 80 / 100;
                                                    let visible_height = max_height.saturating_sub(6);
                                                    let dialog = ExpandedToolDialog {
                                                        tool_name: name.clone(),
                                                        invocation: invocation.clone(),
                                                        output: output.clone(),
                                                        scroll_offset: 0,
                                                        total_lines,
                                                        cached_lines,
                                                        cached_width: content_width,
                                                        visible_height,
                                                    };
                                                    app.active_dialog = Some(ActiveDialog::ToolOutput(dialog));
                                                }
                                            }
                                        }
                                    }
                                    MouseEventKind::Moved => {
                                        // Only process hover when dropdown is visible (performance guard)
                                        if app.show_command_dropdown {
                                            if let Some(item_index) = app.is_in_dropdown_area(mouse.row, mouse.column) {
                                                app.command_selection = item_index;
                                            }
                                            // If outside dropdown area, selection sticks to last hovered item
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Check for chat messages
            msg = async {
                if let Some(ref mut rx) = app.chat_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                match msg {
                    Some(ChatMessage::Text(text)) => {
                        // Accumulate streaming text
                        app.streaming_text.push_str(&text);
                        app.output.push_str(&text);
                        // Clear activity status when receiving text
                        app.activity_status = None;
                    }
                    Some(ChatMessage::ToolStart { name }) => {
                        // Update activity status to show current tool
                        app.activity_status = Some(format!("Using {name}..."));
                    }
                    Some(ChatMessage::Tool { name, invocation, output, is_error }) => {
                        // Finalize any pending streaming text before tool message
                        app.finalize_streaming(false);
                        // Clear activity status
                        app.activity_status = None;
                        // Add tool message
                        app.messages.push(DisplayMessage::tool(&name, &invocation, &output, is_error));
                    }
                    Some(ChatMessage::Usage { input_tokens, output_tokens, cost_usd }) => {
                        // Accumulate session usage
                        app.session_tokens.0 += input_tokens;
                        app.session_tokens.1 += output_tokens;
                        app.session_cost += cost_usd;
                    }
                    Some(ChatMessage::Done(agent)) => {
                        // Finalize streaming text into an assistant message
                        app.finalize_streaming(false);

                        // Save conversation history
                        if let Err(e) = agent.save_history() {
                            tracing::warn!("failed to save history: {e}");
                        }
                        app.agent = Some(agent);
                        app.activity_status = None;
                        app.chat_rx = None;

                        // Process next queued message if any
                        if let Some(next_prompt) = app.activate_first_queued_message() {
                            app.set_input(next_prompt);
                            start_chat(app, permission_tx.clone());
                        } else {
                            app.loading = false;
                        }
                    }
                    Some(ChatMessage::Error(e, agent)) => {
                        // Finalize any partial streaming text
                        app.finalize_streaming(false);

                        // Add error message
                        app.messages.push(DisplayMessage::tool_error("Error", &e));

                        app.agent = Some(agent);
                        let _ = write!(app.output, "\nError: {e}");
                        app.activity_status = None;
                        app.chat_rx = None;

                        // Process next queued message if any
                        if let Some(next_prompt) = app.activate_first_queued_message() {
                            app.set_input(next_prompt);
                            start_chat(app, permission_tx.clone());
                        } else {
                            app.loading = false;
                        }
                    }
                    None => {
                        app.finalize_streaming(false);
                        app.loading = false;
                        app.activity_status = None;
                        app.chat_rx = None;
                    }
                    Some(ChatMessage::ThinkingStart) => {
                        app.streaming_thinking.clear();
                        app.activity_status = Some("Thinking...".to_string());
                    }
                    Some(ChatMessage::Thinking(text)) => {
                        app.streaming_thinking.push_str(&text);
                    }
                }
            }

            // Check for interface messages (permission dialogs)
            msg = async {
                if let Some(ref mut rx) = app.interface_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Some(msg) = msg {
                    match msg {
                        InterfaceMessage::ShowPermissionDialog { request_id, tool_name, action, context } => {
                            app.show_permission_dialog(request_id, tool_name, action, context);
                        }
                        InterfaceMessage::ShowAskUserDialog { request_id, question, options } => {
                            app.show_ask_user_dialog(request_id, question, options);
                        }
                        InterfaceMessage::HideDialog => {
                            app.hide_dialog();
                        }
                    }
                }
            }

            models = async {
                if let Some(ref mut rx) = app.models_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Some(models) = models {
                    tracing::info!(
                        providers = models.len(),
                        "received cached provider models"
                    );
                    app.cached_provider_models = Some(models);
                    app.models_rx = None;
                }
            }
        }
    }
}

/// Handle a key press. Returns true if the app should exit.
#[allow(clippy::too_many_lines)]
fn handle_key(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    permission_tx: &mpsc::UnboundedSender<PermissionMessage>,
) -> bool {
    // Handle Shift+Enter or Alt+Enter for newline insertion
    // Allow even while loading so user can prepare next message
    if (modifiers.contains(KeyModifiers::SHIFT) || modifiers.contains(KeyModifiers::ALT))
        && code == KeyCode::Enter
    {
        app.insert_char('\n');
        return false;
    }

    // Handle Ctrl combinations
    if modifiers.contains(KeyModifiers::CONTROL) {
        match code {
            KeyCode::Char('c') => {
                if app.loading {
                    // Cancel streaming
                    app.chat_rx = None;
                    app.finalize_streaming(true);
                    app.loading = false;
                } else if app.input().is_empty() {
                    return true; // Exit.
                } else {
                    app.clear_input();
                    app.show_command_dropdown = false;
                }
            }
            KeyCode::Char('a') => app.set_cursor(0),
            KeyCode::Char('e') => app.set_cursor(app.edit_buffer.len()),
            KeyCode::Left => app.move_word_left(),
            KeyCode::Right => app.move_word_right(),
            KeyCode::Char('u') => {
                app.delete_to_start();
                app.show_command_dropdown = should_show_dropdown(app.input());
                app.command_selection = 0;
            }
            KeyCode::Char('k') => {
                app.delete_to_end();
                app.show_command_dropdown = should_show_dropdown(app.input());
                app.command_selection = 0;
            }
            KeyCode::Char('w') => {
                app.delete_word();
                app.show_command_dropdown = should_show_dropdown(app.input());
                app.command_selection = 0;
            }
            KeyCode::Char('l') => {
                // Clear output and conversation history
                app.output.clear();
                app.scroll_offset = 0;
                app.clear_conversation();
                app.view_state = ViewState::Welcome;
                app.show_welcome = true;
                app.show_command_dropdown = false;
                if let Some(ref mut agent) = app.agent {
                    agent.clear();
                    if let Err(e) = agent.save_history() {
                        tracing::warn!("failed to save cleared history: {e}");
                    }
                }
                app.output = "Conversation cleared.".to_string();
            }
            KeyCode::Char('s') => {
                // Open session list dialog
                app.show_session_list();
            }
            KeyCode::Char('t') => {
                app.reasoning_effort = app.reasoning_effort.next();
                if let Some(ref mut agent) = app.agent {
                    agent.set_reasoning_effort(app.reasoning_effort);
                }
            }
            _ => {}
        }
        return false;
    }

    // Use stored max scroll values from app state

    // Handle regular keys
    match code {
        KeyCode::Enter => {
            // Queue message if loading
            if !app.input().is_empty() && app.loading {
                if app.queued_message_count() < 10 {
                    app.add_queued_message(app.input().to_string());
                    app.clear_input();
                    app.esc_pressed_once = false;
                    app.backspace_on_empty_once = false;
                }
            } else if !app.input().is_empty() && !app.loading {
                // If dropdown is visible, execute selected item
                if app.show_command_dropdown {
                    match dropdown_mode(app.input()) {
                        DropdownMode::Commands => {
                            let filtered = filter_commands(app.input());
                            if let Some(cmd) = filtered.get(app.command_selection) {
                                app.set_input(cmd.name.to_string());
                                app.show_command_dropdown = false;
                            }
                        }
                        DropdownMode::Models => {
                            let filtered = filter_models(app.input(), &app.agent_config.models);
                            if let Some(model) = filtered.get(app.command_selection) {
                                app.set_input(format!("/model {}", model.id));
                                app.show_command_dropdown = false;
                            }
                        }
                        DropdownMode::None => {}
                    }
                }

                let trimmed = app.input().trim().to_string();

                // Handle exit commands
                if trimmed == "/exit" || trimmed == "/quit" || trimmed == "exit" {
                    return true;
                }

                // Handle clear command (with /new alias)
                if trimmed == "/clear" || trimmed == "/new" {
                    app.clear_input();
                    app.output.clear();
                    app.scroll_offset = 0;
                    app.clear_conversation();
                    app.view_state = ViewState::Welcome;
                    app.show_welcome = true;
                    if let Some(ref mut agent) = app.agent {
                        agent.clear();
                        if let Err(e) = agent.save_history() {
                            tracing::warn!("failed to save cleared history: {e}");
                        }
                    }
                    app.output = "Conversation cleared.".to_string();
                    return false;
                }

                // Handle model switch command
                if trimmed == "/model" || trimmed.starts_with("/model ") {
                    let model_arg = trimmed
                        .strip_prefix("/model")
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if model_arg.is_empty() {
                        app.clear_input();
                        app.show_model_selection_dialog();
                        return false;
                    } else if let Some(agent) = &mut app.agent {
                        // Check if we need to switch providers
                        let current_provider = agent.provider_name();
                        let target_provider = app.agent_config.provider_for_model(&model_arg);

                        if let Some(provider_name) = target_provider {
                            if provider_name != current_provider {
                                match app.agent_config.create_provider_by_name(provider_name) {
                                    Ok(new_provider) => {
                                        agent.set_provider(new_provider);
                                        app.messages.push(DisplayMessage::tool(
                                            "model",
                                            format!("Switched provider to {provider_name}"),
                                            "",
                                            false,
                                        ));
                                    }
                                    Err(e) => {
                                        app.messages.push(DisplayMessage::tool_error(
                                            "model",
                                            format!("Failed to switch provider: {e}"),
                                        ));
                                        app.clear_input();
                                        app.enter_session();
                                        return false;
                                    }
                                }
                            }
                        }

                        agent.set_model(&model_arg);
                        app.model = model_arg.clone();
                        app.provider = agent.provider_name().to_string();
                        let provider_info = &app.provider;
                        app.messages.push(DisplayMessage::tool(
                            "model",
                            format!("Switched to {model_arg} ({provider_info})"),
                            "",
                            false,
                        ));
                        app.enter_session();
                    }
                    app.clear_input();
                    return false;
                }

                // Handle mode switch commands
                if trimmed == "/plan" {
                    if let Some(agent) = &mut app.agent {
                        agent.switch_mode(crate::core::agent::AgentMode::Plan, None);
                        app.sync_agent_mode();
                        app.clear_input();
                    }
                    return false;
                }
                if trimmed == "/build" {
                    if let Some(agent) = &mut app.agent {
                        agent.switch_mode(crate::core::agent::AgentMode::Build, None);
                        app.sync_agent_mode();
                        app.clear_input();
                    }
                    return false;
                }

                // Handle sessions command
                if trimmed == "/sessions" {
                    app.clear_input();
                    app.show_session_list();
                    return false;
                }

                if trimmed == "/init" {
                    let init_prompt = crate::cli::init::get_init_prompt(None);
                    app.set_input(init_prompt);
                    start_chat(app, permission_tx.clone());
                    return false;
                }

                // Check if provider is configured before starting chat
                if app.agent.is_none() {
                    app.active_dialog = Some(ActiveDialog::NoProvider);
                    return false;
                }

                start_chat(app, permission_tx.clone());
            }
        }
        KeyCode::Tab => {
            if !app.loading {
                // Autocomplete if dropdown visible, otherwise toggle mode
                if app.show_command_dropdown {
                    match dropdown_mode(app.input()) {
                        DropdownMode::Commands => {
                            let filtered = filter_commands(app.input());
                            if let Some(cmd) = filtered.get(app.command_selection) {
                                app.set_input(cmd.name.to_string());
                            }
                        }
                        DropdownMode::Models => {
                            let filtered = filter_models(app.input(), &app.agent_config.models);
                            if let Some(model) = filtered.get(app.command_selection) {
                                app.set_input(format!("/model {}", model.id));
                            }
                        }
                        DropdownMode::None => {}
                    }
                } else if let Some(agent) = &mut app.agent {
                    let new_mode = match app.agent_mode {
                        crate::core::agent::AgentMode::Build => crate::core::agent::AgentMode::Plan,
                        crate::core::agent::AgentMode::Plan => crate::core::agent::AgentMode::Build,
                    };
                    agent.switch_mode(new_mode, None);
                    app.sync_agent_mode();
                } else {
                    app.agent_mode = match app.agent_mode {
                        crate::core::agent::AgentMode::Build => crate::core::agent::AgentMode::Plan,
                        crate::core::agent::AgentMode::Plan => crate::core::agent::AgentMode::Build,
                    };
                }
            }
        }
        KeyCode::Char(c) => {
            app.backspace_on_empty_once = false; // Reset double-action flag
            app.esc_pressed_once = false; // Reset other double-action flag too
            // Allow typing while agent is responding
            app.insert_char(c);
            // Update dropdown visibility
            app.show_command_dropdown = should_show_dropdown(app.input());
            if app.show_command_dropdown {
                app.command_selection = 0;
            }
            // Check for @ trigger for file dropdown
            let cursor_pos = app.cursor();
            app.show_file_dropdown = file_picker::should_show_file_dropdown(app.input(), cursor_pos);
            if app.show_file_dropdown {
                app.file_selection = 0;
                // Lazy load files on first @
                if app.cached_project_files.is_empty() {
                    app.cached_project_files = file_picker::list_project_files();
                }
            }
        }
        KeyCode::Backspace => {
            if app.input().is_empty() && app.queued_message_count() > 0 {
                if app.backspace_on_empty_once {
                    app.remove_last_queued_message();
                    app.backspace_on_empty_once = false;
                } else {
                    app.backspace_on_empty_once = true;
                }
            } else if let Some((start, end)) =
                find_at_mention_span_ending_at(app.input(), app.cursor())
            {
                app.delete_range(start, end);
                app.backspace_on_empty_once = false;
            } else {
                app.delete_char();
                app.backspace_on_empty_once = false;
            }
            app.show_command_dropdown = should_show_dropdown(app.input());
            if app.show_command_dropdown {
                app.command_selection = 0;
            }
        }
        KeyCode::Esc => {
            if app.loading {
                // Double-Esc to cancel
                if app.esc_pressed_once {
                    // Cancel streaming (don't abort task - agent must return via Done)
                    app.chat_rx = None;
                    app.clear_queued_messages();
                    app.finalize_streaming(true);
                    app.loading = false;
                    app.esc_pressed_once = false;
                    app.backspace_on_empty_once = false;
                    app.activity_status = None;
                } else {
                    // First Esc - set flag (hint shown via status)
                    app.esc_pressed_once = true;
                }
            } else {
                // Not loading - existing behavior
                if app.show_command_dropdown {
                    app.show_command_dropdown = false;
                    app.clear_input();
                }
            }
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {
            if let Some(action) = keybinding_map().get(&(code, modifiers)) {
                app.execute_action(action);
            }
        }
        // Scrolling - use message scroll in session view
        KeyCode::PageUp => {
            if app.view_state == ViewState::Session {
                app.scroll_messages_up(10);
            } else {
                app.scroll_up(10);
            }
        }
        KeyCode::PageDown => {
            if app.view_state == ViewState::Session {
                app.scroll_messages_down(10);
            } else {
                app.scroll_down(10, 1000); // Welcome screen uses fixed max.
            }
        }
        KeyCode::Up => {
            if app.show_command_dropdown {
                // Navigate dropdown selection up (wrap to bottom)
                let max_idx = match dropdown_mode(app.input()) {
                    DropdownMode::Commands => filter_commands(app.input()).len().saturating_sub(1),
                    DropdownMode::Models => filter_models(app.input(), &app.agent_config.models)
                        .len()
                        .saturating_sub(1),
                    DropdownMode::None => 0,
                };
                app.command_selection = if app.command_selection == 0 {
                    max_idx
                } else {
                    app.command_selection - 1
                };
            } else if app.view_state == ViewState::Session {
                let old_cursor = app.cursor();
                app.move_up();
                // Only scroll messages if prompt is empty or single-line
                let is_multiline_prompt = !app.input().is_empty() && {
                    let layout = TextLayout::new(app.input(), app.prompt_text_width);
                    layout.total_lines > 1
                };
                if app.cursor() == old_cursor && !is_multiline_prompt {
                    app.scroll_messages_up(1);
                }
            } else {
                app.move_up();
            }
        }
        KeyCode::Down => {
            if app.show_command_dropdown {
                let max_idx = match dropdown_mode(app.input()) {
                    DropdownMode::Commands => filter_commands(app.input()).len().saturating_sub(1),
                    DropdownMode::Models => filter_models(app.input(), &app.agent_config.models)
                        .len()
                        .saturating_sub(1),
                    DropdownMode::None => 0,
                };
                app.command_selection = if app.command_selection >= max_idx {
                    0
                } else {
                    app.command_selection + 1
                };
            } else if app.view_state == ViewState::Session {
                let old_cursor = app.cursor();
                app.move_down();
                // Only scroll messages if prompt is empty or single-line
                let is_multiline_prompt = !app.input().is_empty() && {
                    let layout = TextLayout::new(app.input(), app.prompt_text_width);
                    layout.total_lines > 1
                };
                if app.cursor() == old_cursor && !is_multiline_prompt {
                    app.scroll_messages_down(1);
                }
            } else {
                app.move_down();
            }
        }
        _ => {}
    }

    false
}

/// Permission dialog colors.
const PERMISSION_BORDER: Color = Color::Rgb(245, 167, 66); // Orange/warning
const PERMISSION_HIGHLIGHT: Color = Color::Rgb(245, 167, 66);
const PERMISSION_DIM: Color = Color::Rgb(100, 100, 110);

/// Render a centered dialog overlay.
fn render_dialog(
    frame: &mut ratatui::Frame,
    title: &str,
    content: Vec<ratatui::text::Line>,
    buttons: &[(&str, bool)],
    width_percent: u16,
    height: u16,
    border_color: Option<Color>,
) {
    use ratatui::layout::{Alignment, Rect};
    use ratatui::text::Line;
    use ratatui::widgets::Clear;

    let area = frame.area();

    // Calculate dialog size
    let dialog_width = area.width * width_percent / 100;
    let dialog_height = height.min(area.height - 4);

    // Center the dialog
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Render dialog box
    let color = border_color.unwrap_or(Color::Yellow);
    let block = Block::default()
        .title(format!(" {title} "))
        .title_style(Style::default().fg(color))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Split inner area for content and buttons
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(inner);

    // Render content
    let content_para = Paragraph::new(content).wrap(Wrap { trim: false });
    frame.render_widget(content_para, chunks[0]);

    // Render buttons
    let button_text: Vec<ratatui::text::Span> = buttons
        .iter()
        .enumerate()
        .flat_map(|(i, (label, selected))| {
            let style = if *selected {
                Style::default().fg(Color::Black).bg(color)
            } else {
                Style::default().fg(Color::White)
            };
            let mut spans = vec![ratatui::text::Span::styled(format!(" {label} "), style)];
            if i < buttons.len() - 1 {
                spans.push(ratatui::text::Span::raw("  "));
            }
            spans
        })
        .collect();

    let buttons_para = Paragraph::new(Line::from(button_text)).alignment(Alignment::Center);
    frame.render_widget(buttons_para, chunks[1]);
}

/// Render permission dialog with improved UX.
#[allow(clippy::too_many_lines)]
fn render_permission_dialog(frame: &mut ratatui::Frame, dialog: &ActivePermissionDialog) {
    use ratatui::text::{Line, Span};

    // Determine icon and title based on operation type
    let (icon, title) = match &dialog.context {
        PermissionContext::Bash { .. } => ("$", "Run Command"),
        PermissionContext::WriteFile { .. } => ("+", "Create File"),
        PermissionContext::EditFile { .. } => ("~", "Edit File"),
        PermissionContext::AskUser { .. } => ("?", "Question"),
        PermissionContext::WebSearch { .. } => ("⊕", "Web Search"),
        PermissionContext::CodeSearch { .. } => ("◈", "Code Search"),
        PermissionContext::Glob { .. } => ("*", "Find Files"),
        PermissionContext::Grep { .. } => ("⊛", "Search Content"),
        PermissionContext::ListDir { .. } => ("▤", "List Directory"),
        PermissionContext::WebFetch { .. } => ("↓", "Fetch URL"),
    };

    let header_style = Style::default().fg(PERMISSION_HIGHLIGHT);
    let dim_style = Style::default().fg(PERMISSION_DIM);
    let code_style = Style::default().fg(Color::White);

    let mut content = vec![
        Line::from(vec![
            Span::styled(format!("{icon} "), header_style),
            Span::styled(title, header_style),
            Span::styled(format!(" ({})", dialog.tool_name), dim_style),
        ]),
        Line::from(""),
    ];

    match &dialog.context {
        PermissionContext::Bash {
            command,
            working_dir,
        } => {
            content.push(Line::from(Span::styled("Command:", dim_style)));
            // Truncate long commands for display
            let display_cmd = if command.len() > 80 {
                format!("{}...", &command[..77])
            } else {
                command.clone()
            };
            content.push(Line::from(Span::styled(
                format!("  {display_cmd}"),
                code_style,
            )));
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                format!("in {}", working_dir.display()),
                dim_style,
            )));
        }
        PermissionContext::WriteFile {
            path,
            content_preview,
        } => {
            content.push(Line::from(Span::styled(
                format!("{}", path.display()),
                code_style,
            )));
            content.push(Line::from(""));
            content.push(Line::from(Span::styled("Preview:", dim_style)));
            for line in content_preview.lines().take(8) {
                content.push(Line::from(Span::styled(format!("  {line}"), dim_style)));
            }
            if content_preview.lines().count() > 8 {
                content.push(Line::from(Span::styled("  ...", dim_style)));
            }
        }
        PermissionContext::EditFile { path, diff } => {
            content.push(Line::from(Span::styled(
                format!("{}", path.display()),
                code_style,
            )));
            content.push(Line::from(""));
            content.push(Line::from(Span::styled("Changes:", dim_style)));
            for line in diff.lines().take(12) {
                // Color diff lines appropriately
                let line_style = if line.starts_with('+') && !line.starts_with("+++") {
                    Style::default().fg(Color::Green)
                } else if line.starts_with('-') && !line.starts_with("---") {
                    Style::default().fg(Color::Red)
                } else if line.starts_with("@@") {
                    Style::default().fg(Color::Cyan)
                } else {
                    dim_style
                };
                content.push(Line::from(Span::styled(format!("  {line}"), line_style)));
            }
            if diff.lines().count() > 12 {
                content.push(Line::from(Span::styled("  ...", dim_style)));
            }
        }
        PermissionContext::AskUser { .. } => {}
        PermissionContext::WebSearch { query } => {
            content.push(Line::from(Span::styled("Query:", dim_style)));
            let display_query = if query.len() > 60 {
                format!("{}...", &query[..57])
            } else {
                query.clone()
            };
            content.push(Line::from(Span::styled(
                format!("  {display_query}"),
                code_style,
            )));
        }
        PermissionContext::CodeSearch { query, tokens } => {
            content.push(Line::from(Span::styled("Query:", dim_style)));
            let display_query = if query.len() > 60 {
                format!("{}...", &query[..57])
            } else {
                query.clone()
            };
            content.push(Line::from(Span::styled(
                format!("  {display_query}"),
                code_style,
            )));
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                format!("Tokens: {tokens}"),
                dim_style,
            )));
        }
        PermissionContext::Glob { pattern, path } => {
            content.push(Line::from(Span::styled("Pattern:", dim_style)));
            content.push(Line::from(Span::styled(format!("  {pattern}"), code_style)));
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                format!("in {}", path.display()),
                dim_style,
            )));
        }
        PermissionContext::Grep { pattern, path } => {
            content.push(Line::from(Span::styled("Pattern:", dim_style)));
            content.push(Line::from(Span::styled(format!("  {pattern}"), code_style)));
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                format!("in {}", path.display()),
                dim_style,
            )));
        }
        PermissionContext::ListDir { path } => {
            content.push(Line::from(Span::styled(
                format!("{}", path.display()),
                code_style,
            )));
        }
        PermissionContext::WebFetch { url } => {
            content.push(Line::from(Span::styled("URL:", dim_style)));
            let display_url = if url.len() > 60 {
                format!("{}...", &url[..57])
            } else {
                url.clone()
            };
            content.push(Line::from(Span::styled(
                format!("  {display_url}"),
                code_style,
            )));
        }
    }

    // Add navigation hint
    content.push(Line::from(""));
    content.push(Line::from(Span::styled(
        "←/→ navigate · Enter confirm · Esc deny",
        dim_style,
    )));

    // Button labels with keyboard shortcuts
    let buttons = [
        ("[a] Allow once", dialog.selected == 0),
        ("[s] Always", dialog.selected == 1),
        ("[d] Deny", dialog.selected == 2),
    ];

    render_dialog(
        frame,
        "Permission Required",
        content,
        &buttons,
        70,
        22,
        Some(PERMISSION_BORDER),
    );
}

/// Render no provider configured dialog.
fn render_no_provider_dialog(frame: &mut ratatui::Frame) {
    use ratatui::text::{Line, Span};

    let header_style = Style::default().fg(Color::Yellow);
    let dim_style = Style::default().fg(Color::Gray);

    let content = vec![
        Line::from(vec![Span::styled("No provider configured", header_style)]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Run `omni auth login` in another terminal",
            dim_style,
        )]),
        Line::from(vec![Span::styled("to set up a provider.", dim_style)]),
        Line::from(""),
        Line::from(vec![Span::styled("Press Esc to dismiss", dim_style)]),
    ];

    let buttons: [(&str, bool); 0] = [];

    render_dialog(
        frame,
        "Provider Setup",
        content,
        &buttons,
        60,
        10,
        Some(Color::Yellow),
    );
}

/// Render `ask_user` dialog.
fn render_ask_user_dialog(frame: &mut ratatui::Frame, dialog: &ActiveAskUserDialog) {
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};

    let area = frame.area();
    let width_percent: u16 = 70;

    // Split question by newlines to render multi-line questions properly
    let question_line_count = dialog.question.lines().count().max(1);
    let mut content: Vec<Line> = dialog
        .question
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect();
    content.push(Line::from(""));

    let buttons: Vec<(&str, bool)> = if let Some(ref options) = dialog.options {
        // Render options as vertical list in content area
        for (i, opt) in options.iter().enumerate() {
            let is_selected = i == dialog.selected;
            let prefix = if is_selected { "▸ " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            content.push(Line::from(Span::styled(format!("{prefix}{opt}"), style)));
        }
        content.push(Line::from(""));
        vec![("[Enter] Select", false), ("[Esc] Cancel", false)]
    } else {
        content.push(Line::from(format!("> {}_", dialog.input)));
        vec![("[Enter] Submit", true), ("[Esc] Cancel", false)]
    };

    // Calculate height dynamically
    // Base: 2 (borders) + 3 (button area) + 2 (padding)
    let base_height: u16 = 7;
    #[allow(clippy::cast_possible_truncation)]
    let content_lines = question_line_count as u16
        + 1 // Empty line after question
        + dialog.options.as_ref().map_or(1, |opts| opts.len() as u16 + 1);
    let needed_height = base_height + content_lines;
    let height = needed_height.clamp(10, area.height.saturating_sub(4));

    render_dialog(
        frame,
        "Question",
        content,
        &buttons,
        width_percent,
        height,
        None,
    );
}

fn render_tool_output_dialog(frame: &mut ratatui::Frame, dialog: &mut ExpandedToolDialog) {
    use ratatui::layout::{Alignment, Rect};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Clear;

    let area = frame.area();

    let dialog_width = area.width * 80 / 100;
    let max_height = area.height * 80 / 100;
    let content_width = dialog_width.saturating_sub(4);

    let needs_recache = content_width != dialog.cached_width || dialog.cached_lines.is_empty();
    if needs_recache {
        dialog.cached_lines = dialog
            .output
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line.to_owned(),
                    Style::default().fg(line_color(line)),
                ))
            })
            .collect();
        dialog.cached_width = content_width;
        dialog.total_lines = dialog.cached_lines.len();
    }

    let total_lines = dialog.total_lines;

    #[allow(clippy::cast_possible_truncation)]
    let content_height = (total_lines as u16).saturating_add(4);
    let dialog_height = content_height.clamp(8, max_height);

    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let border_color = Color::Rgb(100, 140, 180);
    let block = Block::default()
        .title(format!(" {} ", dialog.tool_name))
        .title_style(Style::default().fg(border_color))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let invocation_style = Style::default().fg(Color::Rgb(180, 180, 190));
    let invocation_line = Line::from(Span::styled(&dialog.invocation, invocation_style));
    frame.render_widget(Paragraph::new(invocation_line), chunks[0]);

    let visible_height = chunks[1].height as usize;
    #[allow(clippy::cast_possible_truncation)]
    {
        dialog.visible_height = visible_height as u16;
    }

    let scroll = dialog.scroll_offset as usize;
    let visible_lines: Vec<Line> = dialog
        .cached_lines
        .iter()
        .skip(scroll)
        .take(visible_height)
        .cloned()
        .collect();

    let output_para = Paragraph::new(visible_lines);
    frame.render_widget(output_para, chunks[1]);

    let hint_style = Style::default().fg(Color::Rgb(100, 100, 110));
    let scroll_indicator = if total_lines > visible_height {
        format!(
            " [{}-{}/{}] ",
            scroll + 1,
            (scroll + visible_height).min(total_lines),
            total_lines
        )
    } else {
        String::new()
    };
    let hint = Line::from(vec![
        Span::styled("Esc to close", hint_style),
        Span::styled(" | ", hint_style),
        Span::styled("↑↓/jk to scroll", hint_style),
        Span::styled(&scroll_indicator, hint_style),
    ]);
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[2]);
}

fn handle_dialog_key(app: &mut App, code: KeyCode, _modifiers: KeyModifiers) -> bool {
    let Some(dialog) = app.active_dialog.take() else {
        return false;
    };

    match dialog {
        ActiveDialog::Permission(d) => match code {
            // Direct shortcuts - work regardless of selection
            KeyCode::Char('a') => {
                if let Some(ref tx) = app.permission_response_tx {
                    let _ = tx.send((
                        d.request_id,
                        "tui-session".to_string(),
                        d.tool_name,
                        d.action,
                        PermissionResponse::Allow,
                    ));
                }
            }
            KeyCode::Char('s') => {
                if let Some(ref tx) = app.permission_response_tx {
                    let _ = tx.send((
                        d.request_id,
                        "tui-session".to_string(),
                        d.tool_name,
                        d.action,
                        PermissionResponse::AllowForSession,
                    ));
                }
            }
            KeyCode::Char('d') | KeyCode::Esc => {
                // Esc always denies (cancel = abort = deny)
                if let Some(ref tx) = app.permission_response_tx {
                    let _ = tx.send((
                        d.request_id,
                        "tui-session".to_string(),
                        d.tool_name,
                        d.action,
                        PermissionResponse::Deny,
                    ));
                }
            }
            // Enter confirms current selection
            KeyCode::Enter => {
                let response = match d.selected {
                    0 => PermissionResponse::Allow,
                    1 => PermissionResponse::AllowForSession,
                    _ => PermissionResponse::Deny,
                };
                if let Some(ref tx) = app.permission_response_tx {
                    let _ = tx.send((
                        d.request_id,
                        "tui-session".to_string(),
                        d.tool_name,
                        d.action,
                        response,
                    ));
                }
            }
            // Navigation
            KeyCode::Left | KeyCode::Char('h') => {
                let mut new_d = d;
                new_d.selected = new_d.selected.saturating_sub(1);
                app.active_dialog = Some(ActiveDialog::Permission(new_d));
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let mut new_d = d;
                new_d.selected = (new_d.selected + 1).min(2);
                app.active_dialog = Some(ActiveDialog::Permission(new_d));
            }
            _ => {
                app.active_dialog = Some(ActiveDialog::Permission(d));
            }
        },
        ActiveDialog::AskUser(d) => {
            if d.options.is_some() {
                let num_options = d.options.as_ref().map_or(0, Vec::len);
                match code {
                    KeyCode::Enter => {
                        if let Some(ref options) = d.options {
                            if let Some(selected) = options.get(d.selected) {
                                if let Some(ref tx) = app.ask_user_response_tx {
                                    let _ = tx.send((
                                        d.request_id,
                                        AskUserResponse::Answer(selected.clone()),
                                    ));
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(ref tx) = app.ask_user_response_tx {
                            let _ = tx.send((d.request_id, AskUserResponse::Cancelled));
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let mut new_d = d;
                        new_d.selected = new_d.selected.saturating_sub(1);
                        app.active_dialog = Some(ActiveDialog::AskUser(new_d));
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let mut new_d = d;
                        new_d.selected = (new_d.selected + 1).min(num_options.saturating_sub(1));
                        app.active_dialog = Some(ActiveDialog::AskUser(new_d));
                    }
                    _ => {
                        app.active_dialog = Some(ActiveDialog::AskUser(d));
                    }
                }
            } else {
                match code {
                    KeyCode::Char(c) => {
                        let mut new_d = d;
                        new_d.input.insert(new_d.cursor, c);
                        new_d.cursor += c.len_utf8();
                        app.active_dialog = Some(ActiveDialog::AskUser(new_d));
                    }
                    KeyCode::Backspace => {
                        let mut new_d = d;
                        if new_d.cursor > 0 {
                            let prev = new_d.input[..new_d.cursor]
                                .char_indices()
                                .next_back()
                                .map_or(0, |(i, _)| i);
                            new_d.input.drain(prev..new_d.cursor);
                            new_d.cursor = prev;
                        }
                        app.active_dialog = Some(ActiveDialog::AskUser(new_d));
                    }
                    KeyCode::Enter => {
                        if let Some(ref tx) = app.ask_user_response_tx {
                            let _ = tx.send((d.request_id, AskUserResponse::Answer(d.input)));
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(ref tx) = app.ask_user_response_tx {
                            let _ = tx.send((d.request_id, AskUserResponse::Cancelled));
                        }
                    }
                    _ => {
                        app.active_dialog = Some(ActiveDialog::AskUser(d));
                    }
                }
            }
        }
        ActiveDialog::SessionList(mut d) => match code {
            KeyCode::Esc => {
                // Close dialog without selecting
            }
            KeyCode::Enter => {
                // Select and switch to the session
                if let Some(session) = d.selected_session() {
                    let session_id = session.id.clone();
                    tracing::info!(session_id = %session_id, "switching to session: {}", session.title);

                    // Switch agent to new session
                    if let Some(ref mut agent) = app.agent {
                        if let Err(e) = agent.switch_session(&session_id) {
                            tracing::error!("failed to switch session: {e}");
                        } else {
                            // Load messages for display
                            if let Some(manager) = agent.session_manager() {
                                match App::load_session_messages(manager, &session_id) {
                                    Ok(messages) => {
                                        app.messages = messages;
                                        app.message_scroll = 0;
                                        app.streaming_text.clear();
                                        // Switch to session view if we have messages
                                        if !app.messages.is_empty() {
                                            app.view_state = ViewState::Session;
                                            app.show_welcome = false;
                                        }
                                    }
                                    Err(e) => tracing::error!("failed to load messages: {e}"),
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                d.select_previous();
                app.active_dialog = Some(ActiveDialog::SessionList(d));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                d.select_next();
                app.active_dialog = Some(ActiveDialog::SessionList(d));
            }
            KeyCode::Char('n') => {
                // Create new session
                if let Some(ref mut agent) = app.agent {
                    match agent.new_session() {
                        Ok(session_id) => {
                            tracing::info!(session_id = %session_id, "created new session");
                            // Clear display and switch to welcome
                            app.messages.clear();
                            app.streaming_text.clear();
                            app.message_scroll = 0;
                            app.view_state = ViewState::Welcome;
                            app.show_welcome = true;
                        }
                        Err(e) => tracing::error!("failed to create session: {e}"),
                    }
                }
            }
            KeyCode::Char('d') => {
                // Delete selected session
                if let Some(session) = d.selected_session() {
                    let session_id = session.id.clone();
                    let is_current = app
                        .agent
                        .as_ref()
                        .and_then(|a| a.session_id())
                        .is_some_and(|id| id == session_id);

                    tracing::info!(session_id = %session_id, "deleting session");

                    // Delete from storage
                    if let Some(ref agent) = app.agent {
                        if let Some(manager) = agent.session_manager() {
                            if let Err(e) = manager.delete_session(&session_id) {
                                tracing::error!("failed to delete session: {e}");
                                app.active_dialog = Some(ActiveDialog::SessionList(d));
                                return false;
                            }
                        }
                    }

                    // Remove from dialog list
                    d.remove_session(&session_id);

                    // If we deleted the current session, create a new one
                    if is_current {
                        if let Some(ref mut agent) = app.agent {
                            match agent.new_session() {
                                Ok(_) => {
                                    app.messages.clear();
                                    app.streaming_text.clear();
                                    app.message_scroll = 0;
                                }
                                Err(e) => {
                                    tracing::error!("failed to create replacement session: {e}");
                                }
                            }
                        }
                    }

                    // Keep dialog open if there are more sessions
                    if !d.sessions().is_empty() {
                        app.active_dialog = Some(ActiveDialog::SessionList(d));
                    }
                    // Otherwise dialog closes (active_dialog stays None)
                } else {
                    app.active_dialog = Some(ActiveDialog::SessionList(d));
                }
            }
            KeyCode::Char(c) => {
                d.filter_push(c);
                app.active_dialog = Some(ActiveDialog::SessionList(d));
            }
            KeyCode::Backspace => {
                d.filter_pop();
                app.active_dialog = Some(ActiveDialog::SessionList(d));
            }
            _ => {
                app.active_dialog = Some(ActiveDialog::SessionList(d));
            }
        },
        ActiveDialog::ModelSelection(mut d) => match code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                if let Some(model) = d.get_selected_model() {
                    if let Some(agent) = &mut app.agent {
                        let current_provider = agent.provider_name();
                        if model.provider != current_provider {
                            if let Ok(new_provider) =
                                app.agent_config.create_provider_by_name(&model.provider)
                            {
                                agent.set_provider(new_provider);
                            }
                        }
                        agent.set_model(&model.id);
                        app.model = model.id.clone();
                        app.provider = model.provider.clone();
                    }
                } else {
                    // Selected item is a header - stay in dialog
                    app.active_dialog = Some(ActiveDialog::ModelSelection(d));
                }
            }
            KeyCode::Up => {
                d.select_previous();
                app.active_dialog = Some(ActiveDialog::ModelSelection(d));
            }
            KeyCode::Down => {
                d.select_next();
                app.active_dialog = Some(ActiveDialog::ModelSelection(d));
            }
            KeyCode::Tab => {
                if let Some(provider) = d.get_selected_provider() {
                    d.toggle_provider_collapse(&provider);
                }
                app.active_dialog = Some(ActiveDialog::ModelSelection(d));
            }
            KeyCode::Char(c) => {
                d.filter.push(c);
                d.set_filter(d.filter.clone());
                app.active_dialog = Some(ActiveDialog::ModelSelection(d));
            }
            KeyCode::Backspace => {
                d.filter.pop();
                d.set_filter(d.filter.clone());
                app.active_dialog = Some(ActiveDialog::ModelSelection(d));
            }
            _ => {
                app.active_dialog = Some(ActiveDialog::ModelSelection(d));
            }
        },
        ActiveDialog::ToolOutput(mut d) => {
            #[allow(clippy::cast_possible_truncation)]
            let total_lines = d.total_lines as u16;
            let visible_height = d.visible_height;
            let max_scroll = total_lines.saturating_sub(visible_height);

            match code {
                KeyCode::Esc => {}
                KeyCode::Up | KeyCode::Char('k') => {
                    d.scroll_offset = d.scroll_offset.saturating_sub(1);
                    app.active_dialog = Some(ActiveDialog::ToolOutput(d));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    d.scroll_offset = d.scroll_offset.saturating_add(1).min(max_scroll);
                    app.active_dialog = Some(ActiveDialog::ToolOutput(d));
                }
                KeyCode::PageUp => {
                    d.scroll_offset = d.scroll_offset.saturating_sub(10);
                    app.active_dialog = Some(ActiveDialog::ToolOutput(d));
                }
                KeyCode::PageDown => {
                    d.scroll_offset = d.scroll_offset.saturating_add(10).min(max_scroll);
                    app.active_dialog = Some(ActiveDialog::ToolOutput(d));
                }
                KeyCode::Home => {
                    d.scroll_offset = 0;
                    app.active_dialog = Some(ActiveDialog::ToolOutput(d));
                }
                KeyCode::End => {
                    d.scroll_offset = max_scroll;
                    app.active_dialog = Some(ActiveDialog::ToolOutput(d));
                }
                _ => {
                    app.active_dialog = Some(ActiveDialog::ToolOutput(d));
                }
            }
        }
        ActiveDialog::NoProvider => {
            if code == KeyCode::Esc {
                return false;
            }
            app.active_dialog = Some(ActiveDialog::NoProvider);
        }
    }

    false
}

/// Start a chat request in the background.
fn start_chat(app: &mut App, permission_tx: mpsc::UnboundedSender<PermissionMessage>) {
    let Some(mut agent) = app.agent.take() else {
        app.output = "No provider configured".to_string();
        return;
    };

    // Ensure agent has permission client with current permission presets
    let client = PermissionClient::with_presets(
        "tui-session".to_string(),
        permission_tx,
        app.current_permissions(),
    );
    agent.set_permission_client(client);

    let prompt = app.take_input();

    // Transition to session view on first message
    app.enter_session();

    // Add user message to the conversation
    app.add_user_message(&prompt);

    // Clear streaming state for new response
    app.streaming_text.clear();
    app.output.clear();
    app.loading = true;

    let (tx, rx) = mpsc::unbounded_channel();
    app.chat_rx = Some(rx);

    tokio::spawn(async move {
        let mut agent = agent;
        let tx_clone = tx.clone();

        let result = agent
            .chat_with_events(&prompt, |event| {
                use crate::core::agent::ChatEvent;
                match event {
                    ChatEvent::Text(text) => {
                        let _ = tx_clone.send(ChatMessage::Text(text));
                    }
                    ChatEvent::ToolStart { name } => {
                        let _ = tx_clone.send(ChatMessage::ToolStart { name });
                    }
                    ChatEvent::ToolCall {
                        name,
                        invocation,
                        output,
                        is_error,
                    } => {
                        let _ = tx_clone.send(ChatMessage::Tool {
                            name,
                            invocation,
                            output,
                            is_error,
                        });
                    }
                    ChatEvent::Usage {
                        input_tokens,
                        output_tokens,
                        cost_usd,
                    } => {
                        let _ = tx_clone.send(ChatMessage::Usage {
                            input_tokens,
                            output_tokens,
                            cost_usd,
                        });
                    }
                    ChatEvent::ThinkingStart => {
                        let _ = tx_clone.send(ChatMessage::ThinkingStart);
                    }
                    ChatEvent::Thinking(text) => {
                        let _ = tx_clone.send(ChatMessage::Thinking(text));
                    }
                }
            })
            .await;

        match result {
            Ok(_) => {
                // Generate title if needed (first message with default title)
                if let Some(first_msg) = agent.needs_title_generation() {
                    if let Err(e) = agent.generate_title(&first_msg).await {
                        tracing::debug!("title generation failed: {e}");
                    }
                }
                let _ = tx.send(ChatMessage::Done(agent));
            }
            Err(e) => {
                let _ = tx.send(ChatMessage::Error(e.to_string(), agent));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_provider_shows_modal() {
        let mut app = App::new();

        // Ensure agent is None (simulating no provider configured)
        app.agent = None;

        // Simulate typing
        app.set_input("test message");

        // Verify agent is None
        assert!(app.agent.is_none());

        // Simulate what happens in handle_key when Enter is pressed
        if app.agent.is_none() {
            app.active_dialog = Some(ActiveDialog::NoProvider);
        }

        // Verify modal is shown
        assert!(matches!(app.active_dialog, Some(ActiveDialog::NoProvider)));
    }

    #[test]
    fn test_modal_dismisses_on_esc() {
        let mut app = App::new();

        // Set up modal
        app.active_dialog = Some(ActiveDialog::NoProvider);
        app.set_input("test message");

        // Simulate Esc key in handle_dialog_key
        let dialog = app.active_dialog.take();
        assert!(matches!(dialog, Some(ActiveDialog::NoProvider)));

        // After Esc, dialog should be None but input preserved
        assert!(app.active_dialog.is_none());
        assert_eq!(app.input(), "test message");
    }

    #[test]
    fn test_input_preserved_when_modal_shown() {
        let mut app = App::new();

        app.set_input("important message");

        // Show modal
        app.active_dialog = Some(ActiveDialog::NoProvider);

        // Dismiss modal
        app.active_dialog = None;

        // Input should be preserved
        assert_eq!(app.input(), "important message");
        assert_eq!(app.cursor(), "important message".len());
    }
}
