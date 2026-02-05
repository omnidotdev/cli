//! Session screen component.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::markdown::parse_markdown_line;
use super::messages::{render_message_with_scroll, wrapped_line_height};
use super::prompt::{PromptMode, render_prompt};
use super::text_layout::TextLayout;
use crate::core::agent::{AgentMode, ReasoningEffort};
use crate::tui::app::Selection;
use crate::tui::message::DisplayMessage;

/// Horizontal padding for message area.
pub const MESSAGE_PADDING_X: u16 = 2;

const DIMMED: Color = Color::Rgb(100, 100, 110);
const THINKING_PREFIX: Color = Color::Rgb(100, 160, 150);

#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
pub fn render_session(
    frame: &mut Frame,
    area: Rect,
    messages: &[DisplayMessage],
    queued_messages: &[String],
    streaming_thinking: &str,
    streaming_text: &str,
    input: &str,
    cursor: usize,
    scroll_offset: u16,
    activity_status: Option<&str>,
    model: &str,
    provider: &str,
    agent_mode: AgentMode,
    selection: Option<&Selection>,
    selected_text: &mut String,
    _session_cost: f64,
    prompt_scroll_offset: usize,
    tool_message_areas: &mut Vec<(Rect, usize)>,
    reasoning_effort: ReasoningEffort,
) -> ((u16, u16), Rect) {
    let padded_width = area.width.saturating_sub(MESSAGE_PADDING_X * 2);
    let prompt_text_width = padded_width.saturating_sub(4).max(1) as usize;
    let input_lines = if input.is_empty() {
        1
    } else {
        let layout = TextLayout::new(input, prompt_text_width);
        layout.total_lines.min(6)
    };
    let prompt_height = (input_lines as u16 + 5).clamp(6, 11);
    let queued_total_height: u16 = queued_messages
        .iter()
        .map(|text| super::messages::queued_message_height(text, padded_width) + 1)
        .sum();

    let (chunks, prompt_idx, queued_idx) = if queued_total_height > 0 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(queued_total_height),
                Constraint::Length(prompt_height),
            ])
            .split(area);
        (chunks, 3, Some(2))
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(prompt_height)])
            .split(area);
        (chunks, 1, None)
    };

    render_message_list(
        frame,
        chunks[0],
        messages,
        streaming_thinking,
        streaming_text,
        scroll_offset,
        selection,
        selected_text,
        tool_message_areas,
    );

    if let Some(idx) = queued_idx {
        render_queued_area(frame, chunks[idx], queued_messages, agent_mode);
    }

    let prompt_area = Rect::new(
        chunks[prompt_idx].x + MESSAGE_PADDING_X,
        chunks[prompt_idx].y,
        chunks[prompt_idx]
            .width
            .saturating_sub(MESSAGE_PADDING_X * 2),
        chunks[prompt_idx].height,
    );

    render_prompt(
        frame,
        prompt_area,
        input,
        cursor,
        PromptMode::FullWidth,
        activity_status,
        model,
        provider,
        None,
        agent_mode,
        prompt_scroll_offset,
        reasoning_effort,
    )
}

fn render_queued_area(
    frame: &mut Frame,
    area: Rect,
    queued_messages: &[String],
    agent_mode: AgentMode,
) {
    let padded_area = Rect::new(
        area.x + MESSAGE_PADDING_X,
        area.y,
        area.width.saturating_sub(MESSAGE_PADDING_X * 2),
        area.height,
    );

    let mut y_offset: u16 = 0;
    for queued_text in queued_messages {
        let msg_height = super::messages::queued_message_height(queued_text, padded_area.width);
        if y_offset + msg_height > padded_area.height {
            break;
        }
        let msg_area = Rect::new(
            padded_area.x,
            padded_area.y + y_offset,
            padded_area.width,
            msg_height,
        );
        super::messages::render_queued_user_message(frame, msg_area, queued_text, agent_mode, 0);
        y_offset += msg_height + 1;
    }
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn render_message_list(
    frame: &mut Frame,
    area: Rect,
    messages: &[DisplayMessage],
    streaming_thinking: &str,
    streaming_text: &str,
    scroll_offset: u16,
    selection: Option<&Selection>,
    selected_text: &mut String,
    tool_message_areas: &mut Vec<(Rect, usize)>,
) {
    let padded_area = Rect::new(
        area.x + MESSAGE_PADDING_X,
        area.y,
        area.width.saturating_sub(MESSAGE_PADDING_X * 2),
        area.height,
    );

    tool_message_areas.clear();

    if messages.is_empty() && streaming_thinking.is_empty() && streaming_text.is_empty() {
        let empty_msg = Paragraph::new(Line::from(Span::styled(
            "No messages yet. Start typing to begin.",
            Style::default().fg(DIMMED),
        )));
        frame.render_widget(empty_msg, padded_area);
        return;
    }

    let mut content_y: u16 = 1;

    for (msg_index, message) in messages.iter().enumerate() {
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

        if matches!(message, DisplayMessage::Tool { .. }) && clip_top == 0 {
            let tool_area = Rect::new(padded_area.x, screen_y, padded_area.width, 1);
            tool_message_areas.push((tool_area, msg_index));
        }

        content_y = msg_end + 1;
    }

    // Render streaming thinking if present (dimmed style)
    if !streaming_thinking.is_empty() {
        let screen_y = if content_y >= scroll_offset {
            padded_area.y + (content_y - scroll_offset)
        } else {
            padded_area.y
        };

        if screen_y < padded_area.y + padded_area.height {
            let available_height = (padded_area.y + padded_area.height).saturating_sub(screen_y);
            let thinking_area =
                Rect::new(padded_area.x, screen_y, padded_area.width, available_height);

            let thinking_line = Line::from(vec![
                Span::styled(
                    "Thinking: ",
                    Style::default()
                        .fg(THINKING_PREFIX)
                        .add_modifier(Modifier::ITALIC),
                ),
                Span::styled(streaming_thinking.to_owned(), Style::default().fg(DIMMED)),
            ]);
            let para = Paragraph::new(thinking_line).wrap(Wrap { trim: false });
            frame.render_widget(para, thinking_area);

            let prefixed = format!("Thinking: {streaming_thinking}");
            let thinking_height: u16 =
                wrapped_line_height(prefixed.chars().count(), padded_area.width.max(1) as usize)
                    .max(1);
            content_y = content_y.saturating_add(thinking_height).saturating_add(1);
        }
    }

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

#[allow(clippy::cast_possible_truncation)]
pub fn calculate_content_height(
    messages: &[DisplayMessage],
    streaming_thinking: &str,
    streaming_text: &str,
    width: u16,
) -> u16 {
    let mut total: u16 = 1;

    for message in messages {
        total = total.saturating_add(estimate_message_height(message, width));
        total = total.saturating_add(1);
    }

    if !streaming_thinking.is_empty() {
        let width_usize = width.max(1) as usize;
        let prefixed = format!("Thinking: {streaming_thinking}");
        let thinking_height: u16 =
            wrapped_line_height(prefixed.chars().count(), width_usize).max(1);
        total = total.saturating_add(thinking_height).saturating_add(1);
    }

    if !streaming_text.is_empty() {
        let width_usize = width.max(1) as usize;
        let streaming_height: u16 = streaming_text
            .lines()
            .map(|line| wrapped_line_height(line.chars().count(), width_usize))
            .sum::<u16>()
            .max(1);
        total = total.saturating_add(streaming_height).saturating_add(1);
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
            mode: AgentMode::Build,
            files: Vec::new(),
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
        // 20 chars on width 10: text_width = 10 - 2 = 8, wraps to 3 lines + 2 padding = 5
        let msg = user_message("12345678901234567890");
        let height = estimate_message_height(&msg, 10);
        assert_eq!(height, 5);
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
        let height = calculate_content_height(&[], "", "", 80);
        assert_eq!(height, 1);
    }

    #[test]
    fn calculate_content_height_single_message() {
        let messages = vec![user_message("hello")];
        let height = calculate_content_height(&messages, "", "", 80);
        assert_eq!(height, 5);
    }

    #[test]
    fn calculate_content_height_multiple_messages() {
        let messages = vec![
            user_message("first"),
            assistant_message("second"),
            user_message("third"),
        ];
        let height = calculate_content_height(&messages, "", "", 80);
        assert_eq!(height, 11);
    }

    #[test]
    fn calculate_content_height_with_streaming() {
        let messages = vec![user_message("hello")];
        let streaming = "streaming text";
        let height = calculate_content_height(&messages, "", streaming, 80);
        assert_eq!(height, 7);
    }

    #[test]
    fn calculate_content_height_streaming_multiline() {
        let streaming = "line one\nline two";
        let height = calculate_content_height(&[], "", streaming, 80);
        assert_eq!(height, 4);
    }
}
