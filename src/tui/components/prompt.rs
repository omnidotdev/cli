//! Reusable prompt component.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};

use super::command_palette::CENTERED_MAX_WIDTH;
use super::text_layout::TextLayout;
use crate::core::agent::AgentMode;

const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const PLAN_PURPLE: Color = Color::Rgb(160, 100, 200);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const INPUT_BG: Color = Color::Rgb(22, 24, 28);

fn format_model_name(model: &str) -> String {
    let name = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .replace("claude-", "Claude ")
        .replace("gpt-", "GPT-")
        .replace("gemini-", "Gemini ")
        .replace("llama-", "Llama ")
        .replace("mistral-", "Mistral ")
        .replace("deepseek-", "DeepSeek ")
        .replace('-', " ");

    name.split_whitespace()
        .map(|word| {
            if word.chars().all(|c| c.is_ascii_digit() || c == '.')
                || word.chars().all(|c| c.is_uppercase() || c.is_ascii_digit())
            {
                word.to_string()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_provider_name(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "openai" => "OpenAI".to_string(),
        "anthropic" => "Anthropic".to_string(),
        "openrouter" => "OpenRouter".to_string(),
        "google" => "Google".to_string(),
        "azure" => "Azure".to_string(),
        "aws" | "bedrock" => "AWS Bedrock".to_string(),
        "mistral" => "Mistral".to_string(),
        "groq" => "Groq".to_string(),
        "together" => "Together".to_string(),
        "fireworks" => "Fireworks".to_string(),
        "deepseek" => "DeepSeek".to_string(),
        "ollama" => "Ollama".to_string(),
        "local" => "Local".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

pub const PLACEHOLDERS: &[&str] = &[
    "what do you want to make?",
    "what are you building?",
    "what's on your mind?",
    "ask anything...",
    "what can I help with?",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMode {
    Centered,
    FullWidth,
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
pub fn render_prompt(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    mode: PromptMode,
    status_left: Option<&str>,
    model: &str,
    provider: &str,
    placeholder: Option<&str>,
    agent_mode: AgentMode,
    scroll_offset: usize,
) -> ((u16, u16), Rect) {
    if area.width < 5 || area.height < 3 {
        return ((area.x, area.y), area);
    }

    let placeholder = placeholder.unwrap_or("ask anything...");

    let (box_area, hints_area, text_width) = match mode {
        PromptMode::Centered => {
            let prompt_width = CENTERED_MAX_WIDTH.min(area.width.saturating_sub(4));
            let prompt_x = area.x + (area.width.saturating_sub(prompt_width)) / 2;
            let prompt_y = (area.y + 2).min(area.y + area.height.saturating_sub(1));
            let text_width = prompt_width.saturating_sub(3).max(1) as usize;
            let input_lines = if input.is_empty() {
                1
            } else {
                let layout = TextLayout::new(input, text_width);
                layout.total_lines.min(6)
            };
            let box_height = (input_lines as u16 + 4).clamp(5, 10);
            let box_area = Rect::new(prompt_x, prompt_y, prompt_width, box_height);
            let hints_area = Rect::new(prompt_x, prompt_y + box_height, prompt_width, 1);
            (box_area, hints_area, text_width)
        }
        PromptMode::FullWidth => {
            let estimated_width = area.width.saturating_sub(3).max(1) as usize;
            let input_lines = if input.is_empty() {
                1
            } else {
                let layout = TextLayout::new(input, estimated_width);
                layout.total_lines.min(6)
            };
            let box_height = (input_lines as u16 + 4).clamp(5, 10);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(box_height), Constraint::Length(1)])
                .split(area);
            let text_width = chunks[0].width.saturating_sub(3).max(1) as usize;
            (chunks[0], chunks[1], text_width)
        }
    };

    let border_color = match agent_mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };

    let (content, cursor_info) = build_prompt_content(
        input,
        cursor,
        placeholder,
        agent_mode,
        model,
        provider,
        text_width,
        box_area.height,
        scroll_offset,
    );

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(INPUT_BG));

    let para = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, box_area);

    render_hints(frame, hints_area, status_left);

    let cursor_x = box_area.x + 2 + cursor_info.col.min(u16::MAX as usize) as u16;
    let cursor_y = box_area.y + 1 + cursor_info.visible_line as u16;

    ((cursor_x, cursor_y), box_area)
}

struct CursorInfo {
    col: usize,
    visible_line: usize,
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn build_prompt_content(
    input: &str,
    cursor: usize,
    placeholder: &str,
    agent_mode: AgentMode,
    model: &str,
    provider: &str,
    text_width: usize,
    box_height: u16,
    scroll_offset: usize,
) -> (Vec<Line<'static>>, CursorInfo) {
    let border_color = match agent_mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };

    let available_lines = box_height.saturating_sub(4) as usize;
    let max_visible_lines = available_lines.max(1);

    let (wrapped, visual_line, cursor_col) = if input.is_empty() {
        (vec![placeholder.to_string()], 0, 0)
    } else {
        let layout = TextLayout::new(input, text_width);
        let wrapped: Vec<String> = layout.lines.iter().map(|l| l.text.clone()).collect();
        let (visual_line, cursor_col) = layout.cursor_to_visual(cursor);

        (wrapped, visual_line, cursor_col)
    };

    let mut scroll_offset = scroll_offset;

    // Auto-scroll to keep cursor visible
    if visual_line < scroll_offset {
        scroll_offset = visual_line;
    } else if visual_line >= scroll_offset + max_visible_lines {
        scroll_offset = visual_line - max_visible_lines + 1;
    }

    let text_style = if input.is_empty() {
        Style::default().fg(DIMMED)
    } else {
        Style::default().fg(Color::White)
    };

    let mut content: Vec<Line<'static>> = vec![Line::from("")];

    for line in wrapped.iter().skip(scroll_offset).take(max_visible_lines) {
        content.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(line.clone(), text_style),
        ]));
    }

    content.push(Line::from(""));

    let mode_str = match agent_mode {
        AgentMode::Build => "Build",
        AgentMode::Plan => "Plan",
    };
    let display_model = format_model_name(model);
    let display_provider = format_provider_name(provider);

    let footer = Line::from(vec![
        Span::raw(" "),
        Span::styled(mode_str.to_string(), Style::default().fg(border_color)),
        Span::raw("  "),
        Span::styled(display_model, Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(display_provider, Style::default().fg(DIMMED)),
    ]);
    content.push(footer);
    content.push(Line::from(""));

    let visible_cursor_line = visual_line.saturating_sub(scroll_offset);
    let clamped_cursor_line = visible_cursor_line.min(max_visible_lines.saturating_sub(1));

    (
        content,
        CursorInfo {
            col: cursor_col,
            visible_line: clamped_cursor_line,
        },
    )
}

fn render_hints(frame: &mut Frame, area: Rect, status_left: Option<&str>) {
    let hints_line = if let Some(activity) = status_left {
        Line::from(Span::styled(
            format!("  {activity}"),
            Style::default().fg(DIMMED),
        ))
    } else {
        Line::from(vec![
            Span::styled("tab", Style::default().fg(Color::White)),
            Span::styled(" mode  ", Style::default().fg(DIMMED)),
            Span::styled("/", Style::default().fg(Color::White)),
            Span::styled(" commands", Style::default().fg(DIMMED)),
        ])
    };

    let hints_para = Paragraph::new(hints_line).alignment(Alignment::Right);
    frame.render_widget(hints_para, area);
}

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
