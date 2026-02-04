//! Text layout module with word-aware wrapping and character position tracking.

/// A single wrapped line with character position tracking
#[derive(Debug, Clone, PartialEq)]
pub struct WrappedLine {
    /// The text content of this wrapped line
    pub text: String,
    /// Starting character index (inclusive) from original text
    pub char_start: usize,
    /// Ending character index (exclusive) from original text
    pub char_end: usize,
}

/// Complete layout information for wrapped text
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// All wrapped lines
    pub lines: Vec<WrappedLine>,
    /// Total number of visual lines
    pub total_lines: usize,
    /// The width used for wrapping
    #[allow(dead_code)]
    pub width: usize,
}

impl TextLayout {
    /// Create a new text layout with word-aware wrapping
    pub fn new(text: &str, width: usize) -> Self {
        if width == 0 {
            return Self {
                lines: vec![WrappedLine {
                    text: String::new(),
                    char_start: 0,
                    char_end: 0,
                }],
                total_lines: 1,
                width: 0,
            };
        }

        let mut lines = Vec::new();
        let mut char_offset = 0;

        // Split by explicit newlines first
        for (line_idx, logical_line) in text.split('\n').enumerate() {
            if line_idx > 0 {
                // Account for the newline character
                char_offset += 1;
            }

            if logical_line.is_empty() {
                lines.push(WrappedLine {
                    text: String::new(),
                    char_start: char_offset,
                    char_end: char_offset,
                });
            } else {
                // Word-wrap this logical line
                let wrapped = Self::wrap_logical_line(logical_line, width, char_offset);
                char_offset += logical_line.chars().count();
                lines.extend(wrapped);
            }
        }

        if lines.is_empty() {
            lines.push(WrappedLine {
                text: String::new(),
                char_start: 0,
                char_end: 0,
            });
        }

        let total_lines = lines.len();
        Self {
            lines,
            total_lines,
            width,
        }
    }

    /// Convert cursor (char index) to visual position (row, col)
    pub fn cursor_to_visual(&self, cursor_char: usize) -> (usize, usize) {
        for (row, line) in self.lines.iter().enumerate() {
            if cursor_char >= line.char_start && cursor_char <= line.char_end {
                let col = cursor_char - line.char_start;
                return (row, col);
            }
        }
        let last_row = self.lines.len().saturating_sub(1);
        if let Some(last_line) = self.lines.last() {
            (last_row, last_line.char_end - last_line.char_start)
        } else {
            (0, 0)
        }
    }

    /// Convert visual position (row, col) to cursor (char index)
    pub fn visual_to_cursor(&self, row: usize, col: usize) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let clamped_row = row.min(self.lines.len() - 1);
        let is_past_last = row >= self.lines.len();

        if let Some(line) = self.lines.get(clamped_row) {
            let line_len = line.char_end - line.char_start;
            if is_past_last {
                line.char_end
            } else {
                let col = col.min(line_len);
                line.char_start + col
            }
        } else {
            0
        }
    }

    /// Get character index at start of visual row
    #[allow(dead_code)]
    pub fn row_start(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.char_start).unwrap_or(0)
    }

    /// Get character index at end of visual row
    #[allow(dead_code)]
    pub fn row_end(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.char_end).unwrap_or(0)
    }

    /// Wrap a single logical line (between newlines) by words
    fn wrap_logical_line(line: &str, width: usize, start_offset: usize) -> Vec<WrappedLine> {
        let mut result = Vec::new();
        let mut current_line = String::new();
        let mut current_line_start = start_offset;

        // Split into words (by whitespace)
        let words: Vec<&str> = line.split_whitespace().collect();

        if words.is_empty() {
            return result;
        }

        for word in &words {
            let word_len = word.chars().count();

            // Check if word fits on current line
            let current_len = current_line.chars().count();
            let space_needed = if current_len == 0 { 0 } else { 1 }; // space before word
            let total_needed = current_len + space_needed + word_len;

            if total_needed <= width {
                // Word fits on current line
                if current_len > 0 {
                    current_line.push(' ');
                }
                current_line.push_str(word);
            } else if word_len <= width {
                // Word doesn't fit, but it's not too long - start new line
                if !current_line.is_empty() {
                    // Save current line
                    let line_char_count = current_line.chars().count();
                    result.push(WrappedLine {
                        text: current_line.clone(),
                        char_start: current_line_start,
                        char_end: current_line_start + line_char_count,
                    });
                    current_line_start += line_char_count + 1; // +1 for space
                }
                current_line = word.to_string();
            } else {
                // Word is too long - need to chunk it by characters
                // First, save current line if not empty
                if !current_line.is_empty() {
                    let line_char_count = current_line.chars().count();
                    result.push(WrappedLine {
                        text: current_line.clone(),
                        char_start: current_line_start,
                        char_end: current_line_start + line_char_count,
                    });
                    current_line_start += line_char_count + 1; // +1 for space
                    current_line.clear();
                }

                // Chunk the long word by characters
                let mut word_chars = word.chars().peekable();
                let mut chunk = String::new();
                let _chunk_start = current_line_start;

                while let Some(ch) = word_chars.next() {
                    let chunk_len = chunk.chars().count();
                    if chunk_len >= width {
                        // Save chunk
                        result.push(WrappedLine {
                            text: chunk.clone(),
                            char_start: current_line_start,
                            char_end: current_line_start + chunk_len,
                        });
                        current_line_start += chunk_len;
                        chunk.clear();
                    }
                    chunk.push(ch);
                }

                // Save remaining chunk
                if !chunk.is_empty() {
                    let chunk_len = chunk.chars().count();
                    result.push(WrappedLine {
                        text: chunk.clone(),
                        char_start: current_line_start,
                        char_end: current_line_start + chunk_len,
                    });
                    current_line_start += chunk_len;
                }

                current_line.clear();
            }
        }

        // Save final line if not empty
        if !current_line.is_empty() {
            let line_char_count = current_line.chars().count();
            result.push(WrappedLine {
                text: current_line,
                char_start: current_line_start,
                char_end: current_line_start + line_char_count,
            });
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        let layout = TextLayout::new("", 10);
        assert_eq!(layout.total_lines, 1);
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].text, "");
        assert_eq!(layout.lines[0].char_start, 0);
        assert_eq!(layout.lines[0].char_end, 0);
    }

    #[test]
    fn test_single_word_fits() {
        let layout = TextLayout::new("hello", 10);
        assert_eq!(layout.total_lines, 1);
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].text, "hello");
        assert_eq!(layout.lines[0].char_start, 0);
        assert_eq!(layout.lines[0].char_end, 5);
    }

    #[test]
    fn test_word_wrap_basic() {
        let layout = TextLayout::new("hello world", 7);
        assert_eq!(layout.total_lines, 2);
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].text, "hello");
        assert_eq!(layout.lines[0].char_start, 0);
        assert_eq!(layout.lines[0].char_end, 5);
        assert_eq!(layout.lines[1].text, "world");
        assert_eq!(layout.lines[1].char_start, 6); // after "hello "
        assert_eq!(layout.lines[1].char_end, 11);
    }

    #[test]
    fn test_word_wrap_multiple() {
        let layout = TextLayout::new("one two three", 5);
        assert_eq!(layout.total_lines, 3);
        assert_eq!(layout.lines.len(), 3);
        assert_eq!(layout.lines[0].text, "one");
        assert_eq!(layout.lines[1].text, "two");
        assert_eq!(layout.lines[2].text, "three");
    }

    #[test]
    fn test_long_word_fallback() {
        let layout = TextLayout::new("https://example.com", 8);
        // Should chunk by characters since word is > width
        assert!(layout.total_lines >= 2);
        // Verify all text is preserved
        let combined: String = layout.lines.iter().map(|l| l.text.clone()).collect();
        assert_eq!(combined, "https://example.com");
    }

    #[test]
    fn test_mixed_content() {
        let layout = TextLayout::new("hi https://x.com bye", 6);
        // "hi" fits, "https://x.com" needs chunking, "bye" fits
        let combined: String = layout.lines.iter().map(|l| l.text.clone()).collect();
        assert_eq!(combined, "hihttps://x.combye");
    }

    #[test]
    fn test_preserves_explicit_newlines() {
        let layout = TextLayout::new("a\nb", 10);
        assert_eq!(layout.total_lines, 2);
        assert_eq!(layout.lines[0].text, "a");
        assert_eq!(layout.lines[0].char_start, 0);
        assert_eq!(layout.lines[0].char_end, 1);
        assert_eq!(layout.lines[1].text, "b");
        assert_eq!(layout.lines[1].char_start, 2); // after "a\n"
        assert_eq!(layout.lines[1].char_end, 3);
    }

    #[test]
    fn test_trailing_newline() {
        let layout = TextLayout::new("hello\n", 10);
        assert_eq!(layout.total_lines, 2);
        assert_eq!(layout.lines[0].text, "hello");
        assert_eq!(layout.lines[1].text, "");
        assert_eq!(layout.lines[1].char_start, 6);
        assert_eq!(layout.lines[1].char_end, 6);
    }

    #[test]
    fn test_unicode_words() {
        let layout = TextLayout::new("日本語 テスト", 6);
        // Each character is 1 char unit in Rust
        assert_eq!(layout.total_lines, 2);
        assert_eq!(layout.lines[0].text, "日本語");
        assert_eq!(layout.lines[1].text, "テスト");
    }

    #[test]
    fn test_char_indices_correct() {
        let text = "hello world test";
        let layout = TextLayout::new(text, 7);

        // Verify char indices map back to original text
        for line in &layout.lines {
            let chars: Vec<char> = text.chars().collect();
            let extracted_chars: String = chars[line.char_start..line.char_end].iter().collect();
            assert_eq!(extracted_chars, line.text);
        }
    }

    #[test]
    fn test_width_zero() {
        let layout = TextLayout::new("hello", 0);
        assert_eq!(layout.total_lines, 1);
        assert_eq!(layout.lines[0].text, "");
    }

    #[test]
    fn test_multiple_spaces_between_words() {
        let layout = TextLayout::new("hello  world", 10);
        // split_whitespace handles multiple spaces, "hello world" is 11 chars so wraps
        assert_eq!(layout.total_lines, 2);
        assert_eq!(layout.lines[0].text, "hello");
        assert_eq!(layout.lines[1].text, "world");
    }

    #[test]
    fn test_word_exactly_fits_width() {
        let layout = TextLayout::new("hello world", 5);
        // "hello" is exactly 5 chars, "world" is exactly 5 chars
        assert_eq!(layout.total_lines, 2);
        assert_eq!(layout.lines[0].text, "hello");
        assert_eq!(layout.lines[1].text, "world");
    }

    #[test]
    fn test_cursor_to_visual_empty() {
        let layout = TextLayout::new("", 10);
        assert_eq!(layout.cursor_to_visual(0), (0, 0));
    }

    #[test]
    fn test_cursor_to_visual_single_line() {
        let layout = TextLayout::new("hello", 10);
        assert_eq!(layout.cursor_to_visual(0), (0, 0));
        assert_eq!(layout.cursor_to_visual(3), (0, 3));
        assert_eq!(layout.cursor_to_visual(5), (0, 5)); // end of text
    }

    #[test]
    fn test_cursor_to_visual_at_wrap() {
        let layout = TextLayout::new("hello world", 7);
        // "hello" is line 0, "world" is line 1
        // cursor at position 6 (the 'w' in world) -> (1, 0)
        assert_eq!(layout.cursor_to_visual(6), (1, 0));
        assert_eq!(layout.cursor_to_visual(7), (1, 1)); // 'o' in world
    }

    #[test]
    fn test_cursor_to_visual_second_line() {
        let layout = TextLayout::new("hello world", 7);
        assert_eq!(layout.cursor_to_visual(8), (1, 2)); // 'r' in world
        assert_eq!(layout.cursor_to_visual(11), (1, 5)); // end of text
    }

    #[test]
    fn test_cursor_to_visual_after_newline() {
        let layout = TextLayout::new("a\nb", 10);
        assert_eq!(layout.cursor_to_visual(0), (0, 0)); // 'a'
        assert_eq!(layout.cursor_to_visual(1), (0, 1)); // end of first line
        assert_eq!(layout.cursor_to_visual(2), (1, 0)); // 'b'
        assert_eq!(layout.cursor_to_visual(3), (1, 1)); // end of text
    }

    #[test]
    fn test_cursor_to_visual_end_of_text() {
        let layout = TextLayout::new("hello", 10);
        assert_eq!(layout.cursor_to_visual(5), (0, 5)); // cursor at end
        assert_eq!(layout.cursor_to_visual(100), (0, 5)); // past end
    }

    #[test]
    fn test_visual_to_cursor_basic() {
        let layout = TextLayout::new("hello", 10);
        assert_eq!(layout.visual_to_cursor(0, 0), 0);
        assert_eq!(layout.visual_to_cursor(0, 3), 3);
        assert_eq!(layout.visual_to_cursor(0, 5), 5);
    }

    #[test]
    fn test_visual_to_cursor_wrapped_line() {
        let layout = TextLayout::new("hello world", 7);
        // Line 0: "hello" (chars 0-5)
        // Line 1: "world" (chars 6-11)
        assert_eq!(layout.visual_to_cursor(1, 0), 6);
        assert_eq!(layout.visual_to_cursor(1, 2), 8);
    }

    #[test]
    fn test_visual_to_cursor_past_line_end() {
        let layout = TextLayout::new("hi", 10);
        // Request column past end of line -> clamp to end
        assert_eq!(layout.visual_to_cursor(0, 100), 2);
    }

    #[test]
    fn test_visual_to_cursor_past_last_line() {
        let layout = TextLayout::new("hello", 10);
        // Request row past last line -> use last line, clamp column
        assert_eq!(layout.visual_to_cursor(100, 0), 5);
    }

    #[test]
    fn test_row_start_and_end() {
        let layout = TextLayout::new("hello world", 7);
        assert_eq!(layout.row_start(0), 0);
        assert_eq!(layout.row_end(0), 5);
        assert_eq!(layout.row_start(1), 6);
        assert_eq!(layout.row_end(1), 11);
    }
}
