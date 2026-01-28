//! Session screen component.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::markdown::parse_markdown_line;
use super::messages::{render_message_with_scroll, wrapped_line_height};
use super::prompt::{PromptMode, render_prompt};
use crate::core::agent::AgentMode;
use crate::tui::app::Selection;
use crate::tui::message::DisplayMessage;

/// Horizontal padding for message area.
pub const MESSAGE_PADDING_X: u16 = 2;

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
    activity_status: Option<&str>,
    model: &str,
    agent_mode: AgentMode,
    selection: Option<&Selection>,
    selected_text: &mut String,
    session_cost: f64,
) -> ((u16, u16), Rect) {
    // Calculate dynamic prompt height based on input lines
    // Height = top padding (1) + input lines + bottom padding (1) + status bar (1)
    let input_lines = input.lines().count().max(1) as u16;
    // Add 1 for empty input that ends with newline
    let input_lines = if input.ends_with('\n') {
        input_lines + 1
    } else {
        input_lines
    };
    let prompt_height = (input_lines + 3).clamp(4, 13);

    // Split into message area and prompt area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                // Messages
            Constraint::Length(prompt_height), // Prompt + status
        ])
        .split(area);

    // Render messages
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
    let status_left = activity_status;
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
    // Apply padding to message area
    let padded_area = Rect::new(
        area.x + MESSAGE_PADDING_X,
        area.y,
        area.width.saturating_sub(MESSAGE_PADDING_X * 2),
        area.height,
    );

    if messages.is_empty() && streaming_text.is_empty() {
        // Render empty state
        let empty_msg = Paragraph::new(Line::from(Span::styled(
            "No messages yet. Start typing to begin.",
            Style::default().fg(DIMMED),
        )));
        frame.render_widget(empty_msg, padded_area);
        return;
    }

    // Calculate content positions and render visible messages with smooth scrolling
    // y_offset tracks position in virtual content space
    // screen_y tracks where we're rendering on screen
    let mut content_y: u16 = 0;

    for message in messages {
        let msg_height = estimate_message_height(message, padded_area.width);
        let msg_end = content_y + msg_height;

        // Skip messages entirely above the visible area
        if msg_end <= scroll_offset {
            content_y = msg_end + 1; // +1 for spacing
            continue;
        }

        // Calculate how much of this message is visible
        let clip_top = scroll_offset.saturating_sub(content_y);
        let screen_y = if content_y >= scroll_offset {
            // Message starts below scroll offset
            padded_area.y + (content_y - scroll_offset)
        } else {
            // Message starts above scroll offset (partially visible)
            padded_area.y
        };

        // Stop if we've gone past the visible area
        if screen_y >= padded_area.y + padded_area.height {
            break;
        }

        let available_height = (padded_area.y + padded_area.height).saturating_sub(screen_y);

        // Render the message with scroll offset for partial visibility
        let msg_area = Rect::new(padded_area.x, screen_y, padded_area.width, available_height);
        let sel_bounds = selection.map(Selection::bounds);
        render_message_with_scroll(
            frame,
            msg_area,
            message,
            clip_top,
            sel_bounds,
            selected_text,
        );

        content_y = msg_end + 1; // +1 for spacing
    }

    // Render streaming text if present
    if !streaming_text.is_empty() {
        // Calculate position in content space
        let screen_y = if content_y >= scroll_offset {
            padded_area.y + (content_y - scroll_offset)
        } else {
            padded_area.y
        };

        // Only render if visible
        if screen_y < padded_area.y + padded_area.height {
            let clip_top = scroll_offset.saturating_sub(content_y);
            let available_height = (padded_area.y + padded_area.height).saturating_sub(screen_y);
            let streaming_area =
                Rect::new(padded_area.x, screen_y, padded_area.width, available_height);

            // Check if streaming text overlaps with selection
            let sel_bounds = selection.map(Selection::bounds);
            let is_selected = sel_bounds.is_some_and(|(min_y, max_y)| {
                screen_y <= max_y && screen_y + available_height >= min_y
            });

            // Build styled lines with markdown parsing, skipping clipped lines
            let all_lines: Vec<Line> = if is_selected {
                // Collect selected lines from streaming text
                if let Some((min_y, max_y)) = sel_bounds {
                    for (i, line) in streaming_text.lines().enumerate() {
                        #[allow(clippy::cast_possible_truncation)]
                        let line_y = screen_y + i as u16;
                        if line_y >= min_y && line_y <= max_y {
                            if !selected_text.is_empty() {
                                selected_text.push('\n');
                            }
                            selected_text.push_str(line);
                        }
                    }
                }
                // Selection styling overrides markdown
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
                // Parse markdown for non-selected streaming text
                streaming_text
                    .lines()
                    .map(|line| Line::from(parse_markdown_line(line)))
                    .collect()
            };

            // Skip clipped lines at the top
            let visible_lines: Vec<Line> = all_lines.into_iter().skip(clip_top as usize).collect();

            let para = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
            frame.render_widget(para, streaming_area);
        }
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

    // Add streaming text height
    if !streaming_text.is_empty() {
        let width = width.max(1) as usize;
        let streaming_height: u16 = streaming_text
            .lines()
            .map(|line| wrapped_line_height(line.chars().count(), width))
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
        // 1 line + 2 padding = 3
        assert_eq!(height, 3);
    }

    #[test]
    fn estimate_height_multiline() {
        let msg = user_message("line one\nline two\nline three");
        let height = estimate_message_height(&msg, 80);
        // 3 lines + 2 padding = 5
        assert_eq!(height, 5);
    }

    #[test]
    fn estimate_height_wrapping() {
        // 20 chars on width 10 should wrap to 2 lines (ceil(20/10) = 2) + 2 padding = 4
        let msg = user_message("12345678901234567890");
        let height = estimate_message_height(&msg, 10);
        assert_eq!(height, 4);
    }

    #[test]
    fn estimate_height_assistant() {
        // Assistant messages have no padding
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
        // 3 (user message with padding) + 1 (spacing) = 4
        assert_eq!(height, 4);
    }

    #[test]
    fn calculate_content_height_multiple_messages() {
        let messages = vec![
            user_message("first"),
            assistant_message("second"),
            user_message("third"),
        ];
        let height = calculate_content_height(&messages, "", 80);
        // (3 + 1) + (1 + 1) + (3 + 1) = 10
        assert_eq!(height, 10);
    }

    #[test]
    fn calculate_content_height_with_streaming() {
        let messages = vec![user_message("hello")];
        let streaming = "streaming text";
        let height = calculate_content_height(&messages, streaming, 80);
        // 4 (user message with padding + spacing) + 1 (streaming) = 5
        assert_eq!(height, 5);
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
