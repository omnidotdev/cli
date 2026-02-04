//! Message rendering components.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::markdown::MarkdownStreamParser;
use super::prompt::find_at_mention_spans;
use super::text_layout::TextLayout;
use crate::core::agent::AgentMode;
use crate::core::session::FileReference;
use crate::tui::message::{icons, tool_icon, DisplayMessage};

const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const PLAN_PURPLE: Color = Color::Rgb(160, 100, 200);
const QUEUED_COLOR: Color = Color::Rgb(200, 160, 80);
const PANEL_BG: Color = Color::Rgb(28, 30, 35);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const ERROR_COLOR: Color = Color::Red;
const SUCCESS_COLOR: Color = Color::Rgb(77, 201, 176);
const SELECTION_BG: Color = Color::Rgb(60, 80, 100);
const SELECTION_FG: Color = Color::White;
const THINKING_PREFIX: Color = Color::Rgb(100, 160, 150);
const FILE_REF_COLOR: Color = Color::Rgb(200, 160, 100);

pub const DIFF_ADD: Color = Color::Rgb(100, 180, 100);
pub const DIFF_DEL: Color = Color::Rgb(220, 100, 100);
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
            text, mode, files, ..
        } => {
            render_user_message_with_scroll(
                frame,
                area,
                text,
                *mode,
                files,
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

#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn render_user_message_with_scroll(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    mode: AgentMode,
    files: &[FileReference],
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
    let files_section_height = if files.is_empty() {
        0
    } else {
        (files.len() as u16) + 2
    };
    let total_height = content_height + VERTICAL_PADDING + files_section_height;
    let visible_height = total_height.saturating_sub(scroll_offset).min(area.height);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

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
            let mut line_spans = vec![Span::raw(" ")];
            line_spans.extend(parse_text_with_mentions(&wrapped_line.text));
            lines.push(Line::from(line_spans));
        }
    }

    if !files.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("Referenced files:", Style::default().fg(DIMMED)),
        ]));
        for file in files {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(&file.path, Style::default().fg(FILE_REF_COLOR)),
            ]));
        }
    }

    lines.push(Line::from(""));

    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll_offset as usize).collect();

    let border_color = match mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(PANEL_BG));

    let para = Paragraph::new(visible_lines).block(block);

    let render_area = Rect::new(area.x, area.y, area.width, visible_height);
    frame.render_widget(para, render_area);
}

fn parse_text_with_mentions(text: &str) -> Vec<Span<'static>> {
    let mentions = find_at_mention_spans(text);
    if mentions.is_empty() {
        return vec![Span::raw(text.to_string())];
    }

    let mut spans = Vec::new();
    let mut last_end = 0;

    for (start, end) in mentions {
        if start > last_end {
            spans.push(Span::raw(text[last_end..start].to_string()));
        }
        spans.push(Span::styled(
            text[start..end].to_string(),
            Style::default().fg(FILE_REF_COLOR),
        ));
        last_end = end;
    }

    if last_end < text.len() {
        spans.push(Span::raw(text[last_end..].to_string()));
    }

    spans
}

#[allow(clippy::cast_possible_truncation)]
pub fn render_queued_user_message(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    mode: AgentMode,
    scroll_offset: u16,
) {
    const LEFT_BORDER_AND_PADDING: u16 = 2;
    const RIGHT_PADDING: u16 = 1;
    const VERTICAL_PADDING: u16 = 2;
    const BADGE_HEIGHT: u16 = 1;

    let horizontal_padding = LEFT_BORDER_AND_PADDING + RIGHT_PADDING;
    let text_width = area.width.saturating_sub(horizontal_padding).max(1) as usize;
    let layout = TextLayout::new(text, text_width);
    let content_height = layout.total_lines as u16;
    let total_height = content_height + VERTICAL_PADDING + BADGE_HEIGHT;
    let visible_height = total_height.saturating_sub(scroll_offset).min(area.height);

    let mut lines: Vec<Line> = Vec::new();

    let mode_label = match mode {
        AgentMode::Build => "build",
        AgentMode::Plan => "plan",
    };
    lines.push(Line::from(vec![
        Span::styled(" ○ Queued", Style::default().fg(QUEUED_COLOR)),
        Span::styled(format!("  [{mode_label}]"), Style::default().fg(DIMMED)),
    ]));
    lines.push(Line::from(""));

    for wrapped_line in &layout.lines {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::raw(wrapped_line.text.clone()),
        ]));
    }
    lines.push(Line::from(""));

    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll_offset as usize).collect();

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(QUEUED_COLOR))
        .style(Style::default().bg(PANEL_BG));

    let para = Paragraph::new(visible_lines).block(block);

    let render_area = Rect::new(area.x, area.y, area.width, visible_height);
    frame.render_widget(para, render_area);
}

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

    let mut parser = MarkdownStreamParser::new();
    let mut all_lines: Vec<Line> = Vec::with_capacity(layout.lines.len());

    for (i, wrapped_line) in layout.lines.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let line_y = area.y + i as u16;
        let is_selected =
            selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

        let line = if is_selected {
            if !selected_text.is_empty() {
                selected_text.push('\n');
            }
            selected_text.push_str(&wrapped_line.text);
            Line::from(Span::styled(
                wrapped_line.text.clone(),
                Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
            ))
        } else {
            Line::from(parser.parse_line(&wrapped_line.text))
        };
        all_lines.push(line);
    }

    let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll_offset as usize).collect();

    let para = Paragraph::new(visible_lines);
    frame.render_widget(para, area);
}

/// Render a tool message with optional diff expansion
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
        DisplayMessage::User {
            text,
            files,
            mode: _,
            ..
        } => {
            let text_width = width.saturating_sub(3).max(1);
            let layout = TextLayout::new(text, text_width);
            let files_height = if files.is_empty() {
                0
            } else {
                (files.len() as u16) + 2
            };
            layout.total_lines as u16 + 2 + files_height
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

#[allow(clippy::cast_possible_truncation)]
pub fn queued_message_height(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let text_width = width.saturating_sub(3).max(1);
    let layout = TextLayout::new(text, text_width);
    layout.total_lines as u16 + 3
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
