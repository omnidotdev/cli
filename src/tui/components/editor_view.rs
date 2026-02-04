//! Editor view for visual cursor management.
//!
//! This module provides visual cursor positioning and line navigation,
//! wrapping `TextLayout` for soft-wrapped text display.

use super::edit_buffer::EditBuffer;
use super::text_layout::TextLayout;

/// Visual cursor position (row, col).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VisualCursor {
    pub row: usize,
    pub col: usize,
}

/// Editor view for visual cursor and scrolling management.
#[derive(Debug, Clone)]
pub struct EditorView {
    width: usize,
    scroll_offset: usize,
}

impl EditorView {
    #[must_use]
    pub const fn new(width: usize) -> Self {
        Self {
            width,
            scroll_offset: 0,
        }
    }

    #[allow(dead_code)]
    pub const fn set_width(&mut self, width: usize) {
        self.width = width;
    }

    #[allow(dead_code)]
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub const fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    /// Returns visual cursor position - ALWAYS BOUNDED to line width.
    #[must_use]
    pub fn get_visual_cursor(&self, buffer: &EditBuffer) -> VisualCursor {
        if buffer.is_empty() {
            return VisualCursor::default();
        }
        let layout = TextLayout::new(buffer.text(), self.width);
        let cursor_char = buffer.byte_to_char_index(buffer.cursor());
        let (row, col) = layout.cursor_to_visual(cursor_char);

        VisualCursor { row, col }
    }

    #[must_use]
    pub fn get_visual_line_count(&self, buffer: &EditBuffer) -> usize {
        if buffer.is_empty() {
            return 1;
        }
        let layout = TextLayout::new(buffer.text(), self.width);
        layout.total_lines
    }

    /// Character index at start of current visual line.
    #[must_use]
    pub fn get_visual_sol(&self, buffer: &EditBuffer) -> usize {
        let layout = TextLayout::new(buffer.text(), self.width);
        let cursor_char = buffer.byte_to_char_index(buffer.cursor());
        let (row, _) = layout.cursor_to_visual(cursor_char);
        layout.row_start(row)
    }

    /// Character index at end of current visual line.
    #[must_use]
    pub fn get_visual_eol(&self, buffer: &EditBuffer) -> usize {
        let layout = TextLayout::new(buffer.text(), self.width);
        let cursor_char = buffer.byte_to_char_index(buffer.cursor());
        let (row, _) = layout.cursor_to_visual(cursor_char);
        layout.row_end(row)
    }

    /// Move cursor up one visual line, preserving column.
    /// Ported from app.rs:631-650
    pub fn move_up_visual(&self, buffer: &mut EditBuffer) {
        if buffer.is_empty() {
            return;
        }
        let layout = TextLayout::new(buffer.text(), self.width);
        let cursor_char = buffer.byte_to_char_index(buffer.cursor());
        let (row, col) = layout.cursor_to_visual(cursor_char);

        if buffer.preferred_column().is_none() {
            buffer.set_preferred_column(Some(col));
        }

        if row == 0 {
            return;
        }

        let target_col = buffer.preferred_column().unwrap_or(col);
        let new_char_idx = layout.visual_to_cursor(row - 1, target_col);
        buffer.set_cursor(buffer.char_to_byte_index(new_char_idx));
    }

    /// Move cursor down one visual line, preserving column.
    /// Ported from app.rs:653-672
    pub fn move_down_visual(&self, buffer: &mut EditBuffer) {
        if buffer.is_empty() {
            return;
        }
        let layout = TextLayout::new(buffer.text(), self.width);
        let cursor_char = buffer.byte_to_char_index(buffer.cursor());
        let (row, col) = layout.cursor_to_visual(cursor_char);

        if buffer.preferred_column().is_none() {
            buffer.set_preferred_column(Some(col));
        }

        if row >= layout.total_lines - 1 {
            return;
        }

        let target_col = buffer.preferred_column().unwrap_or(col);
        let new_char_idx = layout.visual_to_cursor(row + 1, target_col);
        buffer.set_cursor(buffer.char_to_byte_index(new_char_idx));
    }

    /// Adjust `scroll_offset` to keep cursor visible within `max_visible` lines.
    pub fn ensure_cursor_visible(&mut self, buffer: &EditBuffer, max_visible: usize) {
        let visual = self.get_visual_cursor(buffer);

        if visual.row < self.scroll_offset {
            self.scroll_offset = visual.row;
        } else if visual.row >= self.scroll_offset + max_visible {
            self.scroll_offset = visual.row - max_visible + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visual_cursor_empty_buffer() {
        let view = EditorView::new(80);
        let buffer = EditBuffer::new();
        let cursor = view.get_visual_cursor(&buffer);
        assert_eq!(cursor, VisualCursor { row: 0, col: 0 });
    }

    #[test]
    fn test_visual_cursor_single_line() {
        let view = EditorView::new(80);
        let mut buffer = EditBuffer::with_text("hello");
        buffer.set_cursor(2);
        let cursor = view.get_visual_cursor(&buffer);
        assert_eq!(cursor, VisualCursor { row: 0, col: 2 });
    }

    #[test]
    fn test_visual_cursor_wrapped_line() {
        let view = EditorView::new(7);
        let mut buffer = EditBuffer::with_text("hello world");
        // "hello" on line 0, "world" on line 1
        buffer.set_cursor(8); // 'r' in "world" (after "hello w")
        let cursor = view.get_visual_cursor(&buffer);
        assert_eq!(cursor.row, 1);
        assert_eq!(cursor.col, 2); // "wo[r]ld" -> col 2
    }

    #[test]
    fn test_visual_cursor_bounded() {
        // Test that cursor col is bounded to actual line width
        let view = EditorView::new(8);
        let buffer = EditBuffer::with_text("hello     "); // trailing spaces get trimmed in wrap
        let cursor = view.get_visual_cursor(&buffer);
        // The actual wrapped text will have col bounded
        assert!(cursor.col <= 8, "col {} should be <= width 8", cursor.col);
    }

    #[test]
    fn test_visual_line_count_empty() {
        let view = EditorView::new(80);
        let buffer = EditBuffer::new();
        assert_eq!(view.get_visual_line_count(&buffer), 1);
    }

    #[test]
    fn test_visual_line_count_wrapped() {
        let view = EditorView::new(7);
        let buffer = EditBuffer::with_text("hello world");
        assert_eq!(view.get_visual_line_count(&buffer), 2);
    }

    #[test]
    fn test_move_up_visual_from_second_line() {
        let view = EditorView::new(7);
        let mut buffer = EditBuffer::with_text("hello world");
        buffer.set_cursor(8); // 'r' in "world"

        view.move_up_visual(&mut buffer);

        let cursor = view.get_visual_cursor(&buffer);
        assert_eq!(cursor.row, 0);
        // Preferred column should be preserved
        assert_eq!(cursor.col, 2); // Same col as before
    }

    #[test]
    fn test_move_up_visual_at_first_line() {
        let view = EditorView::new(80);
        let mut buffer = EditBuffer::with_text("hello");
        buffer.set_cursor(2);
        let original_cursor = buffer.cursor();

        view.move_up_visual(&mut buffer);

        assert_eq!(buffer.cursor(), original_cursor); // No change
    }

    #[test]
    fn test_move_down_visual_at_last_line() {
        let view = EditorView::new(80);
        let mut buffer = EditBuffer::with_text("hello");
        buffer.set_cursor(2);
        let original_cursor = buffer.cursor();

        view.move_down_visual(&mut buffer);

        assert_eq!(buffer.cursor(), original_cursor); // No change
    }

    #[test]
    fn test_move_down_visual_from_first_line() {
        let view = EditorView::new(7);
        let mut buffer = EditBuffer::with_text("hello world");
        buffer.set_cursor(2); // 'l' in "hello"

        view.move_down_visual(&mut buffer);

        let cursor = view.get_visual_cursor(&buffer);
        assert_eq!(cursor.row, 1);
        // Preferred column 2 preserved
        assert_eq!(cursor.col, 2);
    }

    #[test]
    fn test_ensure_cursor_visible_scroll_up() {
        let mut view = EditorView::new(80);
        view.set_scroll_offset(5); // Scrolled down
        let mut buffer = EditBuffer::with_text("a\nb\nc\nd"); // 4 lines
        buffer.set_cursor(0); // At start (row 0)

        view.ensure_cursor_visible(&buffer, 3);

        assert_eq!(view.scroll_offset(), 0); // Scrolled up to show cursor
    }

    #[test]
    fn test_ensure_cursor_visible_scroll_down() {
        let mut view = EditorView::new(80);
        view.set_scroll_offset(0);
        let mut buffer = EditBuffer::with_text("a\nb\nc\nd\ne\nf");
        buffer.set_cursor(buffer.len()); // At end (last line)

        view.ensure_cursor_visible(&buffer, 3);

        // Cursor at row 5, max_visible 3 -> scroll to show rows 3,4,5
        assert!(view.scroll_offset() >= 3);
    }

    #[test]
    fn test_visual_sol_and_eol() {
        let view = EditorView::new(7);
        let mut buffer = EditBuffer::with_text("hello world");
        // Line 0: "hello" (chars 0-5), Line 1: "world" (chars 6-11)

        buffer.set_cursor(8); // In "world"
        assert_eq!(view.get_visual_sol(&buffer), 6);
        assert_eq!(view.get_visual_eol(&buffer), 11);

        buffer.set_cursor(2); // In "hello"
        assert_eq!(view.get_visual_sol(&buffer), 0);
        assert_eq!(view.get_visual_eol(&buffer), 5);
    }

    #[test]
    fn test_preferred_column_preserved_during_navigation() {
        let view = EditorView::new(10);
        // Create text with varying line lengths
        let mut buffer = EditBuffer::with_text("abcdefghij\nab\nabcdefghij");
        // Line 0: "abcdefghij" (10 chars)
        // Line 1: "ab" (2 chars)
        // Line 2: "abcdefghij" (10 chars)

        buffer.set_cursor(5); // Col 5 on line 0
        view.move_down_visual(&mut buffer);

        // Line 1 only has 2 chars, so cursor moves to col 2 (end)
        let cursor1 = view.get_visual_cursor(&buffer);
        assert_eq!(cursor1.row, 1);
        assert_eq!(cursor1.col, 2);

        // But preferred column should still be 5
        assert_eq!(buffer.preferred_column(), Some(5));

        view.move_down_visual(&mut buffer);

        // Line 2 has 10 chars, preferred col 5 should be restored
        let cursor2 = view.get_visual_cursor(&buffer);
        assert_eq!(cursor2.row, 2);
        assert_eq!(cursor2.col, 5);
    }
}
