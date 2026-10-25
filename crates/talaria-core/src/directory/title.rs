//! Session titles: Hermes's cleanup rule (#231).

/// Titles are at most this many code points.
pub const MAX_TITLE_CHARS: usize = 100;

/// Why a title was refused before sending.
#[derive(Debug, PartialEq, Eq)]
pub enum TitleError {
    Empty,
    TooLong { chars: usize },
}

/// Apply Hermes's cleanup: remove controls and invisibles, collapse whitespace,
/// trim. Empty and too-long titles are refused.
pub fn clean_title(_raw: &str) -> Result<String, TitleError> {
    unimplemented!("#231")
}
