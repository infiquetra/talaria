//! Text safety shared by domain state and terminal presentation.

use std::borrow::Cow;

/// What one character is written as, when it is not written as itself.
enum StandIn {
    Char(char),
    Spaces,
    Code(u32),
}

/// Which extra classes [`sanitize_with_options`] applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SanitizeOptions {
    /// Leave U+000A and U+0009 unchanged.
    pub keep_layout: bool,
    /// Also replace U+200B, U+200C, U+200D, U+2060, and U+FEFF.
    ///
    /// The display sanitizer leaves these alone so a zero-width joiner inside
    /// an emoji stays part of that emoji.
    pub zero_width: bool,
}

impl SanitizeOptions {
    /// Control pictures for controls. Zero-width characters stay.
    pub const DISPLAY: Self = Self {
        keep_layout: false,
        zero_width: false,
    };

    /// Replace newline and tab as well, and replace zero-width characters.
    pub const CLI: Self = Self {
        keep_layout: false,
        zero_width: true,
    };

    /// Keep newline and tab, and replace zero-width characters.
    pub const LAYOUT: Self = Self {
        keep_layout: true,
        zero_width: true,
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

fn stand_in(c: char) -> Option<StandIn> {
    let code = u32::from(c);
    match code {
        0x09 => Some(StandIn::Spaces),
        0x00..=0x1f => char::from_u32(0x2400 + code).map(StandIn::Char),
        0x7f => Some(StandIn::Char('\u{2421}')),
        0x80..=0x9f
        | 0x061c
        | 0x200e
        | 0x200f
        | 0x202a..=0x202e
        | 0x2066..=0x2069
        | 0x2028
        | 0x2029
        | 0xe0000..=0xe007f => Some(StandIn::Code(code)),
        _ => None,
    }
}

fn zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
    )
}

fn classified(c: char, options: SanitizeOptions) -> Option<StandIn> {
    if options.keep_layout && matches!(c, '\n' | '\t') {
        return None;
    }
    if let Some(found) = stand_in(c) {
        return Some(found);
    }
    if options.zero_width && zero_width(c) {
        return Some(StandIn::Code(u32::from(c)));
    }
    None
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

/// Replace controls and direction marks with visible stand-ins. Clean text is borrowed.
pub fn sanitize(text: &str) -> Cow<'_, str> {
    sanitize_with_options(text, SanitizeOptions::DISPLAY, Replacement::StandIn)
}

/// Replace the shared unsafe set. Clean text is borrowed.
///
/// `options.keep_layout` leaves newline and tab in place. `options.zero_width`
/// adds the zero-width format characters that [`sanitize`] leaves alone.
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
