//! OSC 52 clipboard sequence. The stub returns an empty string.

/// `ESC ] 52 ; c ; <base64> BEL`. The stub does not encode.
#[must_use]
pub fn osc52(text: &str) -> String {
    let _ = text;
    String::new()
}
