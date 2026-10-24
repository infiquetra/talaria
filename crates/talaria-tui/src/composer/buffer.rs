//! Grapheme caret, words, and the kill buffer.

#![allow(dead_code)]

/// The draft text and the caret.
#[derive(Debug, Default)]
pub struct Draft {
    text: String,
    caret: usize,
    preferred_col: usize,
    kill: String,
}

impl Draft {
    /// An empty draft.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The draft text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Byte offset of the caret.
    #[must_use]
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Insert filtered text at the caret.
    pub fn insert(&mut self, text: &str) {
        let _ = (
            text,
            &mut self.text,
            &mut self.caret,
            &mut self.preferred_col,
        );
        todo!()
    }

    /// Delete the cluster before the caret.
    pub fn backspace(&mut self) {
        todo!()
    }

    /// Move one cluster left.
    pub fn move_left(&mut self) {
        todo!()
    }

    /// Move one cluster right.
    pub fn move_right(&mut self) {
        todo!()
    }

    /// Move to the previous visual row.
    pub fn move_up(&mut self) {
        let _ = self.preferred_col;
        todo!()
    }

    /// Move to the next visual row.
    pub fn move_down(&mut self) {
        let _ = self.preferred_col;
        todo!()
    }

    /// Move to the start of the previous word.
    pub fn word_left(&mut self) {
        todo!()
    }

    /// Move to the start of the next word.
    pub fn word_right(&mut self) {
        todo!()
    }

    /// Delete the word before the caret into the kill buffer.
    pub fn delete_word_backward(&mut self) {
        let _ = &mut self.kill;
        todo!()
    }

    /// Delete through the end of the logical line into the kill buffer.
    pub fn kill_to_line_end(&mut self) {
        let _ = &mut self.kill;
        todo!()
    }

    /// Delete back to the start of the logical line into the kill buffer.
    pub fn kill_to_line_start(&mut self) {
        let _ = &mut self.kill;
        todo!()
    }

    /// Insert the kill buffer at the caret.
    pub fn yank(&mut self) {
        let _ = &self.kill;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::Draft;

    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

    #[test]
    fn caret_moves_by_cluster() {
        let mut draft = Draft::new();
        draft.insert(FAMILY);
        draft.insert("a");
        assert_eq!(draft.text(), format!("{FAMILY}a"));
        draft.move_left();
        assert_eq!(draft.caret(), FAMILY.len());
        draft.backspace();
        assert_eq!(draft.text(), "a");
        assert_eq!(draft.caret(), 0);
        draft.backspace();
        assert_eq!(draft.text(), "a");
        assert_eq!(draft.caret(), 0);

        let mut draft = Draft::new();
        draft.insert("e");
        draft.insert("\u{0301}");
        assert_eq!(draft.text(), "e\u{0301}");
        draft.move_left();
        assert_eq!(draft.caret(), 0);
        draft.move_right();
        assert_eq!(draft.caret(), "e\u{0301}".len());
        draft.backspace();
        assert_eq!(draft.text(), "");

        draft.insert("\u{0301}");
        assert_eq!(draft.text(), "");
        draft.insert("\u{200D}");
        assert_eq!(draft.text(), "");
        draft.insert("\u{200B}");
        assert_eq!(draft.text(), "");

        draft.insert("你");
        draft.insert("a");
        assert_eq!(draft.caret(), "你a".len());
        draft.move_left();
        assert_eq!(draft.caret(), "你".len());
        draft.move_left();
        assert_eq!(draft.caret(), 0);
        draft.move_right();
        draft.backspace();
        assert_eq!(draft.text(), "a");
        draft.insert("你");
        draft.backspace();
        assert_eq!(draft.text(), "a");
    }

    #[test]
    fn word_kill_yank_and_column() {
        let mut draft = Draft::new();
        draft.insert("one two_three");
        draft.word_left();
        assert_eq!(draft.caret(), "one ".len());
        draft.word_left();
        assert_eq!(draft.caret(), 0);
        draft.word_right();
        assert_eq!(draft.caret(), "one".len());
        draft.word_right();
        assert_eq!(draft.caret(), "one two_three".len());

        draft.delete_word_backward();
        assert_eq!(draft.text(), "one ");
        draft.yank();
        assert_eq!(draft.text(), "one two_three");

        let mut draft = Draft::new();
        draft.insert("ab\ncd");
        assert_eq!(draft.caret(), "ab\ncd".len());
        draft.kill_to_line_start();
        assert_eq!(draft.text(), "ab\n");
        assert_eq!(draft.caret(), "ab\n".len());
        draft.kill_to_line_end();
        assert_eq!(draft.text(), "ab\n");
        draft.move_left();
        assert_eq!(draft.caret(), "ab".len());
        draft.kill_to_line_end();
        assert_eq!(draft.text(), "ab");
        draft.insert("\ncd");
        let second_line = "ab\n".len();
        while draft.caret() > second_line {
            draft.move_left();
        }
        assert_eq!(draft.caret(), second_line);
        draft.kill_to_line_start();
        assert_eq!(draft.text(), "abcd");

        let mut draft = Draft::new();
        draft.insert("keep");
        draft.delete_word_backward();
        assert_eq!(draft.text(), "");
        draft.delete_word_backward();
        draft.yank();
        assert_eq!(draft.text(), "keep");

        let mut draft = Draft::new();
        draft.insert("aaaa\nbb\ncccc\ndddd");
        draft.move_up();
        assert_eq!(draft.caret(), "aaaa\nbb\ncccc".len());
        draft.move_up();
        assert_eq!(draft.caret(), "aaaa\nbb".len());
        draft.move_up();
        assert_eq!(draft.caret(), "aaaa".len());
        draft.move_down();
        assert_eq!(draft.caret(), "aaaa\nbb".len());
        draft.move_down();
        assert_eq!(draft.caret(), "aaaa\nbb\ncccc".len());
    }
}
