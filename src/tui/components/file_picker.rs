//! File picker component for @-mention file context feature.
//!
//! Provides file scanning and filtering functionality for the TUI.

use std::path::PathBuf;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// Dropdown background color
const DROPDOWN_BG: Color = Color::Rgb(35, 38, 45);
/// Selected item background color
const SELECTED_BG: Color = Color::Rgb(50, 55, 65);
/// Amber color for file names
const FILE_REF_COLOR: Color = Color::Rgb(200, 160, 100);
/// Dimmed text color
const DIMMED: Color = Color::Rgb(100, 100, 110);
/// Maximum visible items in dropdown
const MAX_VISIBLE_ITEMS: usize = 10;

/// Binary file extensions to filter out.
const BINARY_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "o", "a", "bin", "png", "jpg", "jpeg", "gif", "bmp", "ico",
    "webp", "svg", "pdf", "zip", "tar", "gz", "rar", "7z", "mp3", "mp4", "wav", "flac", "mov",
    "avi", "mkv", "class", "pyc", "pyo", "so", "o", "a", "lib", "rlib", "wasm",
];

/// List all project files respecting .gitignore.
///
/// Uses `ignore::WalkBuilder` to traverse the project directory and respect .gitignore rules.
/// Filters out binary files and directories, returning only regular files.
/// Capped at 1000 files maximum.
///
/// # Returns
///
/// A vector of `PathBuf` representing project files, sorted by path.
#[must_use]
pub fn list_project_files() -> Vec<PathBuf> {
    let mut files = Vec::new();

    let walker = ignore::WalkBuilder::new(".")
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .standard_filters(true)
        .build();

    for entry in walker.flatten() {
        if files.len() >= 1000 {
            break;
        }

        let path = entry.path();

        // Skip directories, only include files
        if !path.is_file() {
            continue;
        }

        // Filter out binary files by extension
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                if BINARY_EXTENSIONS.contains(&ext_str.to_lowercase().as_str()) {
                    continue;
                }
            }
        }

        files.push(path.to_path_buf());
    }

    files.sort();
    files
}

/// Filter files by query using case-insensitive contains matching.
///
/// Sorts results by path length (shorter paths ranked higher).
/// This provides a simple but effective fuzzy-like experience without external dependencies.
///
/// # Arguments
///
/// * `query` - The search query (case-insensitive)
/// * `files` - The list of files to filter
///
/// # Returns
///
/// A vector of matching `PathBuf` sorted by path length (ascending).
#[must_use]
pub fn fuzzy_filter_files(query: &str, files: &[PathBuf]) -> Vec<PathBuf> {
    let query_lower = query.to_lowercase();

    let mut matches: Vec<PathBuf> = files
        .iter()
        .filter(|file| file.to_string_lossy().to_lowercase().contains(&query_lower))
        .cloned()
        .collect();

    // Sort by path length (shorter = better match)
    matches.sort_by_key(|path| path.to_string_lossy().len());

    matches
}

/// Extract the file query from input at cursor position.
///
/// Returns the query text after the @ symbol, or empty string if no @ found.
#[must_use]
pub fn extract_file_query(input: &str, cursor_pos: usize) -> String {
    if cursor_pos > input.len() {
        return String::new();
    }

    let input_bytes = input.as_bytes();
    let mut pos = cursor_pos;

    // Find the @ symbol by scanning backwards
    while pos > 0 {
        pos -= 1;
        let ch = input_bytes[pos];

        if ch == b'@' {
            // Check if @ is at word boundary
            if pos == 0 || input_bytes[pos - 1].is_ascii_whitespace() {
                // Return everything after @ up to cursor
                return input[pos + 1..cursor_pos].to_string();
            }
            return String::new();
        }

        if ch.is_ascii_whitespace() {
            return String::new();
        }
    }

    String::new()
}

/// Determine if the file dropdown should be shown.
///
/// Detects if the input contains an `@` symbol at a word boundary,
/// indicating the user is trying to mention a file.
///
/// # Arguments
///
/// * `input` - The current input text
/// * `cursor_pos` - The current cursor position in the input
///
/// # Returns
///
/// `true` if the dropdown should be shown, `false` otherwise.
#[must_use]
pub fn should_show_file_dropdown(input: &str, cursor_pos: usize) -> bool {
    // Ensure cursor position is valid
    if cursor_pos > input.len() {
        return false;
    }

    // Look backwards from cursor position to find @ symbol
    let input_bytes = input.as_bytes();
    let mut pos = cursor_pos;

    // Skip any trailing whitespace after cursor
    while pos > 0 && input_bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }

    // Look for @ symbol
    while pos > 0 {
        pos -= 1;
        let ch = input_bytes[pos];

        if ch == b'@' {
            // Check if @ is at word boundary (preceded by whitespace or start of input)
            if pos == 0 || input_bytes[pos - 1].is_ascii_whitespace() {
                return true;
            }
            return false;
        }

        // Stop if we hit whitespace (@ must be at word boundary)
        if ch.is_ascii_whitespace() {
            return false;
        }
    }

    false
}

fn truncate_path_start(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    format!("...{}", &path[path.len() - max_len + 3..])
}

fn format_file_display(path: &std::path::Path, max_width: usize) -> (String, String) {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let parent = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = parent.strip_prefix("./").unwrap_or(&parent).to_string();

    let name_len = file_name.len();
    let available_for_parent = max_width.saturating_sub(name_len + 3);

    let parent_display = if parent.is_empty() {
        String::new()
    } else if parent.len() > available_for_parent {
        truncate_path_start(&parent, available_for_parent)
    } else {
        parent
    };

    (file_name, parent_display)
}

/// Render the file dropdown above the prompt.
///
/// Returns the height used by the dropdown and the dropdown area.
#[allow(clippy::cast_possible_truncation)]
pub fn render_file_dropdown(
    frame: &mut Frame,
    prompt_area: Rect,
    query: &str,
    files: &[PathBuf],
    selected: usize,
) -> (u16, Rect) {
    let filtered = fuzzy_filter_files(query, files);
    let inner_width = prompt_area.width as usize;

    let has_more = filtered.len() > MAX_VISIBLE_ITEMS;
    let more_count = filtered.len().saturating_sub(MAX_VISIBLE_ITEMS);

    let lines: Vec<Line> = if filtered.is_empty() {
        let msg = " No files found";
        let padding = " ".repeat(inner_width.saturating_sub(msg.len()));
        vec![Line::from(vec![
            Span::styled(msg, Style::default().fg(DIMMED).bg(DROPDOWN_BG)),
            Span::styled(padding, Style::default().bg(DROPDOWN_BG)),
        ])]
    } else {
        let mut result: Vec<Line> = filtered
            .iter()
            .take(MAX_VISIBLE_ITEMS)
            .enumerate()
            .map(|(i, path)| {
                let is_selected = i == selected;
                let bg = if is_selected {
                    SELECTED_BG
                } else {
                    DROPDOWN_BG
                };

                let accent = if is_selected {
                    Span::styled("▎", Style::default().fg(FILE_REF_COLOR).bg(bg))
                } else {
                    Span::styled(" ", Style::default().bg(bg))
                };

                let (file_name, parent) = format_file_display(path, inner_width.saturating_sub(4));

                let name_style = if is_selected {
                    Style::default()
                        .fg(FILE_REF_COLOR)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(FILE_REF_COLOR).bg(bg)
                };

                let parent_style = Style::default().fg(DIMMED).bg(bg);

                let content_len = if parent.is_empty() {
                    1 + file_name.len()
                } else {
                    1 + file_name.len() + 2 + parent.len()
                };
                let padding_len = inner_width.saturating_sub(content_len);
                let padding = " ".repeat(padding_len);

                if parent.is_empty() {
                    Line::from(vec![
                        accent,
                        Span::styled(file_name, name_style),
                        Span::styled(padding, Style::default().bg(bg)),
                    ])
                } else {
                    Line::from(vec![
                        accent,
                        Span::styled(file_name, name_style),
                        Span::styled("  ", Style::default().bg(bg)),
                        Span::styled(parent, parent_style),
                        Span::styled(padding, Style::default().bg(bg)),
                    ])
                }
            })
            .collect();

        if has_more {
            let more_msg = format!(" ... and {more_count} more");
            let padding = " ".repeat(inner_width.saturating_sub(more_msg.len()));
            result.push(Line::from(vec![
                Span::styled(more_msg, Style::default().fg(DIMMED).bg(DROPDOWN_BG)),
                Span::styled(padding, Style::default().bg(DROPDOWN_BG)),
            ]));
        }

        result
    };

    let content_lines = lines.len().max(1);
    let dropdown_height = content_lines as u16;
    let dropdown_width = prompt_area.width;

    let dropdown_y = prompt_area.y.saturating_sub(dropdown_height);
    let dropdown_x = prompt_area.x;

    let dropdown_area = Rect::new(dropdown_x, dropdown_y, dropdown_width, dropdown_height);

    frame.render_widget(Clear, dropdown_area);

    let para = Paragraph::new(lines).style(Style::default().bg(DROPDOWN_BG));
    frame.render_widget(para, dropdown_area);

    (dropdown_height, dropdown_area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_show_file_dropdown_at_start() {
        assert!(should_show_file_dropdown("@", 1));
    }

    #[test]
    fn test_should_show_file_dropdown_after_space() {
        assert!(should_show_file_dropdown("hello @", 7));
    }

    #[test]
    fn test_should_show_file_dropdown_with_query() {
        assert!(should_show_file_dropdown("@src/", 5));
    }

    #[test]
    fn test_should_not_show_file_dropdown_no_at() {
        assert!(!should_show_file_dropdown("hello", 5));
    }

    #[test]
    fn test_should_not_show_file_dropdown_at_in_word() {
        assert!(!should_show_file_dropdown("email@example.com", 6));
    }

    #[test]
    fn test_should_not_show_file_dropdown_cursor_before_at() {
        assert!(!should_show_file_dropdown("@ hello", 0));
    }

    #[test]
    fn test_fuzzy_filter_case_insensitive() {
        let files = vec![
            PathBuf::from("src/Main.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("tests/test.rs"),
        ];

        let results = fuzzy_filter_files("main", &files);
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .any(|p| p.to_string_lossy().contains("Main.rs"))
        );
        assert!(
            results
                .iter()
                .any(|p| p.to_string_lossy().contains("main.rs"))
        );
    }

    #[test]
    fn test_fuzzy_filter_files_sorts_by_length() {
        let files = vec![
            PathBuf::from("src/components/button/main.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("main.rs"),
        ];

        let results = fuzzy_filter_files("main", &files);
        assert_eq!(results.len(), 3);
        // Shorter paths should come first
        assert_eq!(results[0], PathBuf::from("main.rs"));
        assert_eq!(results[1], PathBuf::from("src/main.rs"));
    }

    #[test]
    fn test_fuzzy_filter_files_empty_query() {
        let files = vec![PathBuf::from("src/main.rs"), PathBuf::from("tests/test.rs")];

        let results = fuzzy_filter_files("", &files);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_fuzzy_filter_sorts_by_length() {
        let files = vec![
            PathBuf::from("src/components/button/main.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("main.rs"),
        ];

        let results = fuzzy_filter_files("main", &files);
        assert_eq!(results.len(), 3);
        // Shorter paths should come first
        assert_eq!(results[0], PathBuf::from("main.rs"));
        assert_eq!(results[1], PathBuf::from("src/main.rs"));
        assert_eq!(results[2], PathBuf::from("src/components/button/main.rs"));
    }

    #[test]
    fn test_should_show_file_dropdown_word_boundary() {
        // @ at start of input
        assert!(should_show_file_dropdown("@", 1));
        // @ after space
        assert!(should_show_file_dropdown("hello @", 7));
        // @ after space with query
        assert!(should_show_file_dropdown("hello @src/", 11));
        // @ at start with query
        assert!(should_show_file_dropdown("@src/main.rs", 12));
    }

    #[test]
    fn test_should_show_file_dropdown_middle_of_word() {
        // @ in middle of word (email-like)
        assert!(!should_show_file_dropdown("email@example.com", 6));
        // @ in middle of word (not at boundary)
        assert!(!should_show_file_dropdown("test@file", 5));
        // @ in middle of word with cursor after
        assert!(!should_show_file_dropdown("user@domain.com", 10));
    }

    #[test]
    fn test_extract_file_query_basic() {
        assert_eq!(extract_file_query("@src/main.rs", 12), "src/main.rs");
        assert_eq!(extract_file_query("@file.txt", 9), "file.txt");
    }

    #[test]
    fn test_extract_file_query_with_space_before() {
        assert_eq!(extract_file_query("hello @src/main.rs", 18), "src/main.rs");
        assert_eq!(extract_file_query("check @file.txt please", 15), "file.txt");
    }

    #[test]
    fn test_extract_file_query_partial() {
        assert_eq!(extract_file_query("@src/main", 9), "src/main");
        assert_eq!(extract_file_query("@src/", 5), "src/");
    }

    #[test]
    fn test_extract_file_query_no_at_symbol() {
        assert_eq!(extract_file_query("hello world", 11), "");
        assert_eq!(extract_file_query("no mention here", 15), "");
    }

    #[test]
    fn test_extract_file_query_at_in_email() {
        // @ in email should not trigger
        assert_eq!(extract_file_query("user@example.com", 10), "");
    }

    #[test]
    fn test_extract_file_query_cursor_before_at() {
        assert_eq!(extract_file_query("@ hello", 0), "");
    }

    #[test]
    fn test_extract_file_query_cursor_in_middle() {
        assert_eq!(extract_file_query("@src/main.rs", 6), "src/m");
    }

    #[test]
    fn test_list_project_files_filters_binaries() {
        // This test verifies the binary extension filtering logic
        // We can't easily test the full function without a real project,
        // but we can verify the BINARY_EXTENSIONS constant is correct
        assert!(BINARY_EXTENSIONS.contains(&"exe"));
        assert!(BINARY_EXTENSIONS.contains(&"dll"));
        assert!(BINARY_EXTENSIONS.contains(&"so"));
        assert!(BINARY_EXTENSIONS.contains(&"dylib"));
        assert!(BINARY_EXTENSIONS.contains(&"png"));
        assert!(BINARY_EXTENSIONS.contains(&"jpg"));
        assert!(BINARY_EXTENSIONS.contains(&"mp4"));
        assert!(BINARY_EXTENSIONS.contains(&"zip"));
    }

    #[test]
    fn test_fuzzy_filter_unicode_paths() {
        let files = vec![
            PathBuf::from("src/café.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("tests/tëst.rs"),
        ];

        let results = fuzzy_filter_files("café", &files);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], PathBuf::from("src/café.rs"));
    }

    #[test]
    fn test_fuzzy_filter_no_matches() {
        let files = vec![PathBuf::from("src/main.rs"), PathBuf::from("tests/test.rs")];

        let results = fuzzy_filter_files("xyz", &files);
        assert!(results.is_empty());
    }

    #[test]
    fn test_should_show_file_dropdown_cursor_at_end() {
        assert!(should_show_file_dropdown("@", 1));
        assert!(should_show_file_dropdown("@src", 4));
    }

    #[test]
    fn test_should_show_file_dropdown_cursor_beyond_text() {
        // Cursor position beyond text length should return false
        assert!(!should_show_file_dropdown("@src", 10));
    }

    #[test]
    fn test_extract_file_query_empty_input() {
        assert_eq!(extract_file_query("", 0), "");
    }

    #[test]
    fn test_extract_file_query_only_at_symbol() {
        assert_eq!(extract_file_query("@", 1), "");
    }

    #[test]
    fn test_fuzzy_filter_partial_path_match() {
        let files = vec![
            PathBuf::from("src/components/button.rs"),
            PathBuf::from("src/components/input.rs"),
            PathBuf::from("src/utils/helpers.rs"),
        ];

        let results = fuzzy_filter_files("button", &files);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], PathBuf::from("src/components/button.rs"));
    }

    #[test]
    fn test_fuzzy_filter_multiple_matches_sorted() {
        let files = vec![
            PathBuf::from("a/b/c/d/e/f/test.rs"),
            PathBuf::from("test.rs"),
            PathBuf::from("src/test.rs"),
        ];

        let results = fuzzy_filter_files("test", &files);
        assert_eq!(results.len(), 3);
        // Verify sorted by length
        assert!(results[0].to_string_lossy().len() <= results[1].to_string_lossy().len());
        assert!(results[1].to_string_lossy().len() <= results[2].to_string_lossy().len());
    }
}
