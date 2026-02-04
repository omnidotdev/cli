//! File picker component for @-mention file context feature.
//!
//! Provides file scanning and filtering functionality for the TUI.

use std::path::PathBuf;

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
    fn test_fuzzy_filter_files_case_insensitive() {
        let files = vec![
            PathBuf::from("src/Main.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("tests/test.rs"),
        ];

        let results = fuzzy_filter_files("MAIN", &files);
        assert_eq!(results.len(), 2);
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
}
