use super::Failure;
pub const USAGE: &str = "Usage: /providers [keys|accounts|endpoints]";
pub(crate) fn check_reason(error: &Failure, route: &str) -> String {
    match error {
        Failure::NotSent(reason) => sanitize(reason),
        _ => failure(error, route),
    }
}

pub fn sanitize(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        let n = u32::from(c);
        match n {
            9 => result.push_str("    "),
            0..=31 => {
                if let Some(p) = char::from_u32(0x2400 + n) {
                    result.push(p)
                }
            }
            127 => result.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x202a..=0x202e
            | 0x2066..=0x2069
            | 0x2028
            | 0x2029
            | 0xe0000..=0xe007f => result.push_str(&format!("‹U+{n:04X}›")),
            _ => result.push(c),
        }
    }
    result
}
pub(crate) fn clean(text: &str) -> bool {
    sanitize(text) == text
}
pub(crate) fn sentence(text: &str) -> String {
    let text = sanitize(text);
    if text.ends_with(['.', '!', '?']) {
        text
    } else {
        format!("{text}.")
    }
}
pub(crate) fn failure(error: &Failure, route: &str) -> String {
    match error {
        Failure::NotSent(reason) => {
            format!("Talaria could not send the request: {}", sentence(reason))
        }
        Failure::Unknown | Failure::BadReply => "Hermes did not return a readable answer.".into(),
        Failure::Refused { message, .. } => format!("Hermes refused: {}", sentence(message)),
        Failure::Http { status, detail } => match status {
            401 => "Hermes did not accept Talaria's ticket.".into(),
            403 => "Hermes refused a request for another profile.".into(),
            404 => format!(
                "This Hermes has no {route}. It may be older or newer than Talaria expects."
            ),
            400 | 409 => detail
                .as_deref()
                .map(sanitize)
                .unwrap_or_else(|| format!("Hermes refused the request ({status}).")),
            422 => format!(
                "Hermes refused the form{}.",
                detail
                    .as_deref()
                    .map(|d| format!(": {}", sanitize(d)))
                    .unwrap_or_default()
            ),
            429 => "Hermes is limiting requests. Wait a moment and try again.".into(),
            500..=599 => format!("Hermes reported an internal error ({status})."),
            _ => format!("Hermes refused the request ({status})."),
        },
    }
}
pub(crate) fn unknown(noun: &str) -> String {
    format!(
        "The connection dropped before Hermes answered. The {noun} may or may not have happened; the list will reload."
    )
}
pub(crate) fn matches(filter: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    filter
        .to_lowercase()
        .split_whitespace()
        .all(|word| text.contains(word))
}
pub(crate) fn relative(seconds: i64) -> String {
    if seconds < 0 {
        "expired".into()
    } else if seconds < 3600 {
        format!("in {} min", (seconds / 60).max(1))
    } else if seconds < 172800 {
        format!("in {} h", seconds / 3600)
    } else {
        format!("in {} d", seconds / 86400)
    }
}
