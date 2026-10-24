mod markdown_support;

use std::borrow::Cow;

use proptest::prelude::*;
use talaria_tui::markdown::sanitize_untrusted;

#[test]
fn controls_become_pictures() {
    let input = format!(
        "{}{}{}{}",
        char::from_u32(0x1b).unwrap(),
        char::from_u32(0).unwrap(),
        char::from_u32(0x07).unwrap(),
        char::from_u32(0x7f).unwrap()
    );
    let expected = format!(
        "{}{}{}{}",
        char::from_u32(0x241b).unwrap(),
        char::from_u32(0x2400).unwrap(),
        char::from_u32(0x2407).unwrap(),
        char::from_u32(0x2421).unwrap()
    );
    assert_eq!(sanitize_untrusted(&input).as_ref(), expected);
}

#[test]
fn bidi_and_separators_become_hex() {
    let input = "\u{202E}\u{2066}\u{061C}\u{2028}\u{0085}\u{E0041}";
    let expected = "‹U+202E›‹U+2066›‹U+061C›‹U+2028›‹U+0085›‹U+E0041›";
    assert_eq!(sanitize_untrusted(input).as_ref(), expected);
}

#[test]
fn newline_tab_kept_crlf_is_one() {
    let cr = char::from_u32(0x240d).unwrap();
    assert_eq!(sanitize_untrusted("a\nb").as_ref(), "a\nb");
    assert_eq!(sanitize_untrusted("a\tb").as_ref(), "a\tb");
    assert_eq!(sanitize_untrusted("a\r\nb").as_ref(), "a\nb");
    assert_eq!(
        sanitize_untrusted("a\rb").as_ref(),
        format!("a{cr}b").as_str()
    );
    assert_eq!(
        sanitize_untrusted("a\r").as_ref(),
        format!("a{cr}").as_str()
    );
}

#[test]
fn clean_text_is_borrowed() {
    for text in [
        "",
        "hello",
        "a\nb",
        "a\tb",
        "a\u{200D}b",
        "\u{FEFF}",
        "\u{200B}",
        "\u{2060}",
        "\u{FE0F}",
    ] {
        let out = sanitize_untrusted(text);
        assert!(matches!(out, Cow::Borrowed(same) if std::ptr::eq(same, text)));
    }
}

fn forbidden(ch: char) -> bool {
    let code = u32::from(ch);
    if ch == '\n' || ch == '\t' {
        return false;
    }
    code <= 0x1F
        || code == 0x7F
        || (0x80..=0x9F).contains(&code)
        || matches!(code, 0x061C | 0x200E | 0x200F | 0x2028 | 0x2029)
        || (0x202A..=0x202E).contains(&code)
        || (0x2066..=0x2069).contains(&code)
        || (0xE0000..=0xE007F).contains(&code)
}

fn is_hex_stand_in(text: &str) -> bool {
    let mut chars = text.chars();
    if chars.next() != Some('\u{2039}') || chars.next() != Some('U') || chars.next() != Some('+') {
        return false;
    }
    let mut digits = 0usize;
    for ch in chars {
        if ch == '\u{203A}' {
            return digits >= 4;
        }
        let upper_hex = ch.is_ascii_digit() || ('A'..='F').contains(&ch);
        if !upper_hex {
            return false;
        }
        digits += 1;
        if digits > 6 {
            return false;
        }
    }
    false
}

fn skip_stand_in(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let Some(first) = chars.peek().copied() else {
        return false;
    };
    let code = u32::from(first);
    if (0x2400..=0x243F).contains(&code) {
        chars.next();
        return true;
    }
    let pending: String = chars.clone().collect();
    let Some(end) = pending.find('\u{203A}') else {
        return false;
    };
    let candidate = &pending[..end + '\u{203A}'.len_utf8()];
    if !is_hex_stand_in(candidate) {
        return false;
    }
    for _ in candidate.chars() {
        chars.next();
    }
    true
}

fn preserved(input: &str, output: &str) -> bool {
    let mut out = output.chars().peekable();
    for ch in input.chars() {
        if forbidden(ch) {
            continue;
        }
        loop {
            match out.peek().copied() {
                Some(next) if next == ch => {
                    out.next();
                    break;
                }
                Some(_) if skip_stand_in(&mut out) => {}
                _ => return false,
            }
        }
    }
    while out.peek().is_some() {
        if !skip_stand_in(&mut out) {
            return false;
        }
    }
    true
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn output_has_no_forbidden_chars(chars in proptest::collection::vec(any::<char>(), 0..=80)) {
        let text: String = chars.into_iter().collect();
        let rendered = sanitize_untrusted(&text);
        prop_assert!(rendered.chars().all(|ch| !forbidden(ch)));
        prop_assert!(preserved(&text, rendered.as_ref()));
    }
}
