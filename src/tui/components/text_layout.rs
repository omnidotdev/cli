//! Text layout module with word-aware wrapping and character position tracking.

/// A single wrapped line with character position tracking
#[derive(Debug, Clone, PartialEq, Eq)]
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
                // Clamp col to the actual line width to prevent cursor from leaking
                let col = col.min(line.text.chars().count());
                return (row, col);
            }
        }
        let last_row = self.lines.len().saturating_sub(1);
        if let Some(last_line) = self.lines.last() {
            let col = last_line.char_end - last_line.char_start;
            // Clamp col to the actual line width
            let col = col.min(last_line.text.chars().count());
            (last_row, col)
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
        self.lines.get(row).map_or(0, |l| l.char_start)
    }

    /// Get character index at end of visual row
    #[allow(dead_code)]
    pub fn row_end(&self, row: usize) -> usize {
        self.lines.get(row).map_or(0, |l| l.char_end)
    }

    fn wrap_logical_line(line: &str, width: usize, start_offset: usize) -> Vec<WrappedLine> {
        let mut result = Vec::new();
        let mut current_line = String::new();
        let mut current_line_start = start_offset;
        let mut char_pos = start_offset;

        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            while i < chars.len() && chars[i].is_whitespace() && current_line.is_empty() {
                char_pos += 1;
                current_line_start = char_pos;
                i += 1;
            }

            if i >= chars.len() {
                break;
            }

            let word_start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            let word: String = chars[word_start..i].iter().collect();
            let word_len = word.chars().count();

            let current_len = current_line.chars().count();
            let space_needed = usize::from(current_len != 0);
            let total_needed = current_len + space_needed + word_len;

            if total_needed <= width {
                if current_len > 0 {
                    current_line.push(' ');
                }
                current_line.push_str(&word);
                char_pos += word_len;
            } else if word_len <= width {
                if !current_line.is_empty() {
                    result.push(WrappedLine {
                        text: current_line.clone(),
                        char_start: current_line_start,
                        char_end: char_pos,
                    });
                }
                current_line_start = start_offset + word_start;
                current_line = word;
                char_pos = start_offset + i;
            } else {
                if !current_line.is_empty() {
                    result.push(WrappedLine {
                        text: current_line.clone(),
                        char_start: current_line_start,
                        char_end: char_pos,
                    });
                    current_line.clear();
                }

                current_line_start = start_offset + word_start;
                for ch in word.chars() {
                    if current_line.chars().count() >= width {
                        result.push(WrappedLine {
                            text: current_line.clone(),
                            char_start: current_line_start,
                            char_end: current_line_start + current_line.chars().count(),
                        });
                        current_line_start += current_line.chars().count();
                        current_line.clear();
                    }
                    current_line.push(ch);
                }
                char_pos = start_offset + i;
            }

            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
        }

        if !current_line.is_empty() {
            result.push(WrappedLine {
                text: current_line,
                char_start: current_line_start,
                char_end: start_offset + chars.len(),
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
        let layout = TextLayout::new("hi https://x.com bye", 10);
        assert!(layout.total_lines >= 2);
        assert_eq!(layout.lines[0].text, "hi");
        assert_eq!(layout.lines[0].char_start, 0);
        assert_eq!(layout.lines[0].char_end, 2);
        let last = layout.lines.last().unwrap();
        assert!(last.text.contains("bye"));
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
    fn test_cursor_bounded_trailing_spaces() {
        let layout = TextLayout::new("hello     ", 8);
        let (row, col) = layout.cursor_to_visual(10);
        assert_eq!(row, 0);
        assert!(
            col <= layout.lines[row].text.chars().count(),
            "col {} exceeds line width {}",
            col,
            layout.lines[row].text.chars().count()
        );
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

    #[test]
    fn test_navigation_roundtrip() {
        let text = "hello world test";
        let layout = TextLayout::new(text, 7);

        for char_idx in 0..=text.chars().count() {
            let (row, col) = layout.cursor_to_visual(char_idx);
            let back = layout.visual_to_cursor(row, col);
            assert_eq!(
                back, char_idx,
                "Roundtrip failed for char_idx={char_idx}: got ({row}, {col}) -> {back}"
            );
        }
    }

    #[test]
    fn test_navigation_up_down() {
        let text = "hello world test";
        let layout = TextLayout::new(text, 7);

        let start_char = 8;
        let (row, col) = layout.cursor_to_visual(start_char);
        assert_eq!(row, 1);
        assert_eq!(col, 2);

        let up_char = layout.visual_to_cursor(0, col);
        assert_eq!(up_char, 2);

        let down_char = layout.visual_to_cursor(2, col);
        assert_eq!(down_char, 14);
    }
}
