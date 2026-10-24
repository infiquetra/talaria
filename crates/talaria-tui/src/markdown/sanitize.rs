use std::borrow::Cow;

/// Replace terminal controls and directional overrides with visible text.
#[must_use]
pub fn sanitize_untrusted(text: &str) -> Cow<'_, str> {
    if !text.chars().any(needs_replacement) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let code = u32::from(ch);
        match ch {
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                out.push('\n');
            }
            '\n' | '\t' => out.push(ch),
            _ if code < 32 => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    out.push(picture);
                }
            }
            _ if code == 127 => out.push('␡'),
            _ if hex_replacement(code) => out.push_str(&format!("‹U+{code:04X}›")),
            _ => out.push(ch),
        }
    }
    Cow::Owned(out)
}

fn needs_replacement(ch: char) -> bool {
    let code = u32::from(ch);
    (code < 32 && ch != '\n' && ch != '\t') || code == 127 || hex_replacement(code)
}

fn hex_replacement(code: u32) -> bool {
    matches!(code, 0x80..=0x9f | 0x061c | 0x200e..=0x200f | 0x202a..=0x202e
        | 0x2066..=0x2069 | 0x2028..=0x2029 | 0xe0000..=0xe007f)
}
