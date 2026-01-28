//! Markdown parsing for TUI rendering.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

/// Code/inline code color.
const CODE_COLOR: Color = Color::Rgb(200, 160, 100);
/// Link color.
const LINK_COLOR: Color = Color::Rgb(100, 180, 220);

/// Parse a line of markdown text into styled spans.
///
/// Supports:
/// - `**bold**` or `__bold__`
/// - `*italic*` or `_italic_`
/// - `~~strikethrough~~`
/// - `` `inline code` ``
/// - `[link text](url)` - displays link text with underline
pub fn parse_markdown_line(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current_style = Style::default();
    let mut style_stack: Vec<Style> = Vec::new();

    let options = Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(text, options);

    for event in parser {
        match event {
            Event::Text(t) => {
                spans.push(Span::styled(t.into_string(), current_style));
            }
            Event::Code(code) => {
                spans.push(Span::styled(
                    code.into_string(),
                    Style::default().fg(CODE_COLOR),
                ));
            }
            Event::Start(tag) => {
                style_stack.push(current_style);
                match tag {
                    Tag::Strong => {
                        current_style = current_style.add_modifier(Modifier::BOLD);
                    }
                    Tag::Emphasis => {
                        current_style = current_style.add_modifier(Modifier::ITALIC);
                    }
                    Tag::Strikethrough => {
                        current_style = current_style.add_modifier(Modifier::CROSSED_OUT);
                    }
                    Tag::Link { .. } => {
                        current_style = current_style
                            .fg(LINK_COLOR)
                            .add_modifier(Modifier::UNDERLINED);
                    }
                    _ => {}
                }
            }
            Event::End(tag_end) => {
                // Restore previous style
                if let Some(prev_style) = style_stack.pop() {
                    current_style = prev_style;
                }

                // Handle paragraph/line endings - don't add extra newlines for inline parsing
                if matches!(tag_end, TagEnd::Paragraph) {
                    // Paragraph ended, but we're parsing line by line so no action needed
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                spans.push(Span::raw(" "));
            }
            _ => {}
        }
    }

    // If no spans were created, return the original text as-is
    if spans.is_empty() {
        return vec![Span::raw(text.to_owned())];
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let spans = parse_markdown_line("hello world");
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn test_bold() {
        let spans = parse_markdown_line("hello **bold** world");
        assert!(spans.len() > 1);
        // The bold span should have BOLD modifier
        let bold_span = spans.iter().find(|s| s.content == "bold");
        assert!(bold_span.is_some());
        assert!(
            bold_span
                .unwrap()
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn test_italic() {
        let spans = parse_markdown_line("hello *italic* world");
        let italic_span = spans.iter().find(|s| s.content == "italic");
        assert!(italic_span.is_some());
        assert!(
            italic_span
                .unwrap()
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    #[test]
    fn test_strikethrough() {
        let spans = parse_markdown_line("hello ~~struck~~ world");
        let struck_span = spans.iter().find(|s| s.content == "struck");
        assert!(struck_span.is_some());
        assert!(
            struck_span
                .unwrap()
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
    }

    #[test]
    fn test_inline_code() {
        let spans = parse_markdown_line("run `cargo test` now");
        let code_span = spans.iter().find(|s| s.content == "cargo test");
        assert!(code_span.is_some());
        assert_eq!(code_span.unwrap().style.fg, Some(CODE_COLOR));
    }
}
