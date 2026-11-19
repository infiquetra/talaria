//! Text safety shared by domain state and terminal presentation.

use std::borrow::Cow;
use std::cmp::Ordering;

/// Inclusive ranges of characters [`sanitize`] replaces. Sorted and non-overlapping.
///
/// The set is C0 and C1 controls, every Unicode format character (`Cf`), U+2028 and
/// U+2029, the tag block, every variation selector except U+FE0E and U+FE0F, the
/// default-ignorable marks U+034F and U+17B4..=U+17B5, the Hangul fillers, and the
/// blank-width spaces. The integration test pins the exact codepoints.
const NEUTRALIZED: &[(u32, u32)] = &[
    (0x0000, 0x001F),
    (0x007F, 0x00A0),
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x2000, 0x200F),
    (0x2028, 0x202F),
    (0x205F, 0x2064),
    (0x2066, 0x206F),
    (0x2800, 0x2800),
    (0x3000, 0x3000),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0D),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE007F),
    (0xE0100, 0xE01EF),
];

/// Whether [`sanitize`] replaces this character.
///
/// Alert fields, fleet titles, rewind columns and the composer consult this same set.
#[must_use]
pub fn is_neutralized(c: char) -> bool {
    let code = u32::from(c);
    NEUTRALIZED
        .binary_search_by(|&(start, end)| {
            if code > end {
                Ordering::Less
            } else if code < start {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

/// What one character is written as, when it is not written as itself.
enum StandIn {
    Char(char),
    Spaces,
    Code(u32),
}

fn stand_in(c: char) -> Option<StandIn> {
    if !is_neutralized(c) {
        return None;
    }
    let code = u32::from(c);
    match code {
        0x09 => Some(StandIn::Spaces),
        0x00..=0x1f => char::from_u32(0x2400 + code).map(StandIn::Char),
        0x7f => Some(StandIn::Char('\u{2421}')),
        _ => Some(StandIn::Code(code)),
    }
}

/// Which layout characters [`sanitize_with_options`] preserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SanitizeOptions {
    /// Leave U+000A and U+0009 unchanged.
    pub keep_layout: bool,
    /// Leave U+2028 and U+2029 unchanged so a later scan can still see them as separators.
    pub keep_separators: bool,
}

impl SanitizeOptions {
    /// Replace every character in the shared neutralized set.
    pub const DISPLAY: Self = Self {
        keep_layout: false,
        keep_separators: false,
    };

    /// Apply the display policy to command-line output.
    pub const CLI: Self = Self::DISPLAY;

    /// Keep newline and tab.
    pub const LAYOUT: Self = Self {
        keep_layout: true,
        keep_separators: false,
    };
}

/// How an unsafe character is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replacement {
    /// Control pictures and `‹U+XXXX›` brackets.
    StandIn,
    /// One visible character.
    Char(char),
    /// Remove the character.
    Drop,
}

fn classified(c: char, options: SanitizeOptions) -> Option<StandIn> {
    if options.keep_layout && matches!(c, '\n' | '\t') {
        return None;
    }
    if options.keep_separators && matches!(c, '\u{2028}' | '\u{2029}') {
        return None;
    }
    stand_in(c)
}

fn push_replacement(out: &mut String, found: StandIn, replacement: Replacement) {
    match replacement {
        Replacement::Drop => {}
        Replacement::Char(picture) => out.push(picture),
        Replacement::StandIn => match found {
            StandIn::Char(picture) => out.push(picture),
            StandIn::Spaces => out.push_str("    "),
            StandIn::Code(code) => out.push_str(&format!("\u{2039}U+{code:04X}\u{203a}")),
        },
    }
}

/// Replace controls and other neutralized characters with visible stand-ins. Clean text is borrowed.
pub fn sanitize(text: &str) -> Cow<'_, str> {
    sanitize_with_options(text, SanitizeOptions::DISPLAY, Replacement::StandIn)
}

/// Replace characters from the shared neutralized set. Clean text is borrowed.
///
/// The options can preserve newline and tab or the Unicode line and paragraph separators.
pub fn sanitize_with_options<'a>(
    text: &'a str,
    options: SanitizeOptions,
    replacement: Replacement,
) -> Cow<'a, str> {
    let Some((at, _)) = text
        .char_indices()
        .find(|(_, c)| classified(*c, options).is_some())
    else {
        return Cow::Borrowed(text);
    };
    let mut clean = String::with_capacity(text.len() + 8);
    clean.push_str(&text[..at]);
    for c in text[at..].chars() {
        match classified(c, options) {
            None => clean.push(c),
            Some(found) => push_replacement(&mut clean, found, replacement),
        }
    }
    Cow::Owned(clean)
}
