/// Build an OSC 52 clipboard request without writing to the terminal.
#[must_use]
pub fn osc52(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4 + 8);
    encoded.push(char::from(0x1b));
    encoded.push_str("]52;c;");
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        encoded.push(char::from(ALPHABET[usize::from(a >> 2)]));
        encoded.push(char::from(ALPHABET[usize::from((a & 3) << 4 | b >> 4)]));
        encoded.push(if chunk.len() > 1 {
            char::from(ALPHABET[usize::from((b & 15) << 2 | c >> 6)])
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            char::from(ALPHABET[usize::from(c & 63)])
        } else {
            '='
        });
    }
    encoded.push('\x07');
    encoded
}

#[cfg(test)]
mod tests {
    use super::osc52;
    #[test]
    fn encodes_clipboard_text() {
        let prefix = format!("{}]52;c;", char::from(0x1b));
        assert_eq!(osc52("ab"), format!("{prefix}YWI=\x07"));
        assert_eq!(osc52("abc"), format!("{prefix}YWJj\x07"));
        assert_eq!(osc52(""), format!("{prefix}\x07"));
    }
}
