//! Markdown parsing for TUI rendering.

use super::highlighting;
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum ParserState {
    Normal,
    InCodeBlock { language: String },
}

/// Stateful markdown parser that tracks code fences across lines.
#[allow(dead_code)]
pub struct MarkdownStreamParser {
    state: ParserState,
}

#[allow(dead_code)]
impl MarkdownStreamParser {
    /// Create a new parser in the normal state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: ParserState::Normal,
        }
    }

    /// Parse a single line, tracking code fences across calls.
    #[must_use]
    pub fn parse_line(&mut self, line: &str) -> Vec<Span<'static>> {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        match &mut self.state {
            ParserState::Normal => {
                if let Some(language) = parse_fence_language(trimmed) {
                    self.state = ParserState::InCodeBlock { language };
                    return Vec::new();
                }
                parse_markdown_line(line)
            }
            ParserState::InCodeBlock { language } => {
                if trimmed == "```" {
                    self.state = ParserState::Normal;
                    return Vec::new();
                }

                let spans = highlighting::highlight_code(line, language);
                if spans.is_empty() {
                    vec![Span::raw(String::new())]
                } else {
                    spans
                }
            }
        }
    }

    /// Reset the parser to the normal state.
    pub fn reset(&mut self) {
        self.state = ParserState::Normal;
    }
}

#[allow(dead_code)]
fn parse_fence_language(line: &str) -> Option<String> {
    let rest = line.strip_prefix("```")?;
    if rest.is_empty() {
        return Some(String::new());
    }

    let language = rest.trim();
    if language.is_empty() {
        return Some(String::new());
    }

    if language
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(language.to_string())
    } else {
        None
    }
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

    #[test]
    fn test_stream_parser_state_transitions() {
        let mut parser = MarkdownStreamParser::new();

        let spans = parser.parse_line("```rust\n");
        assert!(spans.is_empty());
        assert!(matches!(
            parser.state,
            ParserState::InCodeBlock { ref language } if language == "rust"
        ));

        let spans = parser.parse_line("fn main() {}\n");
        assert!(!spans.is_empty());

        let spans = parser.parse_line("```\n");
        assert!(spans.is_empty());
        assert!(matches!(parser.state, ParserState::Normal));
    }

    #[test]
    fn test_stream_parser_empty_block() {
        let mut parser = MarkdownStreamParser::new();

        assert!(parser.parse_line("```\n").is_empty());
        assert!(matches!(parser.state, ParserState::InCodeBlock { .. }));
        assert!(parser.parse_line("```\n").is_empty());
        assert!(matches!(parser.state, ParserState::Normal));
    }

    #[test]
    fn test_stream_parser_rejects_nested_backticks() {
        let mut parser = MarkdownStreamParser::new();
        let spans = parser.parse_line("````");

        assert!(matches!(parser.state, ParserState::Normal));
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn test_stream_parser_reset() {
        let mut parser = MarkdownStreamParser::new();

        let _ = parser.parse_line("```rust");
        assert!(matches!(parser.state, ParserState::InCodeBlock { .. }));
        parser.reset();
        assert!(matches!(parser.state, ParserState::Normal));
    }
}
