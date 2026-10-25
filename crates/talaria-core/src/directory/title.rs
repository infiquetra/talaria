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
pub fn clean_title(raw: &str) -> Result<String, TitleError> {
    let mut cleaned = String::with_capacity(raw.len());
    for character in raw.chars() {
        if is_removed(character) {
            continue;
        }
        if character.is_whitespace() {
            // Every run of whitespace becomes one space.
            if !cleaned.ends_with(' ') {
                cleaned.push(' ');
            }
            continue;
        }
        cleaned.push(character);
    }
    let cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() {
        return Err(TitleError::Empty);
    }
    let chars = cleaned.chars().count();
    if chars > MAX_TITLE_CHARS {
        return Err(TitleError::TooLong { chars });
    }
    Ok(cleaned)
}

/// Whether Hermes's cleanup removes this character: the control ranges and the
/// invisible ranges.
fn is_removed(character: char) -> bool {
    matches!(
        character,
        '\u{0}'..='\u{8}'
            | '\u{b}'
            | '\u{c}'
            | '\u{e}'..='\u{1f}'
            | '\u{7f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2069}'
            | '\u{feff}'
            | '\u{fffc}'
            | '\u{fff9}'..='\u{fffb}'
    )
}
