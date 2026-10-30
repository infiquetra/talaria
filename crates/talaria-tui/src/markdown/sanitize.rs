use std::borrow::Cow;

use talaria_core::text::sanitize;

/// Sanitize text while retaining markdown's line breaks and tabs.
#[must_use]
pub fn sanitize_untrusted(text: &str) -> Cow<'_, str> {
    if !text.contains('\r')
        && text
            .split(['\n', '\t'])
            .all(|part| matches!(sanitize(part), Cow::Borrowed(_)))
    {
        return Cow::Borrowed(text);
    }

    let mut clean = String::with_capacity(text.len());
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        if matches!(ch, '\n' | '\t') {
            let segment = &text[start..at];
            let segment = if ch == '\n' {
                segment.strip_suffix('\r').unwrap_or(segment)
            } else {
                segment
            };
            clean.push_str(&sanitize(segment));
            clean.push(ch);
            start = at + ch.len_utf8();
        }
    }
    clean.push_str(&sanitize(&text[start..]));
    Cow::Owned(clean)
}
