//! A pure single-line text editor model for inspector fields.
//!
//! The model owns the text, the caret and the selection in **character**
//! indices (never bytes), so multi-byte input is safe. Keys are mapped by the
//! caller; this module only performs the edits, which keeps it testable
//! without a window.

/// One line being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    text: String,
    /// Caret position in characters, `0..=len`.
    caret: usize,
    /// Selection anchor; the selection spans `anchor..caret` (or the reverse).
    anchor: usize,
}

impl TextEdit {
    /// Starts editing `text` with the caret at the end and nothing selected.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let caret = text.chars().count();
        Self {
            text,
            caret,
            anchor: caret,
        }
    }

    /// Starts editing `text` with everything selected (the usual behaviour of
    /// clicking a numeric field).
    #[must_use]
    pub fn select_all(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            caret: text.chars().count(),
            text,
            anchor: 0,
        }
    }

    /// The current text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The caret position in characters.
    #[must_use]
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selection as `(start, end)` character indices, `start <= end`.
    #[must_use]
    pub fn selection(&self) -> (usize, usize) {
        if self.caret < self.anchor {
            (self.caret, self.anchor)
        } else {
            (self.anchor, self.caret)
        }
    }

    /// Whether anything is selected.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    /// Replaces the whole text and puts the caret at the end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.caret = self.text.chars().count();
        self.anchor = self.caret;
    }

    /// Selects everything.
    pub fn select_all_in_place(&mut self) {
        self.anchor = 0;
        self.caret = self.text.chars().count();
    }

    /// Replaces the selection (or inserts at the caret) with `insert`.
    pub fn insert(&mut self, insert: &str) {
        let (start, end) = self.selection();
        let start_byte = self.byte_of(start);
        let end_byte = self.byte_of(end);
        let inserted = insert.chars().count();
        self.text.replace_range(start_byte..end_byte, insert);
        self.caret = start + inserted;
        self.anchor = self.caret;
    }

    /// Inserts one character.
    pub fn insert_char(&mut self, c: char) {
        let mut buffer = [0_u8; 4];
        self.insert(c.encode_utf8(&mut buffer));
    }

    /// Deletes the selection, or the character before the caret.
    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.insert("");
            return;
        }
        if self.caret == 0 {
            return;
        }
        let before = self.byte_of(self.caret - 1);
        let at = self.byte_of(self.caret);
        self.text.replace_range(before..at, "");
        self.caret -= 1;
        self.anchor = self.caret;
    }

    /// Deletes the selection, or the character after the caret.
    pub fn delete(&mut self) {
        if self.has_selection() {
            self.insert("");
            return;
        }
        let len = self.text.chars().count();
        if self.caret >= len {
            return;
        }
        let at = self.byte_of(self.caret);
        let after = self.byte_of(self.caret + 1);
        self.text.replace_range(at..after, "");
        self.anchor = self.caret;
    }

    /// Moves the caret left, extending the selection when `extend`.
    pub fn left(&mut self, extend: bool) {
        self.caret = self.caret.saturating_sub(1);
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Moves the caret right, extending the selection when `extend`.
    pub fn right(&mut self, extend: bool) {
        let len = self.text.chars().count();
        self.caret = (self.caret + 1).min(len);
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Moves the caret to the start, extending the selection when `extend`.
    pub fn home(&mut self, extend: bool) {
        self.caret = 0;
        if !extend {
            self.anchor = 0;
        }
    }

    /// Moves the caret to the end, extending the selection when `extend`.
    pub fn end(&mut self, extend: bool) {
        self.caret = self.text.chars().count();
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Byte offset of character index `index` (clamped to the text length).
    fn byte_of(&self, index: usize) -> usize {
        self.text
            .char_indices()
            .nth(index)
            .map_or(self.text.len(), |(byte, _)| byte)
    }
}

/// The key an editor field consumes, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey {
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Ctrl+A.
    SelectAll,
    /// Enter (commit).
    Commit,
    /// Escape (cancel).
    Cancel,
    /// Tab (next field).
    Next,
    /// Shift+Tab (previous field).
    Previous,
}

impl TextEdit {
    /// Applies a non-character key. Returns `true` when the field still has
    /// focus (false for commit and cancel, which the caller handles).
    pub fn key(&mut self, key: EditKey, shift: bool) -> bool {
        match key {
            EditKey::Backspace => self.backspace(),
            EditKey::Delete => self.delete(),
            EditKey::Left => self.left(shift),
            EditKey::Right => self.right(shift),
            EditKey::Home => self.home(shift),
            EditKey::End => self.end(shift),
            EditKey::SelectAll => self.select_all_in_place(),
            EditKey::Commit | EditKey::Cancel | EditKey::Next | EditKey::Previous => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_replaces_the_selection_and_moves_the_caret() {
        let mut edit = TextEdit::select_all("960");
        edit.insert_char('4');
        assert_eq!(edit.text(), "4");
        assert_eq!(edit.caret(), 1);
        assert!(!edit.has_selection());
        edit.insert_char('2');
        assert_eq!(edit.text(), "42");
        edit.home(false);
        edit.insert_char('1');
        assert_eq!(edit.text(), "142");
        assert_eq!(edit.caret(), 1);
    }

    #[test]
    fn backspace_and_delete_respect_the_selection() {
        let mut edit = TextEdit::new("abcd");
        edit.backspace();
        assert_eq!(edit.text(), "abc");
        edit.home(false);
        edit.delete();
        assert_eq!(edit.text(), "bc");
        edit.select_all_in_place();
        edit.backspace();
        assert_eq!(edit.text(), "");
        assert_eq!(edit.caret(), 0);
        // Nothing to delete at the edges.
        edit.backspace();
        edit.delete();
        assert_eq!(edit.text(), "");
    }

    #[test]
    fn arrows_extend_the_selection_only_with_shift() {
        let mut edit = TextEdit::new("abcd");
        edit.left(false);
        edit.left(false);
        assert_eq!(edit.caret(), 2);
        assert!(!edit.has_selection());
        edit.left(true);
        assert_eq!(edit.selection(), (1, 2));
        edit.right(true);
        edit.right(true);
        assert_eq!(edit.selection(), (2, 3));
        edit.insert("X");
        assert_eq!(edit.text(), "abXd");
    }

    #[test]
    fn home_end_and_select_all() {
        let mut edit = TextEdit::new("hello");
        edit.home(false);
        assert_eq!(edit.caret(), 0);
        edit.end(true);
        assert_eq!(edit.selection(), (0, 5));
        edit.key(EditKey::SelectAll, false);
        assert_eq!(edit.selection(), (0, 5));
    }

    #[test]
    fn multi_byte_text_is_edited_by_character() {
        let mut edit = TextEdit::new("ñandú");
        edit.left(false);
        edit.backspace();
        assert_eq!(edit.text(), "ñanú");
        edit.home(false);
        edit.right(false);
        edit.insert("ó");
        assert_eq!(edit.text(), "ñóanú");
    }

    #[test]
    fn commit_and_cancel_leave_the_field() {
        let mut edit = TextEdit::new("1");
        assert!(!edit.key(EditKey::Commit, false));
        assert!(!edit.key(EditKey::Cancel, false));
        assert!(edit.key(EditKey::Left, false));
        assert!(!edit.key(EditKey::Next, false));
        assert!(!edit.key(EditKey::Previous, false));
    }
}
