//! Reusable prompt component.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use super::command_palette::CENTERED_MAX_WIDTH;
use crate::core::agent::AgentMode;

/// Brand colors
const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const PLAN_PURPLE: Color = Color::Rgb(160, 100, 200);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const INPUT_BG: Color = Color::Rgb(22, 24, 28);

/// Rotating placeholder prompts.
pub const PLACEHOLDERS: &[&str] = &[
    "what do you want to make?",
    "what are you building?",
    "what's on your mind?",
    "ask anything...",
    "what can I help with?",
];

/// Prompt display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMode {
    /// Centered prompt for welcome screen (max-width: 75).
    Centered,
    /// Full-width prompt for session screen.
    FullWidth,
}

/// Render the prompt input.
///
/// Returns the cursor position (x, y) and the prompt area rect.
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
pub fn render_prompt(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    mode: PromptMode,
    status_left: Option<&str>,
    status_right: Option<&str>,
    placeholder: Option<&str>,
    agent_mode: AgentMode,
) -> ((u16, u16), Rect) {
    match mode {
        PromptMode::Centered => {
            let ph = placeholder.unwrap_or("ask anything...");
            render_centered_prompt(frame, area, input, cursor, ph, agent_mode)
        }
        PromptMode::FullWidth => render_full_width_prompt(
            frame,
            area,
            input,
            cursor,
            status_left,
            status_right,
            agent_mode,
        ),
    }
}

/// Render centered prompt for welcome screen.
#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
fn render_centered_prompt(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    placeholder: &str,
    agent_mode: AgentMode,
) -> ((u16, u16), Rect) {
    // Early return for tiny areas
    if area.width < 5 || area.height < 3 {
        return ((area.x, area.y), area);
    }

    let prompt_width = CENTERED_MAX_WIDTH.min(area.width.saturating_sub(4));
    let prompt_x = area.x + (area.width.saturating_sub(prompt_width)) / 2;

    // Padding: 1 char left (after border), 1 char right.
    let padding = " ";
    // Text width = prompt_width - border(1) - left_pad(1) - right_pad(1).
    let text_width = prompt_width.saturating_sub(3).max(1) as usize;

    // Manually wrap text and build content lines.
    let wrapped: Vec<String> = if input.is_empty() {
        vec![placeholder.to_string()]
    } else {
        input
            .split('\n')
            .flat_map(|line| wrap_line(line, text_width))
            .collect()
    };

    // Calculate cursor's visual line position for scrolling.
    let before_cursor = &input[..cursor];
    let mut visual_line = 0;
    let mut cursor_col = 0;

    for (i, line) in input.split('\n').enumerate() {
        let line_byte_start = input
            .split('\n')
            .take(i)
            .map(|l| l.len() + 1)
            .sum::<usize>();
        let line_byte_end = line_byte_start + line.len();

        if cursor <= line_byte_start {
            break;
        }
        if cursor <= line_byte_end {
            let chars_before = before_cursor[line_byte_start..].chars().count();
            let wrapped_row = chars_before / text_width;
            cursor_col = chars_before % text_width;
            visual_line += wrapped_row;
            break;
        }
        let chars = line.chars().count();
        let line_wrapped = if chars == 0 {
            1
        } else {
            chars.div_ceil(text_width)
        };
        visual_line += line_wrapped;
    }

    // Max visible lines (excluding top/bottom padding).
    let max_visible_lines: usize = 10;
    let visible_lines = wrapped.len().min(max_visible_lines);
    let prompt_height = (visible_lines + 2) as u16;

    // Calculate scroll offset to keep cursor visible.
    let scroll_offset = if visual_line >= max_visible_lines {
        visual_line - max_visible_lines + 1
    } else {
        0
    };

    // Position prompt just below the content passed to us (small offset).
    let prompt_y = (area.y + 2).min(area.y + area.height.saturating_sub(1));
    let available_height = area.height.saturating_sub(prompt_y.saturating_sub(area.y));
    let clamped_height = prompt_height.min(available_height).max(1);
    let clamped_width = prompt_width.min(area.width);
    let prompt_area = Rect::new(prompt_x, prompt_y, clamped_width, clamped_height);

    // Build content with vertical padding and consistent horizontal padding.
    let mut content: Vec<Line> = vec![Line::from("")];
    // Color input text based on agent mode.
    let input_color = match agent_mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };
    let text_style = if input.is_empty() {
        Style::default().fg(DIMMED)
    } else {
        Style::default().fg(input_color)
    };

    // Only render visible lines based on scroll offset.
    for line in wrapped.iter().skip(scroll_offset).take(max_visible_lines) {
        // Pad right side to fill width.
        let right_pad_len = text_width.saturating_sub(line.chars().count());
        let right_pad = " ".repeat(right_pad_len + 1); // +1 for right padding
        content.push(Line::from(vec![
            Span::raw(padding),
            Span::styled(line.as_str(), text_style),
            Span::raw(right_pad),
        ]));
    }
    content.push(Line::from(""));

    // Style based on agent mode
    let border_color = match agent_mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(INPUT_BG));

    let para = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, prompt_area);

    // Render mode indicator at top right of input box
    let mode_y = prompt_area.y.saturating_sub(1);
    if mode_y >= area.y {
        let mode_text = match agent_mode {
            AgentMode::Build => "build mode",
            AgentMode::Plan => "plan mode",
        };
        let mode_width = mode_text.chars().count() as u16;
        let mode_x = prompt_area.x + prompt_area.width.saturating_sub(mode_width);
        let mode_span = Span::styled(mode_text, Style::default().fg(border_color));
        let mode_line = Line::from(mode_span);
        let mode_para = Paragraph::new(mode_line);
        let mode_area = Rect::new(mode_x, mode_y, mode_width, 1);
        frame.render_widget(mode_para, mode_area);
    }

    // Cursor position relative to visible area.
    let visible_cursor_line = visual_line.saturating_sub(scroll_offset);
    let clamped_cursor_line = visible_cursor_line.min(max_visible_lines.saturating_sub(1));

    // x = area.x + border(1) + left_pad(1) + cursor_col.
    let cursor_x = prompt_area.x + 2 + cursor_col.min(u16::MAX as usize) as u16;
    // y = prompt_area.y + 1 (top padding) + visible cursor line.
    let cursor_y = prompt_area.y + 1 + clamped_cursor_line as u16;

    ((cursor_x, cursor_y), prompt_area)
}

/// Wrap a single line of text to fit within the given width.
fn wrap_line(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let chars: Vec<char> = text.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + width).min(chars.len());
        lines.push(chars[start..end].iter().collect());
        start = end;
    }

    lines
}

/// Render full-width prompt for session screen.
#[allow(clippy::cast_possible_truncation)]
fn render_full_width_prompt(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    status_left: Option<&str>,
    status_right: Option<&str>,
    agent_mode: AgentMode,
) -> ((u16, u16), Rect) {
    // Early return for tiny areas.
    if area.width < 3 || area.height < 2 {
        return ((area.x, area.y), area);
    }

    // Split into input area and status line.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    // Calculate cursor line position for scrolling.
    let before_cursor = &input[..cursor];
    let cursor_line = before_cursor.matches('\n').count();
    let line_start = before_cursor.rfind('\n').map_or(0, |i| i + 1);
    let cursor_col = before_cursor[line_start..].chars().count();

    let lines: Vec<&str> = input.split('\n').collect();

    // Available height for content (minus top/bottom padding).
    let available_height = chunks[0].height.saturating_sub(2) as usize;
    let max_visible_lines = available_height.max(1);

    // Calculate scroll offset to keep cursor visible.
    let scroll_offset = if cursor_line >= max_visible_lines {
        cursor_line - max_visible_lines + 1
    } else {
        0
    };

    // Build multiline content with vertical padding.
    // Color input text based on agent mode.
    let input_color = match agent_mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };
    let mut content: Vec<Line> = vec![Line::from("")];
    if input.is_empty() {
        content.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("Type here...", Style::default().fg(DIMMED)),
        ]));
    } else {
        // Only render visible lines based on scroll offset.
        for line in lines.iter().skip(scroll_offset).take(max_visible_lines) {
            content.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(*line, Style::default().fg(input_color)),
            ]));
        }
    }
    content.push(Line::from(""));

    // Style based on agent mode
    let border_color = match agent_mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(INPUT_BG));

    let para = Paragraph::new(content).block(block);
    frame.render_widget(para, chunks[0]);

    // Render status line.
    if status_left.is_some() || status_right.is_some() {
        let left = status_left.unwrap_or("");
        let right = status_right.unwrap_or("");

        // Create spans for left and right status.
        let left_span = Span::styled(format!("  {left}"), Style::default().fg(DIMMED));
        let right_span = Span::styled(right, Style::default().fg(DIMMED));

        // Calculate padding.
        let left_width = left.chars().count() + 2;
        let right_width = right.chars().count();
        let padding_width = (chunks[1].width as usize)
            .saturating_sub(left_width)
            .saturating_sub(right_width);
        let padding = " ".repeat(padding_width);

        let status_line = Line::from(vec![left_span, Span::raw(padding), right_span]);

        let status_para = Paragraph::new(status_line);
        frame.render_widget(status_para, chunks[1]);
    }

    // Cursor position relative to visible area.
    let visible_cursor_line = cursor_line.saturating_sub(scroll_offset);
    let clamped_cursor_line = visible_cursor_line.min(max_visible_lines.saturating_sub(1));

    // x = area.x + 1 (border) + 1 (padding) + column.
    let cursor_x = chunks[0].x + 2 + cursor_col.min(u16::MAX as usize) as u16;
    // y = area.y + 1 (top padding) + visible cursor line.
    let cursor_y = chunks[0].y + 1 + clamped_cursor_line as u16;

    ((cursor_x, cursor_y), chunks[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_line_empty_returns_single_empty() {
        let result = wrap_line("", 10);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn wrap_line_shorter_than_width() {
        let result = wrap_line("hello", 10);
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn wrap_line_exact_width() {
        let result = wrap_line("hello", 5);
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn wrap_line_longer_than_width() {
        let result = wrap_line("hello world", 5);
        assert_eq!(result, vec!["hello", " worl", "d"]);
    }

    #[test]
    fn wrap_line_multiple_wraps() {
        let result = wrap_line("abcdefghij", 3);
        assert_eq!(result, vec!["abc", "def", "ghi", "j"]);
    }

    #[test]
    fn wrap_line_unicode() {
        let result = wrap_line("日本語テスト", 3);
        assert_eq!(result, vec!["日本語", "テスト"]);
    }

    #[test]
    fn prompt_mode_equality() {
        assert_eq!(PromptMode::Centered, PromptMode::Centered);
        assert_eq!(PromptMode::FullWidth, PromptMode::FullWidth);
        assert_ne!(PromptMode::Centered, PromptMode::FullWidth);
    }

    #[test]
    fn placeholders_not_empty() {
        assert!(!PLACEHOLDERS.is_empty());
        for placeholder in PLACEHOLDERS {
            assert!(!placeholder.is_empty());
        }
    }
}
