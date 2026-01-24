//! Session screen component.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::markdown::parse_markdown_line;
use super::messages::render_message;
use super::prompt::{PromptMode, render_prompt};
use crate::core::agent::AgentMode;
use crate::tui::app::Selection;
use crate::tui::message::DisplayMessage;

/// Horizontal padding for message area.
pub const MESSAGE_PADDING_X: u16 = 2;
/// Bottom padding for message area.
const MESSAGE_PADDING_BOTTOM: u16 = 1;

/// Brand colors.
const DIMMED: Color = Color::Rgb(100, 100, 110);

/// Render the session screen with message list and prompt.
///
/// Returns the cursor position (x, y) and the prompt area rect.
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
pub fn render_session(
    frame: &mut Frame,
    area: Rect,
    messages: &[DisplayMessage],
    streaming_text: &str,
    input: &str,
    cursor: usize,
    scroll_offset: u16,
    loading: bool,
    model: &str,
    agent_mode: AgentMode,
    selection: Option<&Selection>,
    selected_text: &mut String,
    session_cost: f64,
) -> ((u16, u16), Rect) {
    // Calculate dynamic prompt height based on input lines.
    // Height = top padding (1) + input lines + bottom padding (1) + status bar (1).
    let input_lines = input.lines().count().max(1) as u16;
    // Add 1 for empty input that ends with newline.
    let input_lines = if input.ends_with('\n') {
        input_lines + 1
    } else {
        input_lines
    };
    let prompt_height = (input_lines + 3).clamp(4, 13);

    // Split into message area and prompt area.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                // Messages
            Constraint::Length(prompt_height), // Prompt + status
        ])
        .split(area);

    // Render messages.
    render_message_list(
        frame,
        chunks[0],
        messages,
        streaming_text,
        scroll_offset,
        selection,
        selected_text,
    );

    // Apply same horizontal padding to prompt area for alignment
    let prompt_area = Rect::new(
        chunks[1].x + MESSAGE_PADDING_X,
        chunks[1].y,
        chunks[1].width.saturating_sub(MESSAGE_PADDING_X * 2),
        chunks[1].height,
    );

    // Render prompt with status
    let status_left = if loading { Some("Thinking...") } else { None };
    // Show mode, model, cost, and build version in status
    let version = crate::build_info::short_version();
    let cost_str = if session_cost > 0.0 {
        format!(" · ${session_cost:.4}")
    } else {
        String::new()
    };
    let status_right_text = match agent_mode {
        AgentMode::Build => format!("{model}{cost_str} | {version}"),
        AgentMode::Plan => format!("plan mode · {model}{cost_str} | {version}"),
    };

    render_prompt(
        frame,
        prompt_area,
        input,
        cursor,
        PromptMode::FullWidth,
        status_left,
        Some(&status_right_text),
        None,
        agent_mode,
    )
}

/// Render the scrollable message list.
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn render_message_list(
    frame: &mut Frame,
    area: Rect,
    messages: &[DisplayMessage],
    streaming_text: &str,
    scroll_offset: u16,
    selection: Option<&Selection>,
    selected_text: &mut String,
) {
    // Apply padding to message area.
    let padded_area = Rect::new(
        area.x + MESSAGE_PADDING_X,
        area.y,
        area.width.saturating_sub(MESSAGE_PADDING_X * 2),
        area.height.saturating_sub(MESSAGE_PADDING_BOTTOM),
    );

    if messages.is_empty() && streaming_text.is_empty() {
        // Render empty state.
        let empty_msg = Paragraph::new(Line::from(Span::styled(
            "No messages yet. Start typing to begin.",
            Style::default().fg(DIMMED),
        )));
        frame.render_widget(empty_msg, padded_area);
        return;
    }

    // Calculate total content height and render visible messages.
    let mut y_offset: u16 = 0;
    let mut rendered_height: u16 = 0;
    let visible_start = scroll_offset;

    for message in messages {
        // Estimate message height (simplified - could be more accurate).
        let msg_height = estimate_message_height(message, padded_area.width);

        // Skip messages above the visible area.
        if y_offset + msg_height <= visible_start {
            y_offset += msg_height + 1; // +1 for spacing
            continue;
        }

        // Stop if we've filled the visible area.
        if rendered_height >= padded_area.height {
            break;
        }

        // Calculate render position.
        let render_y = padded_area.y + rendered_height;
        let available_height = padded_area.height.saturating_sub(rendered_height);

        // Render the message.
        let msg_area = Rect::new(padded_area.x, render_y, padded_area.width, available_height);
        let sel_bounds = selection.map(Selection::bounds);
        let height = render_message(frame, msg_area, message, sel_bounds, selected_text);

        rendered_height += height + 1; // +1 for spacing
        y_offset += msg_height + 1;
    }

    // Render streaming text if present.
    if !streaming_text.is_empty() && rendered_height < padded_area.height {
        // Add padding before streaming text
        rendered_height += 1;
        let render_y = padded_area.y + rendered_height;
        let available_height = padded_area.height.saturating_sub(rendered_height);
        let streaming_area =
            Rect::new(padded_area.x, render_y, padded_area.width, available_height);

        // Check if streaming text overlaps with selection.
        let sel_bounds = selection.map(Selection::bounds);
        let is_selected = sel_bounds.is_some_and(|(min_y, max_y)| {
            render_y <= max_y && render_y + available_height >= min_y
        });

        // Build styled lines with markdown parsing.
        let lines: Vec<Line> = if is_selected {
            // Collect selected lines from streaming text.
            if let Some((min_y, max_y)) = sel_bounds {
                for (i, line) in streaming_text.lines().enumerate() {
                    #[allow(clippy::cast_possible_truncation)]
                    let line_y = render_y + i as u16;
                    if line_y >= min_y && line_y <= max_y {
                        if !selected_text.is_empty() {
                            selected_text.push('\n');
                        }
                        selected_text.push_str(line);
                    }
                }
            }
            // Selection styling overrides markdown.
            streaming_text
                .lines()
                .map(|line| {
                    Line::from(Span::styled(
                        line.to_owned(),
                        Style::default()
                            .bg(Color::Rgb(60, 80, 100))
                            .fg(Color::White),
                    ))
                })
                .collect()
        } else {
            // Parse markdown for non-selected streaming text.
            streaming_text
                .lines()
                .map(|line| Line::from(parse_markdown_line(line)))
                .collect()
        };

        let para = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(para, streaming_area);
    }
}

/// Estimate the height needed to render a message, accounting for text wrapping
#[allow(clippy::cast_possible_truncation)]
fn estimate_message_height(message: &DisplayMessage, width: u16) -> u16 {
    super::messages::message_height(message, width)
}

/// Calculate total content height for all messages and streaming text.
#[allow(clippy::cast_possible_truncation)]
pub fn calculate_content_height(
    messages: &[DisplayMessage],
    streaming_text: &str,
    width: u16,
) -> u16 {
    let mut total: u16 = 0;

    for message in messages {
        total = total.saturating_add(estimate_message_height(message, width));
        total = total.saturating_add(1); // Spacing between messages.
    }

    // Add streaming text height.
    if !streaming_text.is_empty() {
        let width = width.max(1) as usize;
        let streaming_height: u16 = streaming_text
            .lines()
            .map(|line| {
                let chars = line.chars().count();
                ((chars / width) + 1) as u16
            })
            .sum::<u16>()
            .max(1);
        total = total.saturating_add(streaming_height);
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_message(text: &str) -> DisplayMessage {
        DisplayMessage::User {
            text: text.to_string(),
            timestamp: None,
        }
    }

    fn assistant_message(text: &str) -> DisplayMessage {
        DisplayMessage::Assistant {
            text: text.to_string(),
        }
    }

    #[test]
    fn estimate_height_single_line() {
        let msg = user_message("hello");
        let height = estimate_message_height(&msg, 80);
        assert_eq!(height, 1);
    }

    #[test]
    fn estimate_height_multiline() {
        let msg = user_message("line one\nline two\nline three");
        let height = estimate_message_height(&msg, 80);
        assert_eq!(height, 3);
    }

    #[test]
    fn estimate_height_wrapping() {
        // 20 chars on width 10 should wrap to 3 lines (ceil(20/10) + 1 per line calc).
        let msg = user_message("12345678901234567890");
        let height = estimate_message_height(&msg, 10);
        assert_eq!(height, 3);
    }

    #[test]
    fn estimate_height_assistant() {
        let msg = assistant_message("response text");
        let height = estimate_message_height(&msg, 80);
        assert_eq!(height, 1);
    }

    #[test]
    fn calculate_content_height_empty() {
        let height = calculate_content_height(&[], "", 80);
        assert_eq!(height, 0);
    }

    #[test]
    fn calculate_content_height_single_message() {
        let messages = vec![user_message("hello")];
        let height = calculate_content_height(&messages, "", 80);
        // 1 for message + 1 for spacing.
        assert_eq!(height, 2);
    }

    #[test]
    fn calculate_content_height_multiple_messages() {
        let messages = vec![
            user_message("first"),
            assistant_message("second"),
            user_message("third"),
        ];
        let height = calculate_content_height(&messages, "", 80);
        // (1 + 1) + (1 + 1) + (1 + 1) = 6.
        assert_eq!(height, 6);
    }

    #[test]
    fn calculate_content_height_with_streaming() {
        let messages = vec![user_message("hello")];
        let streaming = "streaming text";
        let height = calculate_content_height(&messages, streaming, 80);
        // 2 for message + 1 for streaming.
        assert_eq!(height, 3);
    }

    #[test]
    fn calculate_content_height_streaming_multiline() {
        let streaming = "line one\nline two";
        let height = calculate_content_height(&[], streaming, 80);
        assert_eq!(height, 2);
    }

    #[test]
    fn message_padding_constants() {
        assert!(MESSAGE_PADDING_X > 0);
    }
}
