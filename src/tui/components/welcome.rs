//! Welcome screen component.

use std::env;
use std::fs;
use std::path::Path;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::prompt::{PromptMode, render_prompt};

use crate::core::agent::{AgentMode, ReasoningEffort};
use crate::tui::app::{LOGO_LINES, LOGO_SHADOW};

/// Brand colors
const SHADOW_COLOR: Color = Color::Rgb(30, 80, 70);
const LOGO_COLOR: Color = Color::Rgb(77, 201, 176);
const TAGLINE_COLOR: Color = Color::Rgb(140, 140, 150);
const CWD_COLOR: Color = Color::Rgb(100, 100, 110);
const TIP_COLOR: Color = Color::Rgb(180, 160, 100);

/// Minimum width to show footer links and version.
const MIN_WIDTH_FOR_FOOTER: u16 = 60;

/// Render the welcome screen with logo, tagline, and centered prompt.
///
/// Returns the cursor position (x, y) and the prompt area rect.
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
pub fn render_welcome(
    frame: &mut Frame,
    area: Rect,
    tagline: &str,
    tip: &str,
    input: &str,
    cursor: usize,
    placeholder: &str,
    agent_mode: AgentMode,
    model: &str,
    provider: &str,
    prompt_scroll_offset: usize,
    reasoning_effort: ReasoningEffort,
) -> ((u16, u16), Rect) {
    // Early return for tiny terminals
    if area.width < 10 || area.height < 5 {
        return ((area.x, area.y), area);
    }

    // Calculate vertical centering
    let logo_height = LOGO_LINES.len() as u16;
    let total_height = logo_height + 8; // logo + tagline + cwd + model + prompt + mode indicator
    let start_y = area.y + area.height.saturating_sub(total_height) / 2;

    // Calculate horizontal centering
    let logo_width = LOGO_LINES.first().map_or(0, |l| l.chars().count()) as u16;
    let center_x = area.x + area.width.saturating_sub(logo_width) / 2;

    // Render shadow (offset right only for subtle depth)
    let shadow_style = Style::default().fg(SHADOW_COLOR);
    for (i, shadow_line) in LOGO_SHADOW.iter().enumerate() {
        let y = start_y + i as u16;
        if y < area.y + area.height {
            render_non_space_chars(
                frame.buffer_mut(),
                area,
                center_x,
                y,
                shadow_line,
                shadow_style,
            );
        }
    }

    // Render main logo (skip spaces to preserve shadow)
    let logo_style = Style::default().fg(LOGO_COLOR);
    for (i, logo_line) in LOGO_LINES.iter().enumerate() {
        let y = start_y + i as u16;
        if y < area.y + area.height {
            render_non_space_chars(frame.buffer_mut(), area, center_x, y, logo_line, logo_style);
        }
    }

    // Render tagline
    let tagline_y = start_y + logo_height + 1;
    if tagline_y < area.y + area.height.saturating_sub(1) {
        let tagline_text = format!("  {tagline}");
        let tagline_width = tagline_text.chars().count() as u16;
        let tagline_x = area.x + area.width.saturating_sub(tagline_width) / 2;
        let tagline_span = Span::styled(tagline_text, Style::default().fg(TAGLINE_COLOR));
        let para = Paragraph::new(Line::from(tagline_span));
        let clamped = clamp_rect(Rect::new(tagline_x, tagline_y, tagline_width, 1), area);
        frame.render_widget(para, clamped);
    }

    // Render CWD with git branch if available
    let cwd_y = tagline_y + 2;
    if cwd_y < area.y + area.height.saturating_sub(1) {
        let cwd = env::current_dir().map_or_else(|_| "~".to_string(), |p| abbreviate_path(&p));
        let cwd_text = match git_branch() {
            Some(branch) => format!("in {cwd} ({branch})"),
            None => format!("in {cwd}"),
        };
        let cwd_width = cwd_text.chars().count() as u16;
        let cwd_x = area.x + area.width.saturating_sub(cwd_width) / 2;
        let cwd_span = Span::styled(cwd_text, Style::default().fg(CWD_COLOR));
        let para = Paragraph::new(Line::from(cwd_span));
        let clamped = clamp_rect(Rect::new(cwd_x, cwd_y, cwd_width + 2, 1), area);
        frame.render_widget(para, clamped);
    }

    // Render model info
    let model_y = cwd_y + 1;
    if model_y < area.y + area.height.saturating_sub(1) && !model.is_empty() {
        let model_text = format!("using {model}");
        let model_width = model_text.chars().count() as u16;
        let model_x = area.x + area.width.saturating_sub(model_width) / 2;
        let model_span = Span::styled(model_text, Style::default().fg(CWD_COLOR));
        let para = Paragraph::new(Line::from(model_span));
        let clamped = clamp_rect(Rect::new(model_x, model_y, model_width + 2, 1), area);
        frame.render_widget(para, clamped);
    }

    // Render centered prompt
    let prompt_y = model_y
        .saturating_add(2)
        .min(area.y + area.height.saturating_sub(3));
    let prompt_area = Rect::new(
        area.x,
        prompt_y,
        area.width,
        area.height.saturating_sub(prompt_y.saturating_sub(area.y)),
    );

    // Render prompt first to get actual box position
    let (cursor_pos, actual_prompt_box) = render_prompt(
        frame,
        prompt_area,
        input,
        cursor,
        PromptMode::Centered,
        None,
        model,
        provider,
        Some(placeholder),
        agent_mode,
        prompt_scroll_offset,
        reasoning_effort,
    );

    // Only render footer if terminal is wide enough
    if area.width >= MIN_WIDTH_FOR_FOOTER && area.height > 4 {
        let footer_y = area.y + area.height.saturating_sub(1);

        let tip_y = actual_prompt_box.y + actual_prompt_box.height + 2;
        if !tip.is_empty() && tip_y < footer_y {
            let tip_with_dot = format!("● {tip}");
            let tip_width = tip_with_dot.chars().count() as u16;
            let tip_x = area.x + area.width.saturating_sub(tip_width) / 2;
            let tip_line = Line::from(vec![
                Span::styled("● ", Style::default().fg(TIP_COLOR)),
                Span::styled(tip, Style::default().fg(TIP_COLOR)),
            ]);
            let tip_para = Paragraph::new(tip_line);
            let clamped = clamp_rect(Rect::new(tip_x, tip_y, tip_width, 1), area);
            frame.render_widget(tip_para, clamped);
        }

        // Render socials in bottom left corner
        let links_text = "x.com/omnidotdev · discord.gg/omnidotdev · docs.omni.dev";
        let links_style = Style::default().fg(CWD_COLOR);
        render_text(
            frame.buffer_mut(),
            area,
            area.x + 1,
            footer_y,
            links_text,
            links_style,
        );

        // Render version in bottom right corner
        let version = format!("early access | {}", crate::build_info::short_version());
        let version_width = version.chars().count() as u16;
        let version_x = area.x + area.width.saturating_sub(version_width + 1);
        let version_span = Span::styled(version, Style::default().fg(CWD_COLOR));
        let version_para = Paragraph::new(Line::from(version_span));
        let clamped = clamp_rect(Rect::new(version_x, footer_y, version_width, 1), area);
        frame.render_widget(version_para, clamped);
    }

    (cursor_pos, actual_prompt_box)
}

/// Clamp a Rect to fit within bounds.
fn clamp_rect(rect: Rect, bounds: Rect) -> Rect {
    let x = rect
        .x
        .max(bounds.x)
        .min(bounds.x + bounds.width.saturating_sub(1));
    let y = rect
        .y
        .max(bounds.y)
        .min(bounds.y + bounds.height.saturating_sub(1));
    let max_w = bounds.x + bounds.width - x;
    let max_h = bounds.y + bounds.height - y;
    Rect::new(x, y, rect.width.min(max_w), rect.height.min(max_h))
}

/// Abbreviate a path by replacing home directory with ~.
fn abbreviate_path(path: &Path) -> String {
    if let Ok(home) = env::var("HOME") {
        let home_path = Path::new(&home);
        if let Ok(stripped) = path.strip_prefix(home_path) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

/// Get the current git branch name if in a git repository.
fn git_branch() -> Option<String> {
    let cwd = env::current_dir().ok()?;
    let git_head = find_git_head(&cwd)?;
    let contents = fs::read_to_string(git_head).ok()?;

    // Parse HEAD: either "ref: refs/heads/branch-name" or a detached commit hash
    let contents = contents.trim();
    if let Some(ref_path) = contents.strip_prefix("ref: refs/heads/") {
        Some(ref_path.to_string())
    } else {
        // Detached HEAD - show short commit hash
        Some(contents.chars().take(7).collect())
    }
}

/// Find .git/HEAD by walking up the directory tree.
fn find_git_head(start: &Path) -> Option<std::path::PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let git_head = current.join(".git/HEAD");
        if git_head.exists() {
            return Some(git_head);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Render non-space characters to buffer, preserving existing content at space positions.
#[allow(clippy::cast_possible_truncation)]
fn render_non_space_chars(
    buf: &mut Buffer,
    bounds: Rect,
    x: u16,
    y: u16,
    text: &str,
    style: Style,
) {
    if y < bounds.y || y >= bounds.y + bounds.height {
        return;
    }
    for (i, ch) in text.chars().enumerate() {
        if ch != ' ' {
            let cell_x = x + i as u16;
            if cell_x >= bounds.x && cell_x < bounds.x + bounds.width {
                if let Some(cell) = buf.cell_mut((cell_x, y)) {
                    cell.set_char(ch);
                    cell.set_style(style);
                }
            }
        }
    }
}

/// Render plain text to buffer with bounds checking.
#[allow(clippy::cast_possible_truncation)]
fn render_text(buf: &mut Buffer, bounds: Rect, x: u16, y: u16, text: &str, style: Style) {
    if y < bounds.y || y >= bounds.y + bounds.height {
        return;
    }
    for (i, ch) in text.chars().enumerate() {
        let cell_x = x + i as u16;
        if cell_x >= bounds.x && cell_x < bounds.x + bounds.width {
            if let Some(cell) = buf.cell_mut((cell_x, y)) {
                cell.set_char(ch);
                cell.set_style(style);
            }
        }
    }
}
