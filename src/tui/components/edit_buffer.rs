//! Edit buffer for logical text state management.
//!
//! This module provides a buffer for text input with cursor management,
//! following the pattern from `OpenTUI`'s `EditBuffer`.

/// A buffer for text input with cursor management.
#[derive(Debug, Clone, Default)]
pub struct EditBuffer {
    text: String,
    cursor: usize,                   // Byte offset into text
    preferred_column: Option<usize>, // Character column for up/down nav
}

impl EditBuffer {
    /// Create a new empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a buffer with initial text, cursor at end.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            preferred_column: None,
        }
    }

    /// Get the text content.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the cursor position (byte offset).
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Get the preferred column for vertical navigation.
    #[must_use]
    pub const fn preferred_column(&self) -> Option<usize> {
        self.preferred_column
    }

    /// Get the length of the text in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Check if the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Set cursor position with bounds checking.
    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.text.len());
    }

    /// Move cursor left by one character.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            // Find previous char boundary
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
        }
        self.preferred_column = None;
    }

    /// Move cursor right by one character.
    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            // Find next char boundary
            self.cursor = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map_or(self.text.len(), |(i, _)| self.cursor + i);
        }
        self.preferred_column = None;
    }

    /// Move cursor left by one word.
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let before = &self.text[..self.cursor];
        let mut chars: Vec<(usize, char)> = before.char_indices().collect();
        chars.reverse();

        // Skip whitespace
        while let Some(&(_, c)) = chars.first() {
            if !c.is_whitespace() {
                break;
            }
            chars.remove(0);
        }

        // Skip word characters
        while let Some(&(i, c)) = chars.first() {
            if c.is_whitespace() {
                self.cursor = i + c.len_utf8();
                return;
            }
            chars.remove(0);
        }

        self.cursor = 0;
    }

    /// Move cursor right by one word.
    pub fn move_word_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }

        let after = &self.text[self.cursor..];
        let mut chars = after.char_indices().peekable();

        // Skip current word characters
        while let Some(&(_, c)) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            chars.next();
        }

        // Skip whitespace
        while let Some(&(_, c)) = chars.peek() {
            if !c.is_whitespace() {
                break;
            }
            chars.next();
        }

        self.cursor = chars
            .peek()
            .map_or(self.text.len(), |&(i, _)| self.cursor + i);
    }

    /// Insert character at cursor.
    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.preferred_column = None;
    }

    /// Delete character before cursor (backspace).
    pub fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
        }
        self.preferred_column = None;
    }

    /// Delete character after cursor (delete key).
    pub fn delete_char_after(&mut self) {
        if self.cursor < self.text.len() {
            let next = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map_or(self.text.len(), |(i, _)| self.cursor + i);
            self.text.drain(self.cursor..next);
        }
        self.preferred_column = None;
    }

    /// Delete from cursor to beginning of line.
    pub fn delete_to_start(&mut self) {
        self.text.drain(..self.cursor);
        self.cursor = 0;
    }

    /// Delete from cursor to end of line.
    pub fn delete_to_end(&mut self) {
        self.text.truncate(self.cursor);
    }

    /// Delete word before cursor.
    pub fn delete_word(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Find start of word (skip trailing spaces, then skip word chars)
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end();

        let word_start = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map_or(0, |i| i + 1);

        self.text.drain(word_start..self.cursor);
        self.cursor = word_start;
    }

    /// Insert newline at cursor.
    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    /// Clear all text and reset cursor.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Convert byte index to character index.
    #[must_use]
    pub fn byte_to_char_index(&self, byte_idx: usize) -> usize {
        self.text[..byte_idx.min(self.text.len())].chars().count()
    }

    /// Convert character index to byte index.
    #[must_use]
    pub fn char_to_byte_index(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// Get the current cursor position as (`line_index`, `column`).
    ///
    /// Line index is 0-based, column is the character count from line start.
    #[must_use]
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let before_cursor = &self.text[..self.cursor];
        let line_index = before_cursor.matches('\n').count();
        let line_start = before_cursor.rfind('\n').map_or(0, |i| i + 1);
        let column = before_cursor[line_start..].chars().count();
        (line_index, column)
    }

    /// Check if input contains multiple lines.
    #[must_use]
    pub fn is_multiline(&self) -> bool {
        self.text.contains('\n')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty() {
        let buf = EditBuffer::new();
        assert_eq!(buf.text(), "");
        assert_eq!(buf.cursor(), 0);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn with_text_sets_cursor_at_end() {
        let buf = EditBuffer::with_text("hello");
        assert_eq!(buf.text(), "hello");
        assert_eq!(buf.cursor(), 5);
        assert_eq!(buf.len(), 5);
        assert!(!buf.is_empty());
    }

    #[test]
    fn insert_char_advances_cursor() {
        let mut buf = EditBuffer::new();
        buf.insert_char('a');
        assert_eq!(buf.text(), "a");
        assert_eq!(buf.cursor(), 1);

        buf.insert_char('b');
        assert_eq!(buf.text(), "ab");
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn insert_char_at_middle() {
        let mut buf = EditBuffer::with_text("ac");
        buf.set_cursor(1);
        buf.insert_char('b');
        assert_eq!(buf.text(), "abc");
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn delete_char_before_removes_previous() {
        let mut buf = EditBuffer::with_text("abc");
        buf.delete_char_before();
        assert_eq!(buf.text(), "ab");
        assert_eq!(buf.cursor(), 2);

        buf.delete_char_before();
        assert_eq!(buf.text(), "a");
        assert_eq!(buf.cursor(), 1);
    }

    #[test]
    fn delete_char_before_at_start_does_nothing() {
        let mut buf = EditBuffer::with_text("abc");
        buf.set_cursor(0);
        buf.delete_char_before();
        assert_eq!(buf.text(), "abc");
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn delete_char_after_removes_next() {
        let mut buf = EditBuffer::with_text("abc");
        buf.set_cursor(0);
        buf.delete_char_after();
        assert_eq!(buf.text(), "bc");
        assert_eq!(buf.cursor(), 0);

        buf.delete_char_after();
        assert_eq!(buf.text(), "c");
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn delete_char_after_at_end_does_nothing() {
        let mut buf = EditBuffer::with_text("abc");
        buf.delete_char_after();
        assert_eq!(buf.text(), "abc");
        assert_eq!(buf.cursor(), 3);
    }

    #[test]
    fn move_left_at_start_does_nothing() {
        let mut buf = EditBuffer::with_text("abc");
        buf.set_cursor(0);
        buf.move_left();
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn move_left_moves_by_char() {
        let mut buf = EditBuffer::with_text("abc");
        buf.move_left();
        assert_eq!(buf.cursor(), 2);
        buf.move_left();
        assert_eq!(buf.cursor(), 1);
    }

    #[test]
    fn move_right_at_end_does_nothing() {
        let mut buf = EditBuffer::with_text("abc");
        buf.move_right();
        assert_eq!(buf.cursor(), 3);
    }

    #[test]
    fn move_right_moves_by_char() {
        let mut buf = EditBuffer::with_text("abc");
        buf.set_cursor(0);
        buf.move_right();
        assert_eq!(buf.cursor(), 1);
        buf.move_right();
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn move_word_left_skips_whitespace_and_word() {
        let mut buf = EditBuffer::with_text("hello world");
        buf.move_word_left();
        assert_eq!(buf.cursor(), 6); // After "hello "

        buf.move_word_left();
        assert_eq!(buf.cursor(), 0); // Start
    }

    #[test]
    fn move_word_right_skips_word_and_whitespace() {
        let mut buf = EditBuffer::with_text("hello world");
        buf.set_cursor(0);
        buf.move_word_right();
        assert_eq!(buf.cursor(), 6); // Before "world"

        buf.move_word_right();
        assert_eq!(buf.cursor(), 11); // End
    }

    #[test]
    fn delete_to_start_removes_before_cursor() {
        let mut buf = EditBuffer::with_text("hello world");
        buf.set_cursor(5);
        buf.delete_to_start();
        assert_eq!(buf.text(), " world");
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn delete_to_end_removes_after_cursor() {
        let mut buf = EditBuffer::with_text("hello world");
        buf.set_cursor(5);
        buf.delete_to_end();
        assert_eq!(buf.text(), "hello");
        assert_eq!(buf.cursor(), 5);
    }

    #[test]
    fn delete_word_removes_previous_word() {
        let mut buf = EditBuffer::with_text("hello world");
        buf.delete_word();
        assert_eq!(buf.text(), "hello ");
        assert_eq!(buf.cursor(), 6);

        buf.delete_word();
        assert_eq!(buf.text(), "");
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn clear_resets_buffer() {
        let mut buf = EditBuffer::with_text("hello");
        buf.clear();
        assert_eq!(buf.text(), "");
        assert_eq!(buf.cursor(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn insert_newline_adds_line_break() {
        let mut buf = EditBuffer::with_text("hello");
        buf.insert_newline();
        assert_eq!(buf.text(), "hello\n");
        assert!(buf.is_multiline());
    }

    #[test]
    fn cursor_line_col_single_line() {
        let mut buf = EditBuffer::with_text("hello");
        buf.set_cursor(2);
        assert_eq!(buf.cursor_line_col(), (0, 2));
    }

    #[test]
    fn cursor_line_col_multiline() {
        let mut buf = EditBuffer::with_text("hello\nworld");
        buf.set_cursor(8); // "w" in "world"
        assert_eq!(buf.cursor_line_col(), (1, 2));
    }

    #[test]
    fn byte_to_char_index_ascii() {
        let buf = EditBuffer::with_text("hello");
        assert_eq!(buf.byte_to_char_index(0), 0);
        assert_eq!(buf.byte_to_char_index(2), 2);
        assert_eq!(buf.byte_to_char_index(5), 5);
    }

    #[test]
    fn byte_to_char_index_unicode() {
        let buf = EditBuffer::with_text("h€llo"); // € is 3 bytes
        assert_eq!(buf.byte_to_char_index(0), 0);
        assert_eq!(buf.byte_to_char_index(1), 1); // Before €
        assert_eq!(buf.byte_to_char_index(4), 2); // After €
        assert_eq!(buf.byte_to_char_index(7), 5);
    }

    #[test]
    fn char_to_byte_index_ascii() {
        let buf = EditBuffer::with_text("hello");
        assert_eq!(buf.char_to_byte_index(0), 0);
        assert_eq!(buf.char_to_byte_index(2), 2);
        assert_eq!(buf.char_to_byte_index(5), 5);
    }

    #[test]
    fn char_to_byte_index_unicode() {
        let buf = EditBuffer::with_text("h€llo"); // € is 3 bytes
        assert_eq!(buf.char_to_byte_index(0), 0);
        assert_eq!(buf.char_to_byte_index(1), 1); // Before €
        assert_eq!(buf.char_to_byte_index(2), 4); // After €
        assert_eq!(buf.char_to_byte_index(5), 7);
    }

    #[test]
    fn set_cursor_clamps_to_bounds() {
        let mut buf = EditBuffer::with_text("hello");
        buf.set_cursor(100);
        assert_eq!(buf.cursor(), 5);

        buf.set_cursor(2);
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn unicode_handling_emoji() {
        let mut buf = EditBuffer::with_text("a👍b");
        assert_eq!(buf.len(), 6); // a(1) + 👍(4) + b(1)

        buf.set_cursor(1);
        buf.move_right();
        assert_eq!(buf.cursor(), 5); // After emoji

        buf.move_left();
        assert_eq!(buf.cursor(), 1); // Before emoji
    }

    #[test]
    fn preferred_column_cleared_on_horizontal_movement() {
        let mut buf = EditBuffer::with_text("hello");
        buf.preferred_column = Some(3);

        buf.move_left();
        assert_eq!(buf.preferred_column(), None);

        buf.preferred_column = Some(3);
        buf.move_right();
        assert_eq!(buf.preferred_column(), None);

        buf.preferred_column = Some(3);
        buf.insert_char('x');
        assert_eq!(buf.preferred_column(), None);
    }
}
