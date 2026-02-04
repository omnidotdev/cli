//! Command palette dropdown for slash commands.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::config::ModelInfo;

/// Brand colors
const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const DROPDOWN_BG: Color = Color::Rgb(35, 38, 45);

/// Max width for centered UI elements (input box, command palette)
pub const CENTERED_MAX_WIDTH: u16 = 72;

/// A slash command.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    /// Command name (e.g., "/exit").
    pub name: &'static str,
    /// Description shown in dropdown.
    pub description: &'static str,
}

/// Available commands
pub const COMMANDS: &[Command] = &[
    Command {
        name: "/model",
        description: "Switch AI model",
    },
    Command {
        name: "/clear",
        description: "Clear conversation history",
    },
    Command {
        name: "/new",
        description: "Start new conversation (alias for /clear)",
    },
    Command {
        name: "/sessions",
        description: "Browse and switch sessions",
    },
    Command {
        name: "/plan",
        description: "Switch to plan mode",
    },
    Command {
        name: "/build",
        description: "Switch to build mode",
    },
    Command {
        name: "/exit",
        description: "Exit the application",
    },
    Command {
        name: "/quit",
        description: "Exit the application",
    },
];

/// Filter commands by prefix (case-insensitive).
#[must_use]
pub fn filter_commands(input: &str) -> Vec<&'static Command> {
    let input_lower = input.to_lowercase();
    COMMANDS
        .iter()
        .filter(|cmd| cmd.name.to_lowercase().starts_with(&input_lower))
        .collect()
}

/// Dropdown mode for the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropdownMode {
    /// Show command suggestions (e.g., "/cl" -> /clear)
    Commands,
    /// Show model suggestions (e.g., "/model gpt" -> gpt-4o)
    Models,
    /// No dropdown
    None,
}

/// Determine the dropdown mode based on input.
#[must_use]
pub fn dropdown_mode(input: &str) -> DropdownMode {
    if input.starts_with("/model ") {
        DropdownMode::Models
    } else if input.starts_with('/') && !input.contains(' ') {
        DropdownMode::Commands
    } else {
        DropdownMode::None
    }
}

/// Check if input should show the command dropdown.
#[must_use]
pub fn should_show_dropdown(input: &str) -> bool {
    dropdown_mode(input) != DropdownMode::None
}

/// Filter models by query (case-insensitive).
#[must_use]
pub fn filter_models<'a>(input: &str, models: &'a [ModelInfo]) -> Vec<&'a ModelInfo> {
    let query = input
        .strip_prefix("/model ")
        .unwrap_or("")
        .trim()
        .to_lowercase();

    if query.is_empty() {
        models.iter().collect()
    } else {
        models
            .iter()
            .filter(|m| m.id.to_lowercase().contains(&query))
            .collect()
    }
}

/// Render the command dropdown above the prompt.
///
/// Returns the height used by the dropdown.
#[allow(clippy::cast_possible_truncation)]
pub fn render_command_dropdown(
    frame: &mut Frame,
    prompt_area: Rect,
    input: &str,
    selected: usize,
) -> u16 {
    let filtered = filter_commands(input);

    // Build content lines
    let lines: Vec<Line> = if filtered.is_empty() {
        vec![Line::from(Span::styled(
            format!("  No commands matching '{input}'"),
            Style::default().fg(DIMMED),
        ))]
    } else {
        filtered
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let is_selected = i == selected;
                let prefix = if is_selected { "▸ " } else { "  " };

                let name_style = if is_selected {
                    Style::default().fg(BRAND_TEAL).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(BRAND_TEAL)
                };

                let desc_style = if is_selected {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(DIMMED)
                };

                Line::from(vec![
                    Span::styled(prefix, name_style),
                    Span::styled(cmd.name, name_style),
                    Span::raw("  "),
                    Span::styled(cmd.description, desc_style),
                ])
            })
            .collect()
    };

    let content_lines = lines.len().max(1);
    let dropdown_height = (content_lines + 2) as u16; // +2 for borders
    let dropdown_width = prompt_area.width;

    // Position directly above the prompt (no gap)
    let dropdown_y = prompt_area.y.saturating_sub(dropdown_height);
    let dropdown_x = prompt_area.x;

    let dropdown_area = Rect::new(dropdown_x, dropdown_y, dropdown_width, dropdown_height);

    // Clear area behind dropdown
    frame.render_widget(Clear, dropdown_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIMMED))
        .style(Style::default().bg(DROPDOWN_BG));

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, dropdown_area);

    dropdown_height
}

/// Render the model dropdown above the prompt.
///
/// Returns the height used by the dropdown.
#[allow(clippy::cast_possible_truncation)]
pub fn render_model_dropdown(
    frame: &mut Frame,
    prompt_area: Rect,
    input: &str,
    selected: usize,
    models: &[ModelInfo],
) -> u16 {
    let filtered = filter_models(input, models);

    let lines: Vec<Line> = if filtered.is_empty() {
        let query = input.strip_prefix("/model ").unwrap_or("").trim();
        vec![Line::from(Span::styled(
            format!("  No models matching '{query}'"),
            Style::default().fg(DIMMED),
        ))]
    } else {
        filtered
            .iter()
            .enumerate()
            .map(|(i, model)| {
                let is_selected = i == selected;
                let prefix = if is_selected { "▸ " } else { "  " };

                let name_style = if is_selected {
                    Style::default().fg(BRAND_TEAL).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(BRAND_TEAL)
                };

                let provider_style = if is_selected {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(DIMMED)
                };

                Line::from(vec![
                    Span::styled(prefix, name_style),
                    Span::styled(&model.id, name_style),
                    Span::raw("  "),
                    Span::styled(format!("({})", model.provider), provider_style),
                ])
            })
            .collect()
    };

    let content_lines = lines.len().max(1);
    let dropdown_height = (content_lines + 2) as u16;
    let dropdown_width = prompt_area.width;

    let dropdown_y = prompt_area.y.saturating_sub(dropdown_height);
    let dropdown_x = prompt_area.x;

    let dropdown_area = Rect::new(dropdown_x, dropdown_y, dropdown_width, dropdown_height);

    frame.render_widget(Clear, dropdown_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIMMED))
        .style(Style::default().bg(DROPDOWN_BG));

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, dropdown_area);

    dropdown_height
}
