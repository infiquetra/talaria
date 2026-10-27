//! Safe display of untrusted text.

pub(crate) fn sanitize(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    for c in text.chars() {
        let code = u32::from(c);
        match code {
            0x09 => clean.push_str("    "),
            0x00..=0x1f => clean.push(char::from_u32(0x2400 + code).unwrap_or('\u{fffd}')),
            0x7f => clean.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x202a..=0x202e
            | 0x2066..=0x2069
            | 0x2028
            | 0x2029
            | 0xe0000..=0xe007f => clean.push_str(&format!("\u{2039}U+{code:04X}\u{203a}")),
            _ => clean.push(c),
        }
    }
    clean
}
