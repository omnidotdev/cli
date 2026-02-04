//! Syntax highlighting for code blocks using syntect.
//!
//! This module provides syntax highlighting functionality using the syntect library.
//! It lazily loads syntax definitions and themes using `OnceLock` for efficient resource management.

use std::sync::OnceLock;

use ratatui::{
    style::{Color, Modifier, Style as RatatuiStyle},
    text::Span,
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Style as SyntectStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

/// Default theme for syntax highlighting.
#[allow(dead_code)]
const DEFAULT_THEME: &str = "base16-ocean.dark";

/// Static storage for the syntax set (loaded once).
#[allow(dead_code)]
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// Static storage for the theme set (loaded once).
#[allow(dead_code)]
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Get the global `SyntaxSet` instance, loading it on first access.
#[allow(dead_code)]
fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Get the global `ThemeSet` instance, loading it on first access.
#[allow(dead_code)]
fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Language aliases for common extensions.
///
/// Maps full language names to their file extensions.
#[allow(dead_code)]
fn resolve_language_alias(lang: &str) -> String {
    let lower = lang.to_lowercase();
    match lower.as_str() {
        "typescript" | "ts" => "ts".to_string(),
        "javascript" | "js" => "js".to_string(),
        "python" | "py" => "py".to_string(),
        "yaml" | "yml" => "yaml".to_string(),
        "rust" | "rs" => "rs".to_string(),
        "ruby" | "rb" => "rb".to_string(),
        "golang" | "go" => "go".to_string(),
        "csharp" | "c#" | "cs" => "cs".to_string(),
        "cpp" | "c++" | "cxx" => "cpp".to_string(),
        "markdown" | "md" => "md".to_string(),
        "shell" | "bash" | "sh" | "zsh" => "sh".to_string(),
        "dockerfile" => "Dockerfile".to_string(),
        "makefile" | "make" => "Makefile".to_string(),
        _ => lower,
    }
}

/// Convert a syntect `Style` to a ratatui `Style`.
///
/// Maps the foreground color and font style (bold, italic) from syntect to ratatui equivalents.
#[must_use]
#[allow(dead_code)]
pub fn syntect_to_ratatui_style(style: SyntectStyle) -> RatatuiStyle {
    let fg = style.foreground;
    let mut ratatui_style = RatatuiStyle::default().fg(Color::Rgb(fg.r, fg.g, fg.b));

    let mut modifiers = Modifier::empty();
    if style.font_style.contains(FontStyle::BOLD) {
        modifiers |= Modifier::BOLD;
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        modifiers |= Modifier::ITALIC;
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        modifiers |= Modifier::UNDERLINED;
    }

    if !modifiers.is_empty() {
        ratatui_style = ratatui_style.add_modifier(modifiers);
    }

    ratatui_style
}

/// Highlight a code string and return styled spans.
///
/// # Arguments
///
/// * `code` - The source code to highlight
/// * `language` - The language extension or name (e.g., "rs", "rust", "typescript", "ts")
///
/// # Returns
///
/// A vector of styled spans. If the language is not recognized, returns unstyled text.
///
/// # Example
///
/// ```ignore
/// use omni_cli::tui::components::highlighting::highlight_code;
///
/// let spans = highlight_code("fn main() {}", "rs");
/// assert!(!spans.is_empty());
/// ```
#[must_use]
pub fn highlight_code(code: &str, language: &str) -> Vec<Span<'static>> {
    let ps = syntax_set();
    let ts = theme_set();

    let ext = resolve_language_alias(language);
    let syntax = ps
        .find_syntax_by_extension(&ext)
        .or_else(|| ps.find_syntax_by_name(language));

    let Some(syntax) = syntax else {
        return code
            .lines()
            .flat_map(|line| vec![Span::raw(line.to_owned()), Span::raw("\n".to_owned())])
            .collect();
    };

    let theme = &ts.themes[DEFAULT_THEME];
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut spans = Vec::new();

    for line in LinesWithEndings::from(code) {
        match highlighter.highlight_line(line, ps) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    spans.push(Span::styled(
                        text.to_owned(),
                        syntect_to_ratatui_style(style),
                    ));
                }
            }
            Err(_) => {
                spans.push(Span::raw(line.to_owned()));
            }
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rust_highlighting() {
        let code = "fn main() {}";
        let spans = highlight_code(code, "rs");

        assert!(!spans.is_empty());

        let has_color = spans
            .iter()
            .any(|span| matches!(span.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(
            has_color,
            "Expected at least one colored span for Rust code"
        );
    }

    #[test]
    fn test_unknown_language_returns_unstyled() {
        let code = "some random text";
        let spans = highlight_code(code, "unknownlanguage123");

        assert!(!spans.is_empty());

        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("some random text"));
    }

    #[test]
    fn test_language_alias_typescript() {
        let code = "const x = 1;";
        let spans_ts = highlight_code(code, "ts");
        let spans_typescript = highlight_code(code, "typescript");

        assert!(!spans_ts.is_empty());
        assert!(!spans_typescript.is_empty());
        assert_eq!(spans_ts.len(), spans_typescript.len());

        for (s1, s2) in spans_ts.iter().zip(spans_typescript.iter()) {
            assert_eq!(s1.content, s2.content);
        }
    }

    #[test]
    fn test_language_alias_python() {
        let code = "def hello(): pass";
        let spans_py = highlight_code(code, "py");
        let spans_python = highlight_code(code, "python");

        assert!(!spans_py.is_empty());
        assert!(!spans_python.is_empty());
        assert_eq!(spans_py.len(), spans_python.len());
    }

    #[test]
    fn test_language_alias_yaml() {
        let code = "key: value";
        let spans_full = highlight_code(code, "yaml");
        let spans_short = highlight_code(code, "yml");

        assert!(!spans_full.is_empty());
        assert!(!spans_short.is_empty());
        assert_eq!(spans_full.len(), spans_short.len());
    }

    #[test]
    fn test_language_alias_javascript() {
        let code = "function test() {}";
        let spans_js = highlight_code(code, "js");
        let spans_javascript = highlight_code(code, "javascript");

        assert!(!spans_js.is_empty());
        assert!(!spans_javascript.is_empty());
        assert_eq!(spans_js.len(), spans_javascript.len());
    }

    #[test]
    fn test_oncelock_singleton_behavior() {
        let ps1 = syntax_set();
        let ps2 = syntax_set();
        let ts1 = theme_set();
        let ts2 = theme_set();

        assert!(std::ptr::eq(ps1, ps2));
        assert!(std::ptr::eq(ts1, ts2));
    }

    #[test]
    fn test_multiline_code() {
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        let spans = highlight_code(code, "rs");

        assert!(!spans.is_empty());

        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains('\n'));
    }

    #[test]
    fn test_empty_code() {
        let spans = highlight_code("", "rs");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_style_conversion() {
        let syntect_style = SyntectStyle {
            foreground: syntect::highlighting::Color {
                r: 100,
                g: 150,
                b: 200,
                a: 255,
            },
            background: syntect::highlighting::Color::WHITE,
            font_style: FontStyle::BOLD | FontStyle::ITALIC,
        };

        let ratatui_style = syntect_to_ratatui_style(syntect_style);

        assert_eq!(ratatui_style.fg, Some(Color::Rgb(100, 150, 200)));
        assert!(ratatui_style.add_modifier.contains(Modifier::BOLD));
        assert!(ratatui_style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_syntax_set_access_performance() {
        use std::time::Instant;

        let start = Instant::now();
        let _ = syntax_set();
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 100,
            "SyntaxSet access took {}ms, expected < 100ms",
            elapsed.as_millis()
        );

        eprintln!("[perf] syntax_set() access: {:?}", elapsed);
    }

    #[test]
    fn test_theme_set_access_performance() {
        use std::time::Instant;

        let start = Instant::now();
        let _ = theme_set();
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 100,
            "ThemeSet access took {}ms, expected < 100ms",
            elapsed.as_millis()
        );

        eprintln!("[perf] theme_set() access: {:?}", elapsed);
    }

    #[test]
    fn test_oncelock_cached_access_is_fast() {
        use std::time::Instant;

        let _ = syntax_set();
        let _ = theme_set();

        let start_syntax = Instant::now();
        let _ = syntax_set();
        let elapsed_syntax = start_syntax.elapsed();

        let start_theme = Instant::now();
        let _ = theme_set();
        let elapsed_theme = start_theme.elapsed();

        assert!(
            elapsed_syntax.as_micros() < 100,
            "Cached syntax_set() took {}μs, expected < 100μs",
            elapsed_syntax.as_micros()
        );
        assert!(
            elapsed_theme.as_micros() < 100,
            "Cached theme_set() took {}μs, expected < 100μs",
            elapsed_theme.as_micros()
        );

        eprintln!(
            "[perf] Cached access - syntax_set: {:?}, theme_set: {:?}",
            elapsed_syntax, elapsed_theme
        );
    }

    #[test]
    fn test_highlight_100_lines_performance() {
        use std::time::Instant;

        let code_lines: Vec<String> = (0..100)
            .map(|i| {
                format!(
                    "    fn function_{}(x: i32, y: &str) -> Result<String, Error> {{ Ok(format!(\"{{}} {{}}\", x, y)) }}",
                    i
                )
            })
            .collect();
        let code = code_lines.join("\n");

        let _ = highlight_code("fn test() {}", "rs");

        let start = Instant::now();
        let spans = highlight_code(&code, "rs");
        let elapsed = start.elapsed();

        assert!(!spans.is_empty(), "Expected spans for 100 lines of code");
        assert!(
            elapsed.as_millis() < 50,
            "Highlighting 100 lines took {}ms, expected < 50ms",
            elapsed.as_millis()
        );

        eprintln!(
            "[perf] Highlighted 100 lines ({} spans) in {:?}",
            spans.len(),
            elapsed
        );
    }

    #[test]
    fn test_highlight_performance_multiple_languages() {
        use std::time::Instant;

        let test_cases = [
            ("rs", "fn main() { let x = 42; println!(\"{}\", x); }"),
            ("py", "def main():\n    x = 42\n    print(f'{x}')"),
            (
                "ts",
                "const main = (): void => { const x = 42; console.log(x); };",
            ),
            ("js", "function main() { const x = 42; console.log(x); }"),
        ];

        for (lang, code) in test_cases {
            let start = Instant::now();
            let spans = highlight_code(code, lang);
            let elapsed = start.elapsed();

            assert!(!spans.is_empty(), "Expected spans for {} code", lang);
            assert!(
                elapsed.as_millis() < 50,
                "Highlighting {} code took {}ms, expected < 50ms",
                lang,
                elapsed.as_millis()
            );

            eprintln!("[perf] {} highlighting: {:?}", lang, elapsed);
        }
    }
}
