//! Message rendering components.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::markdown::parse_markdown_line;
use crate::tui::message::{DisplayMessage, icons, tool_icon};

/// Brand colors
const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const PANEL_BG: Color = Color::Rgb(18, 19, 22);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const ERROR_COLOR: Color = Color::Red;
const SUCCESS_COLOR: Color = Color::Rgb(77, 201, 176);
const SELECTION_BG: Color = Color::Rgb(60, 80, 100);
const SELECTION_FG: Color = Color::White;

/// Continuation character for tool output
const CONT_CHAR: &str = "⎿";

/// Render a user message with teal border, panel background, and vertical padding
///
/// Returns the height used.
#[allow(clippy::cast_possible_truncation)]
fn render_user_message(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) -> u16 {
    // Calculate actual height needed, accounting for line wrapping
    // Subtract 1 for the left border
    let width = area.width.saturating_sub(1).max(1) as usize;
    let content_height: u16 = text
        .lines()
        .map(|line| wrapped_line_height(line.chars().count(), width))
        .sum::<u16>()
        .max(1);
    // Add 2 for top and bottom padding
    let height = (content_height + 2).min(area.height);

    // Build lines with selection highlighting, adding vertical padding
    let mut lines: Vec<Line> = vec![Line::from("")]; // Top padding
    for (i, line_text) in text.lines().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let line_y = area.y + 1 + i as u16; // +1 for top padding
        let is_selected =
            selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

        if is_selected {
            if !selected_text.is_empty() {
                selected_text.push('\n');
            }
            selected_text.push_str(line_text);
            lines.push(Line::from(Span::styled(
                line_text,
                Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
            )));
        } else {
            lines.push(Line::from(line_text));
        }
    }
    lines.push(Line::from("")); // Bottom padding

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(BRAND_TEAL))
        .style(Style::default().bg(PANEL_BG));

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    let render_area = Rect::new(area.x, area.y, area.width, height);
    frame.render_widget(para, render_area);

    height
}

/// Render an assistant message without border
///
/// Returns the height used.
#[allow(clippy::cast_possible_truncation)]
fn render_assistant_message(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) -> u16 {
    // Calculate actual height needed, accounting for line wrapping
    let width = area.width.max(1) as usize;
    let height: u16 = text
        .lines()
        .map(|line| wrapped_line_height(line.chars().count(), width))
        .sum::<u16>()
        .max(1)
        .min(area.height);

    // Build lines with selection highlighting and markdown parsing
    let lines: Vec<Line> = text
        .lines()
        .enumerate()
        .map(|(i, line_text)| {
            #[allow(clippy::cast_possible_truncation)]
            let line_y = area.y + i as u16;
            let is_selected =
                selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

            if is_selected {
                if !selected_text.is_empty() {
                    selected_text.push('\n');
                }
                selected_text.push_str(line_text);
                Line::from(Span::styled(
                    line_text,
                    Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
                ))
            } else {
                // Parse markdown formatting for non-selected lines
                Line::from(parse_markdown_line(line_text))
            }
        })
        .collect();

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });

    let render_area = Rect::new(area.x, area.y, area.width, height);
    frame.render_widget(para, render_area);

    height
}

/// Render a tool message with icon and tree-style output
///
/// Format:
/// ```text
/// ● Bash(cargo build 2>&1)
///   ⎿  Compiling omni-cli v0.1.0
///      Finished `dev` profile
/// ```
///
/// Returns the height used.
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn render_tool_message(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    invocation: &str,
    output: &str,
    is_error: bool,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) -> u16 {
    let icon = if is_error {
        icons::ERROR
    } else {
        tool_icon(name)
    };
    let icon_color = if is_error { ERROR_COLOR } else { SUCCESS_COLOR };

    let mut lines: Vec<Line> = Vec::new();

    // Header line: ● ToolName(invocation)
    let header = if invocation.is_empty() {
        format!("{icon} {name}")
    } else {
        format!("{icon} {name}({invocation})")
    };

    let header_y = area.y;
    let is_header_selected =
        selection.is_some_and(|(min_y, max_y)| header_y >= min_y && header_y <= max_y);

    if is_header_selected {
        if !selected_text.is_empty() {
            selected_text.push('\n');
        }
        selected_text.push_str(&header);
        lines.push(Line::from(Span::styled(
            header,
            Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
            Span::styled(name, Style::default().fg(Color::White)),
            Span::styled(
                if invocation.is_empty() {
                    String::new()
                } else {
                    format!("({invocation})")
                },
                Style::default().fg(DIMMED),
            ),
        ]));
    }

    // Output lines with continuation character
    let output_lines: Vec<&str> = output.lines().collect();
    let max_output_lines = 12;
    let show_lines = output_lines.len().min(max_output_lines);
    let truncated = output_lines.len() > max_output_lines;

    for (i, line_text) in output_lines.iter().take(show_lines).enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let line_y = area.y + 1 + i as u16;
        let is_selected =
            selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

        // First line gets the continuation char, rest get spacing
        let prefix = if i == 0 {
            format!("  {CONT_CHAR}  ")
        } else {
            "     ".to_string()
        };

        if is_selected {
            if !selected_text.is_empty() {
                selected_text.push('\n');
            }
            selected_text.push_str(line_text);
            lines.push(Line::from(Span::styled(
                format!("{prefix}{line_text}"),
                Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
            )));
        } else {
            let text_color = if is_error { ERROR_COLOR } else { DIMMED };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(DIMMED)),
                Span::styled((*line_text).to_string(), Style::default().fg(text_color)),
            ]));
        }
    }

    // Truncation indicator
    if truncated {
        let remaining = output_lines.len() - max_output_lines;
        lines.push(Line::from(Span::styled(
            format!("     ... ({remaining} more lines)"),
            Style::default().fg(DIMMED),
        )));
    }

    // Calculate wrapped height - each line may take multiple rows
    let width = area.width.max(1) as usize;
    let prefix_len = 5; // "  ⎿  " or "     "
    let effective_width = width.saturating_sub(prefix_len).max(1);

    let mut height: u16 = 1; // Header line
    for line_text in output_lines.iter().take(show_lines) {
        height += wrapped_line_height(line_text.chars().count(), effective_width);
    }
    if truncated {
        height += 1;
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    let render_area = Rect::new(area.x, area.y, area.width, height.min(area.height));
    frame.render_widget(para, render_area);

    height
}

/// Render a `DisplayMessage` to the frame
///
/// Returns the height used.
pub fn render_message(
    frame: &mut Frame,
    area: Rect,
    message: &DisplayMessage,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) -> u16 {
    match message {
        DisplayMessage::User { text, .. } => {
            render_user_message(frame, area, text, selection, selected_text)
        }
        DisplayMessage::Assistant { text } => {
            render_assistant_message(frame, area, text, selection, selected_text)
        }
        DisplayMessage::Tool {
            name,
            invocation,
            output,
            is_error,
        } => render_tool_message(
            frame,
            area,
            name,
            invocation,
            output,
            *is_error,
            selection,
            selected_text,
        ),
    }
}

/// Calculate how many rows a line of text takes when wrapped to a given width
#[inline]
#[allow(clippy::cast_possible_truncation)]
pub const fn wrapped_line_height(chars: usize, width: usize) -> u16 {
    if chars == 0 {
        1
    } else {
        chars.div_ceil(width) as u16
    }
}

/// Calculate message height without rendering
#[allow(clippy::cast_possible_truncation)]
pub fn message_height(message: &DisplayMessage, width: u16) -> u16 {
    let width = width.max(1) as usize;
    match message {
        DisplayMessage::User { text, .. } => {
            // User messages have top and bottom padding (+2)
            let content_height: u16 = text
                .lines()
                .map(|line| wrapped_line_height(line.chars().count(), width))
                .sum::<u16>()
                .max(1);
            content_height + 2
        }
        DisplayMessage::Assistant { text } => text
            .lines()
            .map(|line| wrapped_line_height(line.chars().count(), width))
            .sum::<u16>()
            .max(1),
        DisplayMessage::Tool { output, .. } => {
            let max_output_lines = 12;
            let output_line_count = output.lines().count();
            let truncated = output_line_count > max_output_lines;
            let prefix_len = 5; // "  ⎿  " or "     "
            let effective_width = width.saturating_sub(prefix_len).max(1);

            // Calculate wrapped height for each output line
            let output_height: u16 = output
                .lines()
                .take(max_output_lines)
                .map(|line| wrapped_line_height(line.chars().count(), effective_width))
                .sum();

            // 1 for header + wrapped output lines + optional truncation line
            1 + output_height + u16::from(truncated)
        }
    }
}
