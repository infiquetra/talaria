//! Literal search over one session's cells. Issue 222. The search itself is not written yet.

use ratatui::text::Line;
use talaria_core::session::Cell;

use super::render::PagerTheme;
use crate::history::CellLines;

/// Search text of final cells, kept by the application across openings of the transcript.
#[derive(Debug, Default)]
pub struct SearchIndex {
    built: u64,
}

impl SearchIndex {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self { built: 0 }
    }

    /// Drop every stored cell.
    pub fn clear(&mut self) {
        self.built = 0;
    }

    /// How many final-cell entries have been built since [`Self::new`].
    #[doc(hidden)]
    #[must_use]
    pub fn build_count(&self) -> u64 {
        self.built
    }
}

/// The text one cell contributes to search, with labels left out.
#[must_use]
pub fn cell_search_text(_cell: &Cell, _other: &dyn CellLines) -> String {
    todo!()
}

/// One source character, folded when the lowercase form is also one character.
#[must_use]
pub fn fold(_c: char) -> char {
    todo!()
}

/// A literal query. An uppercase letter makes the match case-sensitive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// The query text, never containing a newline.
    pub text: String,
    /// True when `text` contains an uppercase letter.
    pub case_sensitive: bool,
}

impl Query {
    /// Smart case: uppercase in `text` turns case sensitivity on.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            case_sensitive: text.chars().any(char::is_uppercase),
        }
    }
}

/// One match. Offsets count characters in the cell's search text, never bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    /// Index into the cell slice.
    pub cell_index: usize,
    /// Zero-based match inside that cell.
    pub ordinal_in_cell: usize,
    /// Character index where the match starts.
    pub char_start: usize,
    /// Character length of the match.
    pub char_len: usize,
}

/// Every non-overlapping match, ordered by cell then character offset.
#[must_use]
pub fn find_all(
    _cells: &[Cell],
    _epoch: u64,
    _index: &mut SearchIndex,
    _query: &Query,
    _other: &dyn CellLines,
) -> Vec<Match> {
    todo!()
}

/// Copy `line`, painting matches. The text is unchanged.
#[must_use]
pub fn highlight_line(
    _line: &Line<'static>,
    _query: &Query,
    _current: Option<usize>,
    _theme: &PagerTheme,
) -> Line<'static> {
    todo!()
}
