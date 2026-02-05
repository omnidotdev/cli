//! Message rendering components.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::diff::{parse_diff, render_diff};
use super::markdown::MarkdownStreamParser;
use super::prompt::find_at_mention_spans;
use super::text_layout::TextLayout;
use crate::core::agent::AgentMode;
use crate::core::session::FileReference;
use crate::tui::message::{classify_tool, icons, tool_icon, DisplayMessage, ToolRenderStyle};

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
const DIFF_PANEL_BG: Color = Color::Rgb(24, 26, 32);
const DIFF_BORDER: Color = Color::Rgb(70, 100, 130);
const MAX_TOOL_OUTPUT_LINES: usize = 12;

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

fn extract_file_path_from_invocation(invocation: &str) -> &str {
    if let Some(start) = invocation.find('"') {
        if let Some(end) = invocation[start + 1..].find('"') {
            return &invocation[start + 1..start + 1 + end];
        }
    }
    invocation.split_whitespace().next().unwrap_or(invocation)
}

fn extract_diff_portion(output: &str) -> &str {
    if let Some(pos) = output.find("---") {
        return &output[pos..];
    }
    output
}

fn is_diff_file_header(line: &Line<'_>) -> bool {
    let Some(first_span) = line.spans.first() else {
        return false;
    };
    if !first_span.content.starts_with(' ') && !first_span.content.starts_with('\u{2190}') {
        return false;
    }
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let trimmed = text.trim();
    if !trimmed.contains('/') {
        return false;
    }
    let path = std::path::Path::new(trimmed);
    path.extension().is_some_and(|ext| {
        matches!(
            ext.to_str(),
            Some(
                "rs" | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "json"
                    | "toml"
                    | "md"
                    | "txt"
                    | "yaml"
                    | "yml"
                    | "html"
                    | "css"
                    | "py"
                    | "go"
                    | "java"
                    | "c"
                    | "cpp"
                    | "h"
            )
        )
    })
}

/// Extract stats from tool output for minimal display
fn extract_tool_stats(name: &str, output: &str) -> String {
    match name {
        "read_file" | "Read" => {
            let line_count = output.lines().count();
            format!("→ {line_count} lines")
        }
        "list_dir" => {
            let item_count = output.lines().filter(|l| !l.is_empty()).count();
            format!("→ {item_count} items")
        }
        "memory_add" => "→ Saved".to_string(),
        "memory_delete" => "→ Deleted".to_string(),
        "plan_enter" => "→ Plan mode".to_string(),
        "plan_exit" => "→ Build mode".to_string(),
        _ => String::new(),
    }
}

fn extract_summary_stats(name: &str, output: &str) -> (String, usize) {
    let line_count = output.lines().filter(|l| !l.is_empty()).count();
    match name {
        "glob" | "Glob" => {
            let count = if line_count == 0 { 0 } else { line_count };
            (format!("→ {count} files"), line_count)
        }
        "grep" | "Grep" => {
            let files: std::collections::HashSet<&str> =
                output.lines().filter_map(|l| l.split(':').next()).collect();
            let file_count = files.len();
            (
                format!("→ {line_count} matches in {file_count} files"),
                line_count,
            )
        }
        "memory_search" => (format!("→ {line_count} memories"), line_count),
        "lsp" => {
            let preview = output
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(40)
                .collect::<String>();
            let preview = if preview.len() < output.lines().next().unwrap_or("").len() {
                format!("{preview}...")
            } else {
                preview
            };
            (format!("→ {preview}"), line_count)
        }
        "skill" => ("→ Loaded".to_string(), line_count),
        _ => (String::new(), line_count),
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
    expanded_tool_messages: &std::collections::HashSet<usize>,
    message_index: usize,
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
            let is_expanded = expanded_tool_messages.contains(&message_index);
            render_tool_message_with_scroll(
                frame,
                area,
                name,
                invocation,
                output,
                *is_error,
                scroll_offset,
                is_expanded,
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

/// Render a tool message with multi-line output display
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn render_tool_message_with_scroll(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    invocation: &str,
    output: &str,
    is_error: bool,
    scroll_offset: u16,
    is_expanded: bool,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    let icon = if is_error {
        icons::ERROR
    } else {
        tool_icon(name)
    };
    let icon_color = if is_error { ERROR_COLOR } else { SUCCESS_COLOR };

    // Classify tool for render style dispatch
    // Error states always use CommandOutput for full visibility
    let render_style = if is_error {
        ToolRenderStyle::CommandOutput
    } else {
        classify_tool(name)
    };

    let mut all_lines: Vec<Line> = Vec::new();

    let header_y = area.y;
    let header_selected =
        selection.is_some_and(|(min_y, max_y)| header_y >= min_y && header_y <= max_y);

    if header_selected {
        if !selected_text.is_empty() {
            selected_text.push('\n');
        }
        let header_text = if invocation.is_empty() {
            format!("{icon} {name}")
        } else {
            format!("{icon} {name}({invocation})")
        };
        selected_text.push_str(&header_text);
        all_lines.push(Line::from(Span::styled(
            header_text,
            Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
        )));
    } else {
        all_lines.push(Line::from(vec![
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

    match render_style {
        ToolRenderStyle::DiffPanel => {
            let file_path = extract_file_path_from_invocation(invocation);
            let diff_portion = extract_diff_portion(output);

            let action = if name == "write_file" {
                "Write"
            } else {
                "Edit"
            };
            let panel_width = area.width.saturating_sub(3) as usize;
            let make_panel_line = |content_spans: Vec<Span<'static>>, content_char_len: usize| {
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(
                        "\u{258e}",
                        Style::default().fg(DIFF_BORDER).bg(DIFF_PANEL_BG),
                    ),
                ];
                spans.extend(content_spans);
                let padding_needed = panel_width.saturating_sub(content_char_len);
                if padding_needed > 0 {
                    spans.push(Span::styled(
                        " ".repeat(padding_needed),
                        Style::default().bg(DIFF_PANEL_BG),
                    ));
                }
                Line::from(spans)
            };

            all_lines.push(make_panel_line(vec![], 0));

            let header_text = format!(" \u{2190} {action} {file_path}");
            let header_len = header_text.chars().count();
            all_lines.push(make_panel_line(
                vec![Span::styled(
                    header_text,
                    Style::default()
                        .fg(Color::Rgb(200, 200, 220))
                        .bg(DIFF_PANEL_BG),
                )],
                header_len,
            ));

            all_lines.push(make_panel_line(vec![], 0));

            let parsed = parse_diff(diff_portion);
            let diff_lines = render_diff(&parsed, panel_width as u16);

            for diff_line in diff_lines {
                if is_diff_file_header(&diff_line) {
                    continue;
                }

                let mut content_spans: Vec<Span<'static>> = Vec::new();
                let mut content_len = 0usize;
                for span in diff_line.spans {
                    content_len += span.content.chars().count();
                    let bg = span.style.bg.unwrap_or(DIFF_PANEL_BG);
                    content_spans.push(Span::styled(span.content, span.style.bg(bg)));
                }
                all_lines.push(make_panel_line(content_spans, content_len));
            }

            all_lines.push(make_panel_line(vec![], 0));
        }
        ToolRenderStyle::Minimal => {
            render_minimal_tool(
                frame,
                area,
                name,
                invocation,
                output,
                scroll_offset,
                selection,
                selected_text,
            );
            return;
        }
        ToolRenderStyle::SummaryExpandable => {
            render_summary_expandable_tool(
                frame,
                area,
                name,
                invocation,
                output,
                scroll_offset,
                is_expanded,
                selection,
                selected_text,
            );
            return;
        }
        ToolRenderStyle::Structured
        | ToolRenderStyle::Interactive
        | ToolRenderStyle::CommandOutput => {
            let output_lines: Vec<&str> = output.lines().collect();
            let total_lines = output_lines.len();
            let show_expand_indicator = total_lines > MAX_TOOL_OUTPUT_LINES && !is_expanded;
            let show_collapse_indicator = is_expanded && total_lines > MAX_TOOL_OUTPUT_LINES;

            let lines_to_show = if is_expanded {
                total_lines
            } else {
                total_lines.min(MAX_TOOL_OUTPUT_LINES)
            };

            for (i, line_text) in output_lines.iter().take(lines_to_show).enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                let line_y = area.y + 1 + i as u16;
                let is_selected =
                    selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

                if is_selected {
                    if !selected_text.is_empty() {
                        selected_text.push('\n');
                    }
                    selected_text.push_str(line_text);
                    all_lines.push(Line::from(Span::styled(
                        format!("  {line_text}"),
                        Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
                    )));
                } else {
                    let color = line_color(line_text);
                    all_lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled((*line_text).to_string(), Style::default().fg(color)),
                    ]));
                }
            }

            if show_expand_indicator {
                let remaining = total_lines - MAX_TOOL_OUTPUT_LINES;
                all_lines.push(Line::from(Span::styled(
                    format!("  \u{25B6} [+{remaining} more lines]"),
                    Style::default().fg(DIMMED),
                )));
            } else if show_collapse_indicator {
                all_lines.push(Line::from(Span::styled(
                    "  \u{25BC} [collapse]".to_string(),
                    Style::default().fg(DIMMED),
                )));
            }
        }
    }

    let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll_offset as usize).collect();

    let para = Paragraph::new(visible_lines);
    frame.render_widget(para, area);
}

/// Render a tool message as a single-line summary (no expansion)
#[allow(clippy::too_many_arguments)]
fn render_minimal_tool(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    invocation: &str,
    output: &str,
    scroll_offset: u16,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    let icon = tool_icon(name);
    let stats = extract_tool_stats(name, output);

    let header_y = area.y;
    let header_selected =
        selection.is_some_and(|(min_y, max_y)| header_y >= min_y && header_y <= max_y);

    let line = if header_selected {
        let header_text = if invocation.is_empty() {
            format!("{icon} {name} {stats}")
        } else {
            format!("{icon} {name}({invocation}) {stats}")
        };
        if !selected_text.is_empty() {
            selected_text.push('\n');
        }
        selected_text.push_str(&header_text);
        Line::from(Span::styled(
            header_text,
            Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
        ))
    } else {
        Line::from(vec![
            Span::styled(format!("{icon} "), Style::default().fg(SUCCESS_COLOR)),
            Span::styled(name, Style::default().fg(Color::White)),
            Span::styled(
                if invocation.is_empty() {
                    String::new()
                } else {
                    format!("({invocation})")
                },
                Style::default().fg(DIMMED),
            ),
            Span::styled(format!(" {stats}"), Style::default().fg(DIMMED)),
        ])
    };

    if scroll_offset == 0 {
        let para = Paragraph::new(vec![line]);
        frame.render_widget(para, Rect::new(area.x, area.y, area.width, 1));
    }
}

/// Render a tool with summary header and expandable details
#[allow(clippy::too_many_arguments)]
fn render_summary_expandable_tool(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    invocation: &str,
    output: &str,
    scroll_offset: u16,
    is_expanded: bool,
    selection: Option<(u16, u16)>,
    selected_text: &mut String,
) {
    let icon = tool_icon(name);
    let (summary, total_lines) = extract_summary_stats(name, output);

    let mut all_lines: Vec<Line> = Vec::new();

    let header_y = area.y;
    let header_selected =
        selection.is_some_and(|(min_y, max_y)| header_y >= min_y && header_y <= max_y);

    if header_selected {
        let header_text = if invocation.is_empty() {
            format!("{icon} {name} {summary}")
        } else {
            format!("{icon} {name}({invocation}) {summary}")
        };
        if !selected_text.is_empty() {
            selected_text.push('\n');
        }
        selected_text.push_str(&header_text);
        all_lines.push(Line::from(Span::styled(
            header_text,
            Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
        )));
    } else {
        all_lines.push(Line::from(vec![
            Span::styled(format!("{icon} "), Style::default().fg(SUCCESS_COLOR)),
            Span::styled(name, Style::default().fg(Color::White)),
            Span::styled(
                if invocation.is_empty() {
                    String::new()
                } else {
                    format!("({invocation})")
                },
                Style::default().fg(DIMMED),
            ),
            Span::styled(format!(" {summary}"), Style::default().fg(DIMMED)),
        ]));
    }

    let show_details = is_expanded || total_lines <= 3;
    let show_expand_indicator = !is_expanded && total_lines > 3;
    let show_collapse_indicator = is_expanded && total_lines > 3;

    if show_details && total_lines > 0 {
        for (i, line_text) in output.lines().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let line_y = area.y + 1 + i as u16;
            let is_selected =
                selection.is_some_and(|(min_y, max_y)| line_y >= min_y && line_y <= max_y);

            if is_selected {
                if !selected_text.is_empty() {
                    selected_text.push('\n');
                }
                selected_text.push_str(line_text);
                all_lines.push(Line::from(Span::styled(
                    format!("  {line_text}"),
                    Style::default().bg(SELECTION_BG).fg(SELECTION_FG),
                )));
            } else {
                all_lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(line_text.to_string(), Style::default().fg(DIMMED)),
                ]));
            }
        }
    }

    if show_expand_indicator {
        all_lines.push(Line::from(Span::styled(
            format!("  \u{25B6} [+{total_lines} more lines]"),
            Style::default().fg(DIMMED),
        )));
    } else if show_collapse_indicator {
        all_lines.push(Line::from(Span::styled(
            "  \u{25BC} [collapse]".to_string(),
            Style::default().fg(DIMMED),
        )));
    }

    let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll_offset as usize).collect();
    let para = Paragraph::new(visible_lines);
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
pub fn message_height(message: &DisplayMessage, width: u16, is_expanded: bool) -> u16 {
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
        DisplayMessage::Tool {
            name,
            output,
            is_error,
            ..
        } => {
            // Classify tool for render style dispatch
            // Error states always use CommandOutput for full visibility
            let render_style = if *is_error {
                ToolRenderStyle::CommandOutput
            } else {
                classify_tool(name)
            };

            match render_style {
                ToolRenderStyle::Minimal => 1,
                ToolRenderStyle::DiffPanel => {
                    let diff_portion = extract_diff_portion(output);
                    let parsed = parse_diff(diff_portion);
                    let diff_lines = render_diff(&parsed, width as u16);
                    let diff_line_count = diff_lines
                        .iter()
                        .filter(|l| !is_diff_file_header(l))
                        .count();
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        1 + 4 + diff_line_count as u16
                    }
                }
                ToolRenderStyle::SummaryExpandable => {
                    let (_, total_lines) = extract_summary_stats(name, output);
                    if is_expanded || total_lines <= 3 {
                        let indicator = u16::from(is_expanded && total_lines > 3);
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            1 + total_lines as u16 + indicator
                        }
                    } else {
                        2
                    }
                }
                _ => {
                    let line_count = output.lines().count();
                    if is_expanded || line_count <= MAX_TOOL_OUTPUT_LINES {
                        let indicator =
                            u16::from(is_expanded && line_count > MAX_TOOL_OUTPUT_LINES);
                        1 + line_count as u16 + indicator
                    } else {
                        1 + MAX_TOOL_OUTPUT_LINES as u16 + 1
                    }
                }
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
pub fn queued_message_height(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let text_width = width.saturating_sub(3).max(1);
    let layout = TextLayout::new(text, text_width);
    layout.total_lines as u16 + 3
}
