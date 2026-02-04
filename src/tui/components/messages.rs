//! Message rendering components.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::markdown::parse_markdown_line;
use super::text_layout::TextLayout;
use crate::core::agent::AgentMode;
use crate::tui::message::{icons, tool_icon, DisplayMessage};

/// Brand colors
const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const PLAN_PURPLE: Color = Color::Rgb(160, 100, 200);
/// Lighter panel background for "previous" user messages
const PANEL_BG: Color = Color::Rgb(28, 30, 35);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const ERROR_COLOR: Color = Color::Red;
const SUCCESS_COLOR: Color = Color::Rgb(77, 201, 176);
const SELECTION_BG: Color = Color::Rgb(60, 80, 100);
const SELECTION_FG: Color = Color::White;
const THINKING_PREFIX: Color = Color::Rgb(100, 160, 150);

pub const DIFF_ADD: Color = Color::Rgb(80, 160, 80);
pub const DIFF_DEL: Color = Color::Rgb(180, 80, 80);
pub const DIFF_HUNK: Color = Color::Rgb(80, 140, 180);

fn format_line_badge(count: usize) -> String {
    match count {
        0 => "[no output]".to_string(),
        1 => "[1 line]".to_string(),
        n => format!("[{n} lines]"),
    }
}

pub fn line_color(line: &str) -> Color {
    if (line.starts_with('+') || line.starts_with('>')) && !line.starts_with("+++") {
        DIFF_ADD
    } else if (line.starts_with('-') || line.starts_with('<')) && !line.starts_with("---") {
        DIFF_DEL
    } else if line.starts_with("@@") || line.starts_with("diff ") {
        DIFF_HUNK
    } else {
        DIMMED
    }
}

/// Render a `DisplayMessage` with scroll offset for partial visibility
///
/// The `scroll_offset` parameter specifies how many lines to skip from the top
/// of the message content, enabling smooth line-by-line scrolling.
#[allow(clippy::too_many_arguments)]
pub fn render_message_with_scroll(
    frame: &mut Frame,
    area: Rect,
    message: &DisplayMessage,
    scroll_offset: u16,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    match message {
        DisplayMessage::User {
            text, mode, queued, ..
        } => {
            render_user_message_with_scroll(
                frame,
                area,
                text,
                *mode,
                *queued,
                scroll_offset,
                selection,
                selected_text,
            );
        }
        DisplayMessage::Assistant { text } => {
            render_assistant_message_with_scroll(
                frame,
                area,
                text,
                scroll_offset,
                selection,
                selected_text,
            );
        }
        DisplayMessage::Tool {
            name,
            invocation,
            output,
            is_error,
        } => {
            render_tool_message_with_scroll(
                frame,
                area,
                name,
                invocation,
                output,
                *is_error,
                scroll_offset,
                selection,
                selected_text,
            );
        }
        DisplayMessage::Reasoning { text } => {
            render_reasoning_message_with_scroll(
                frame,
                area,
                text,
                scroll_offset,
                selection,
                selected_text,
            );
        }
    }
}

/// Render a user message with scroll offset for partial visibility
const QUEUED_COLOR: Color = Color::Rgb(200, 160, 80);

#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn render_user_message_with_scroll(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    mode: AgentMode,
    queued: bool,
    scroll_offset: u16,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    const LEFT_BORDER_AND_PADDING: u16 = 2;
    const RIGHT_PADDING: u16 = 1;
    const VERTICAL_PADDING: u16 = 2;

    let horizontal_padding = LEFT_BORDER_AND_PADDING + RIGHT_PADDING;
    let text_width = area.width.saturating_sub(horizontal_padding).max(1) as usize;
    let layout = TextLayout::new(text, text_width);
    let content_height = layout.total_lines as u16;
    let badge_height = if queued { 1 } else { 0 };
    let total_height = content_height + VERTICAL_PADDING + badge_height;
    let visible_height = total_height.saturating_sub(scroll_offset).min(area.height);

    let mut lines: Vec<Line> = Vec::new();

    if queued {
        lines.push(Line::from(Span::styled(
            " ○ Queued",
            Style::default().fg(QUEUED_COLOR),
        )));
    }

    lines.push(Line::from("")); // Top padding

    for (i, wrapped_line) in layout.lines.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let line_y = area.y + 1 + i as u16;
        let is_selected =
            selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

        if is_selected {
            if !selected_text.is_empty() {
                selected_text.push('\n');
            }
            selected_text.push_str(&wrapped_line.text);
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    wrapped_line.text.clone(),
                    Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::raw(wrapped_line.text.clone()),
            ]));
        }
    }
    lines.push(Line::from("")); // Bottom padding

    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll_offset as usize).collect();

    let border_color = if queued {
        QUEUED_COLOR
    } else {
        match mode {
            AgentMode::Build => BRAND_TEAL,
            AgentMode::Plan => PLAN_PURPLE,
        }
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(PANEL_BG));

    let para = Paragraph::new(visible_lines).block(block);

    let render_area = Rect::new(area.x, area.y, area.width, visible_height);
    frame.render_widget(para, render_area);
}

/// Render an assistant message with scroll offset for partial visibility
#[allow(clippy::cast_possible_truncation)]
fn render_assistant_message_with_scroll(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    scroll_offset: u16,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    let text_width = area.width.max(1) as usize;
    let layout = TextLayout::new(text, text_width);

    let all_lines: Vec<Line> = layout
        .lines
        .iter()
        .enumerate()
        .map(|(i, wrapped_line)| {
            #[allow(clippy::cast_possible_truncation)]
            let line_y = area.y + i as u16;
            let is_selected =
                selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

            if is_selected {
                if !selected_text.is_empty() {
                    selected_text.push('\n');
                }
                selected_text.push_str(&wrapped_line.text);
                Line::from(Span::styled(
                    wrapped_line.text.clone(),
                    Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
                ))
            } else {
                Line::from(parse_markdown_line(&wrapped_line.text))
            }
        })
        .collect();

    let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll_offset as usize).collect();

    let para = Paragraph::new(visible_lines);
    frame.render_widget(para, area);
}

/// Render a tool message as a single collapsed line with line count badge
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn render_tool_message_with_scroll(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    invocation: &str,
    output: &str,
    is_error: bool,
    scroll_offset: u16,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    if scroll_offset > 0 {
        return;
    }

    let icon = if is_error {
        icons::ERROR
    } else {
        tool_icon(name)
    };
    let icon_color = if is_error { ERROR_COLOR } else { SUCCESS_COLOR };

    let line_count = output.lines().count();
    let badge = format_line_badge(line_count);

    let header = if invocation.is_empty() {
        format!("{icon} {name}")
    } else {
        format!("{icon} {name}({invocation})")
    };

    let header_y = area.y;
    let is_selected =
        selection.is_some_and(|(min_y, max_y)| header_y >= min_y && header_y <= max_y);

    let line = if is_selected {
        if !selected_text.is_empty() {
            selected_text.push('\n');
        }
        let full_text = format!("{header} {badge}");
        selected_text.push_str(&full_text);
        Line::from(Span::styled(
            full_text,
            Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
        ))
    } else {
        Line::from(vec![
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
            Span::styled(format!(" {badge}"), Style::default().fg(DIMMED)),
        ])
    };

    let para = Paragraph::new(vec![line]);
    frame.render_widget(para, area);
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

fn render_reasoning_message_with_scroll(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    scroll_offset: u16,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    let text_width = area.width.max(1) as usize;
    let prefixed_text = format!("Thinking: {text}");
    let layout = TextLayout::new(&prefixed_text, text_width);

    let all_lines: Vec<Line> = layout
        .lines
        .iter()
        .enumerate()
        .map(|(i, wrapped_line)| {
            #[allow(clippy::cast_possible_truncation)]
            let line_y = area.y + i as u16;
            let is_selected =
                selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

            if is_selected {
                if !selected_text.is_empty() {
                    selected_text.push('\n');
                }
                selected_text.push_str(&wrapped_line.text);
                Line::from(Span::styled(
                    wrapped_line.text.clone(),
                    Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
                ))
            } else if i == 0 && wrapped_line.text.starts_with("Thinking: ") {
                let prefix_len = "Thinking: ".len();
                let (prefix, content) = wrapped_line
                    .text
                    .split_at(prefix_len.min(wrapped_line.text.len()));
                Line::from(vec![
                    Span::styled(
                        prefix.to_string(),
                        Style::default()
                            .fg(THINKING_PREFIX)
                            .add_modifier(Modifier::ITALIC),
                    ),
                    Span::styled(content.to_string(), Style::default().fg(DIMMED)),
                ])
            } else {
                Line::from(Span::styled(
                    wrapped_line.text.clone(),
                    Style::default().fg(DIMMED),
                ))
            }
        })
        .collect();

    let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll_offset as usize).collect();

    let para = Paragraph::new(visible_lines);
    frame.render_widget(para, area);
}

#[allow(clippy::cast_possible_truncation)]
pub fn message_height(message: &DisplayMessage, width: u16) -> u16 {
    let width = width.max(1) as usize;
    match message {
        DisplayMessage::User { text, mode: _, .. } => {
            let text_width = width.saturating_sub(3).max(1);
            let layout = TextLayout::new(text, text_width);
            layout.total_lines as u16 + 2
        }
        DisplayMessage::Assistant { text } => {
            let layout = TextLayout::new(text, width);
            (layout.total_lines as u16).max(1)
        }
        DisplayMessage::Reasoning { text } => {
            let prefixed_text = format!("Thinking: {text}");
            let layout = TextLayout::new(&prefixed_text, width);
            (layout.total_lines as u16).max(1)
        }
        DisplayMessage::Tool { .. } => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_line_badge_empty() {
        assert_eq!(format_line_badge(0), "[no output]");
    }

    #[test]
    fn test_format_line_badge_singular() {
        assert_eq!(format_line_badge(1), "[1 line]");
    }

    #[test]
    fn test_format_line_badge_plural() {
        assert_eq!(format_line_badge(5), "[5 lines]");
        assert_eq!(format_line_badge(247), "[247 lines]");
    }

    #[test]
    fn test_format_line_badge_large() {
        assert_eq!(format_line_badge(1000), "[1000 lines]");
    }
}
