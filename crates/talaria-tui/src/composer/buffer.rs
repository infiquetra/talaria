//! Grapheme caret, words, and the kill buffer.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::wrap::{Row, cluster_width, wrap};

/// Characters dropped when they stand alone, whatever the width table says.
fn invisible_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'
            | '\u{200C}'
            | '\u{200D}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{2060}'
            | '\u{FEFF}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

/// A cluster that would draw nothing: width 0, or only characters of the list above.
pub(crate) fn invisible_cluster(cluster: &str) -> bool {
    if cluster == "\n" || cluster == "\t" {
        return false;
    }
    UnicodeWidthStr::width(cluster) == 0 || cluster.chars().all(invisible_char)
}

/// A control character the buffer may not hold. Line break and tab are allowed.
pub(crate) fn forbidden_control(c: char) -> bool {
    c.is_control() && c != '\n' && c != '\t'
}

/// Pasted or loaded text, cleaned: line breaks normalised, controls removed, and each cluster
/// that would draw nothing dropped, as if the text went into an empty draft.
pub fn clean_text(text: &str) -> String {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    let kept: String = normalised
        .chars()
        .filter(|c| !forbidden_control(*c))
        .collect();
    if kept
        .graphemes(true)
        .all(|cluster| !invisible_cluster(cluster))
    {
        return kept;
    }
    kept.graphemes(true)
        .filter(|cluster| !invisible_cluster(cluster))
        .collect()
}

/// The boundary before `at`, or 0.
pub(crate) fn prev_boundary(text: &str, at: usize) -> usize {
    text[..at]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(index, _)| index)
}

/// The boundary after `at`, or the end.
pub(crate) fn next_boundary(text: &str, at: usize) -> usize {
    text[at..]
        .graphemes(true)
        .next()
        .map_or(text.len(), |cluster| at + cluster.len())
}

/// The first boundary at or after `at`.
fn snap(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .find(|index| *index >= at)
        .unwrap_or(text.len())
}

fn is_word(cluster: &str) -> bool {
    cluster
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Which row holds the caret. A caret at the end of a soft-wrapped row belongs to the next.
pub(crate) fn row_of(rows: &[Row], caret: usize) -> usize {
    let last = rows.len().saturating_sub(1);
    for (index, row) in rows.iter().enumerate() {
        if caret < row.start {
            return index.saturating_sub(1);
        }
        if caret < row.end || (caret == row.end && (row.hard_break_after || index == last)) {
            return index;
        }
    }
    last
}

/// Display column of `at` within its row.
fn column_in(text: &str, row: &Row, at: usize) -> usize {
    text[row.start..at.clamp(row.start, row.end)]
        .graphemes(true)
        .map(cluster_width)
        .sum()
}

/// The caret position in `row` closest to `column` without passing it.
fn position_at(text: &str, row: &Row, column: usize, last_row: bool) -> usize {
    let mut used = 0;
    let mut at = row.start;
    let slice = &text[row.start..row.end];
    let clusters: Vec<(usize, &str)> = slice.grapheme_indices(true).collect();
    for (offset, cluster) in &clusters {
        let width = cluster_width(cluster);
        if used + width > column {
            return row.start + offset;
        }
        used += width;
        at = row.start + offset + cluster.len();
    }
    // The end of a soft-wrapped row is drawn on the next row; stay before its last cluster.
    if !row.hard_break_after
        && !last_row
        && at == row.end
        && let Some((offset, _)) = clusters.last()
    {
        return row.start + offset;
    }
    at
}

/// The draft text and the caret.
#[derive(Debug, Default)]
pub struct Draft {
    text: String,
    caret: usize,
    preferred_col: Option<usize>,
    kill: String,
    revision: u64,
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

    /// Changes each time the text changes.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Insert typed text at the caret. Controls are removed, and a cluster of the inserted
    /// text that would draw nothing on its own is removed again; a mark or joiner that attaches
    /// to the cluster before it stays.
    pub fn insert(&mut self, text: &str) {
        let kept: String = text.chars().filter(|c| !forbidden_control(*c)).collect();
        if kept.is_empty() {
            return;
        }
        let start = self.caret;
        self.text.insert_str(start, &kept);
        let mut end = start + kept.len();
        let mut at = start;
        while at < end {
            let next = next_boundary(&self.text, at);
            let starts_cluster = at == 0 || prev_boundary(&self.text, next) == at;
            if starts_cluster && next <= end && invisible_cluster(&self.text[at..next]) {
                self.text.replace_range(at..next, "");
                end -= next - at;
            } else {
                at = next;
            }
        }
        self.caret = snap(&self.text, end);
        self.edited();
    }

    /// Insert text that is already clean, as one edit.
    pub(crate) fn insert_clean(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.text.insert_str(self.caret, text);
        self.caret = snap(&self.text, self.caret + text.len());
        self.edited();
    }

    /// Replace the whole text; the caret goes to the end. The kill buffer is kept.
    pub(crate) fn replace(&mut self, text: String) {
        self.text = text;
        self.caret = self.text.len();
        self.edited();
    }

    /// Replace a valid UTF-8 byte range and keep the caret on a grapheme boundary.
    /// Invalid ranges leave the draft unchanged.
    pub(crate) fn replace_range(&mut self, range: std::ops::Range<usize>, text: &str) -> bool {
        if self.text.get(range.clone()).is_none() {
            return false;
        }
        let end = range.start + text.len();
        self.text.replace_range(range, text);
        self.caret = snap(&self.text, end);
        self.edited();
        true
    }

    /// Put the caret at a byte offset, on a boundary.
    pub(crate) fn set_caret(&mut self, at: usize) {
        self.caret = snap(&self.text, at);
        self.preferred_col = None;
    }

    fn edited(&mut self) {
        self.revision += 1;
        self.preferred_col = None;
    }

    pub(crate) fn reset_preferred_column(&mut self) {
        self.preferred_col = None;
    }

    fn remove(&mut self, from: usize, to: usize) -> String {
        let removed = self.text[from..to].to_string();
        self.text.replace_range(from..to, "");
        self.caret = snap(&self.text, from);
        self.edited();
        removed
    }

    fn kill_span(&mut self, from: usize, to: usize) {
        if from < to {
            self.kill = self.remove(from, to);
        }
    }

    /// Delete the cluster before the caret.
    pub fn backspace(&mut self) {
        if self.caret > 0 {
            let from = prev_boundary(&self.text, self.caret);
            self.remove(from, self.caret);
        }
    }

    /// Delete the cluster after the caret.
    pub fn delete_forward(&mut self) {
        if self.caret < self.text.len() {
            let to = next_boundary(&self.text, self.caret);
            self.remove(self.caret, to);
        }
    }

    /// Move one cluster left.
    pub fn move_left(&mut self) {
        if self.caret > 0 {
            self.set_caret(prev_boundary(&self.text, self.caret));
        }
    }

    /// Move one cluster right.
    pub fn move_right(&mut self) {
        if self.caret < self.text.len() {
            self.set_caret(next_boundary(&self.text, self.caret));
        }
    }

    /// Move to the previous logical line, keeping the preferred column.
    #[allow(
        dead_code,
        reason = "the composer moves by visual rows; tests use logical lines"
    )]
    pub fn move_up(&mut self) {
        let rows = logical_rows(&self.text);
        self.move_up_in(&rows);
    }

    /// Move to the next logical line, keeping the preferred column.
    #[allow(
        dead_code,
        reason = "the composer moves by visual rows; tests use logical lines"
    )]
    pub fn move_down(&mut self) {
        let rows = logical_rows(&self.text);
        self.move_down_in(&rows);
    }

    /// Move to the previous visual row of `rows`. False on the first row.
    pub(crate) fn move_up_in(&mut self, rows: &[Row]) -> bool {
        let row = row_of(rows, self.caret);
        if row == 0 || rows.is_empty() {
            return false;
        }
        let last = row - 1 == rows.len() - 1;
        let column = *self
            .preferred_col
            .get_or_insert_with(|| column_in(&self.text, &rows[row], self.caret));
        self.caret = position_at(&self.text, &rows[row - 1], column, last);
        true
    }

    /// Move to the next visual row of `rows`. False on the last row.
    pub(crate) fn move_down_in(&mut self, rows: &[Row]) -> bool {
        let row = row_of(rows, self.caret);
        if rows.is_empty() || row + 1 >= rows.len() {
            return false;
        }
        let last = row + 1 == rows.len() - 1;
        let column = *self
            .preferred_col
            .get_or_insert_with(|| column_in(&self.text, &rows[row], self.caret));
        self.caret = position_at(&self.text, &rows[row + 1], column, last);
        true
    }

    /// Start of the logical line.
    pub(crate) fn line_start(&mut self) {
        let start = self.text[..self.caret].rfind('\n').map_or(0, |at| at + 1);
        self.set_caret(start);
    }

    /// End of the logical line.
    pub(crate) fn line_end(&mut self) {
        let end = self.text[self.caret..]
            .find('\n')
            .map_or(self.text.len(), |at| self.caret + at);
        self.set_caret(end);
    }

    /// Move to the start of the previous word.
    pub fn word_left(&mut self) {
        let start = self.word_start_before(self.caret);
        self.set_caret(start);
    }

    /// Move to the end of the next word.
    pub fn word_right(&mut self) {
        let mut at = self.caret;
        let mut seen_word = false;
        for cluster in self.text[self.caret..].graphemes(true) {
            let word = is_word(cluster);
            if seen_word && !word {
                break;
            }
            seen_word |= word;
            at += cluster.len();
        }
        self.set_caret(at);
    }

    fn word_start_before(&self, from: usize) -> usize {
        let mut at = from;
        let mut seen_word = false;
        for (index, cluster) in self.text[..from].grapheme_indices(true).rev() {
            let word = is_word(cluster);
            if seen_word && !word {
                break;
            }
            seen_word |= word;
            at = index;
        }
        at
    }

    /// Delete the word before the caret into the kill buffer.
    pub fn delete_word_backward(&mut self) {
        let start = self.word_start_before(self.caret);
        self.kill_span(start, self.caret);
    }

    /// Delete through the end of the logical line into the kill buffer. At the end of a line
    /// it deletes the line break.
    pub fn kill_to_line_end(&mut self) {
        let rest = &self.text[self.caret..];
        let end = match rest.find('\n') {
            Some(0) => self.caret + 1,
            Some(at) => self.caret + at,
            None => self.text.len(),
        };
        self.kill_span(self.caret, end);
    }

    /// Delete back to the start of the logical line into the kill buffer. At the start of a
    /// line it deletes the line break before it.
    pub fn kill_to_line_start(&mut self) {
        let head = &self.text[..self.caret];
        let start = match head.rfind('\n') {
            Some(at) if at + 1 == self.caret => at,
            Some(at) => at + 1,
            None => 0,
        };
        self.kill_span(start, self.caret);
    }

    /// Insert the kill buffer at the caret.
    pub fn yank(&mut self) {
        let kill = self.kill.clone();
        self.insert_clean(&kill);
    }
}

/// One row per logical line: used before the composer has been drawn.
pub(crate) fn logical_rows(text: &str) -> Vec<Row> {
    wrap(text, usize::MAX)
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
