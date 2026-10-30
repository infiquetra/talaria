//! Text safety shared by domain state and terminal presentation.

use std::borrow::Cow;

/// What one character is written as, when it is not written as itself.
enum StandIn {
    Char(char),
    Spaces,
    Code(u32),
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

/// Replace controls and direction marks with visible stand-ins. Clean text is borrowed.
pub fn sanitize(text: &str) -> Cow<'_, str> {
    let Some(first) = text.char_indices().find(|(_, c)| stand_in(*c).is_some()) else {
        return Cow::Borrowed(text);
    };
    let mut clean = String::with_capacity(text.len() + 8);
    clean.push_str(&text[..first.0]);
    for c in text[first.0..].chars() {
        match stand_in(c) {
            None => clean.push(c),
            Some(StandIn::Char(picture)) => clean.push(picture),
            Some(StandIn::Spaces) => clean.push_str("    "),
            Some(StandIn::Code(code)) => {
                clean.push_str(&format!("\u{2039}U+{code:04X}\u{203a}"));
            }
        }
    }
    Cow::Owned(clean)
}
