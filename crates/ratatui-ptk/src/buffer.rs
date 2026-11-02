/// Multi-line text with a cursor and undo/redo history.
///
/// The cursor is a byte offset into `text` and always sits on a char boundary.
#[derive(Debug, Default, Clone)]
pub struct TextBuffer {
    text: String,
    cursor: usize,
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
    // Whether the last typed char was whitespace; a run of either kind is one undo step.
    typing: Option<bool>,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replaces the whole text as one undoable step and moves the cursor to the end.
    pub fn set_text(&mut self, text: &str) {
        self.checkpoint();
        self.text = text.to_owned();
        self.cursor = self.text.len();
    }

    /// Replaces the text and cursor without recording an undo step; checkpoint first to make one.
    pub(crate) fn overwrite(&mut self, text: String, cursor: usize) {
        self.text = text;
        self.cursor = cursor;
        self.typing = None;
    }

    /// Clears the text and the undo history.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        assert!(self.text.is_char_boundary(cursor));
        self.cursor = cursor;
        self.typing = None;
    }

    /// Zero-based (row, byte column) of the cursor.
    pub fn cursor_row_col(&self) -> (usize, usize) {
        let before = &self.text[..self.cursor];
        let row = before.matches('\n').count();
        let col = self.cursor - self.line_start();
        (row, col)
    }

    pub fn line_count(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    pub fn insert_char(&mut self, c: char) {
        if self.typing != Some(c.is_whitespace()) {
            self.checkpoint();
        }
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.typing = Some(c.is_whitespace());
    }

    pub fn insert_str(&mut self, s: &str) {
        self.checkpoint();
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Replaces the text between `start` and the cursor with `s`.
    pub fn replace_before_cursor(&mut self, start: usize, s: &str) {
        self.checkpoint();
        self.text.replace_range(start..self.cursor, s);
        self.cursor = start + s.len();
    }

    pub fn delete_backward(&mut self) {
        if let Some(prev) = self.prev_boundary(self.cursor) {
            self.delete_range(prev, self.cursor);
        }
    }

    pub fn delete_forward(&mut self) {
        if let Some(next) = self.next_boundary(self.cursor) {
            self.delete_range(self.cursor, next);
        }
    }

    /// Deletes `start..end` and returns the removed text.
    pub fn delete_range(&mut self, start: usize, end: usize) -> String {
        if start == end {
            return String::new();
        }
        self.checkpoint();
        let removed = self.text[start..end].to_owned();
        self.text.replace_range(start..end, "");
        self.cursor = start;
        removed
    }

    pub fn move_left(&mut self) {
        if let Some(prev) = self.prev_boundary(self.cursor) {
            self.set_cursor(prev);
        }
    }

    pub fn move_right(&mut self) {
        if let Some(next) = self.next_boundary(self.cursor) {
            self.set_cursor(next);
        }
    }

    /// Moves one line up keeping the byte column where possible. Returns false on the first line.
    pub fn move_up(&mut self) -> bool {
        let start = self.line_start();
        if start == 0 {
            return false;
        }
        let col = self.cursor - start;
        let prev_start = self.text[..start - 1].rfind('\n').map_or(0, |i| i + 1);
        self.set_cursor(self.clamp_col(prev_start, start - 1, col));
        true
    }

    /// Moves one line down keeping the byte column where possible. Returns false on the last line.
    pub fn move_down(&mut self) -> bool {
        let end = self.line_end();
        if end == self.text.len() {
            return false;
        }
        let col = self.cursor - self.line_start();
        let next_start = end + 1;
        let next_end = self.text[next_start..]
            .find('\n')
            .map_or(self.text.len(), |i| next_start + i);
        self.set_cursor(self.clamp_col(next_start, next_end, col));
        true
    }

    pub fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    pub fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i)
    }

    /// Start of the word before the cursor, skipping whitespace and punctuation first.
    pub fn word_start_before(&self) -> usize {
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end_matches(|c: char| !is_word_char(c));
        trimmed
            .rfind(|c: char| !is_word_char(c))
            .map_or(0, |i| i + trimmed[i..].chars().next().unwrap().len_utf8())
    }

    /// End of the word after the cursor, skipping whitespace and punctuation first.
    pub fn word_end_after(&self) -> usize {
        let after = &self.text[self.cursor..];
        let skip = after.len() - after.trim_start_matches(|c: char| !is_word_char(c)).len();
        let rest = &after[skip..];
        self.cursor + skip + rest.find(|c: char| !is_word_char(c)).unwrap_or(rest.len())
    }

    pub fn undo(&mut self) {
        if let Some(state) = self.undo.pop() {
            self.redo
                .push((std::mem::take(&mut self.text), self.cursor));
            (self.text, self.cursor) = state;
            self.typing = None;
        }
    }

    pub fn redo(&mut self) {
        if let Some(state) = self.redo.pop() {
            self.undo
                .push((std::mem::take(&mut self.text), self.cursor));
            (self.text, self.cursor) = state;
            self.typing = None;
        }
    }

    pub(crate) fn checkpoint(&mut self) {
        self.undo.push((self.text.clone(), self.cursor));
        self.redo.clear();
        self.typing = None;
    }

    fn clamp_col(&self, start: usize, end: usize, col: usize) -> usize {
        let mut pos = (start + col).min(end);
        while !self.text.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    fn prev_boundary(&self, pos: usize) -> Option<usize> {
        self.text[..pos].char_indices().next_back().map(|(i, _)| i)
    }

    fn next_boundary(&self, pos: usize) -> Option<usize> {
        self.text[pos..].chars().next().map(|c| pos + c.len_utf8())
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> TextBuffer {
        let mut b = TextBuffer::new();
        s.chars().for_each(|c| b.insert_char(c));
        b
    }

    #[test]
    fn typing_groups_undo_by_word() {
        let mut b = typed("select 1");
        b.undo();
        assert_eq!(b.text(), "select ");
        b.undo();
        assert_eq!(b.text(), "select");
        b.undo();
        assert_eq!(b.text(), "");
        b.redo();
        b.redo();
        assert_eq!(b.text(), "select ");
    }

    #[test]
    fn vertical_moves_clamp_column_and_respect_utf8() {
        let mut b = typed("abcdef\nあい\nxyz");
        b.set_cursor(5); // "abcde|f"
        assert!(b.move_down());
        assert_eq!(b.cursor_row_col(), (1, 3)); // after "あ"
        assert!(b.move_down());
        assert_eq!(b.cursor_row_col(), (2, 3));
        assert!(!b.move_down());
        assert!(b.move_up());
        assert!(b.move_up());
        assert!(!b.move_up());
    }

    #[test]
    fn word_boundaries() {
        let mut b = typed("select foo_bar, baz");
        b.set_cursor(15); // after ","
        assert_eq!(b.word_start_before(), 7);
        b.set_cursor(7);
        assert_eq!(b.word_end_after(), 14);
    }

    #[test]
    fn delete_handles_multibyte() {
        let mut b = typed("aあ");
        b.delete_backward();
        assert_eq!(b.text(), "a");
        b.set_cursor(0);
        b.delete_forward();
        assert_eq!(b.text(), "");
    }
}
