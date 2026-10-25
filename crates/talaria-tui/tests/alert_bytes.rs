//! Alert bytes: bell and desktop notification sequences. Behavior contract: issue 228.

use talaria_tui::terminal::alert_bytes;

#[test]
fn bell_and_osc9_exact() {
    let bytes = alert_bytes("needs you: approval: ls", true, true, false);
    let mut expected = vec![0x07];
    expected.extend_from_slice(b"\x1b]9;Talaria: needs you: approval: ls");
    expected.push(0x07);
    assert_eq!(bytes, expected);

    assert_eq!(
        alert_bytes("needs you: approval: ls", true, false, false),
        vec![0x07]
    );

    let bytes = alert_bytes("needs you: approval: ls", false, true, false);
    let mut expected = b"\x1b]9;Talaria: needs you: approval: ls".to_vec();
    expected.push(0x07);
    assert_eq!(bytes, expected);

    assert!(alert_bytes("needs you: approval: ls", false, false, false).is_empty());
}

#[test]
fn body_sanitised_and_cut() {
    // Controls, DEL, C1, and bidi overrides are removed.
    let bytes = alert_bytes("a\u{0}b\u{7}c\u{7f}d\u{9c}e\u{202e}f", false, true, false);
    let mut expected = b"\x1b]9;Talaria: abcdef".to_vec();
    expected.push(0x07);
    assert_eq!(bytes, expected);

    // A 200-character body becomes 119 characters and an ellipsis.
    let long = "w".repeat(200);
    let bytes = alert_bytes(&long, false, true, false);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with("\u{1b}]9;Talaria: "));
    assert!(text.ends_with('\u{7}'));
    let body = text
        .strip_prefix("\u{1b}]9;Talaria: ")
        .unwrap()
        .strip_suffix('\u{7}')
        .unwrap();
    assert_eq!(body.chars().count(), 120);
    assert_eq!(body, "w".repeat(119) + "…");
}

#[test]
fn tmux_passthrough_wrap() {
    let bytes = alert_bytes("needs you: approval: ls", true, true, true);
    // BEL bare first, then the wrapped payload.
    assert_eq!(bytes[0], 0x07);
    let wrapped = &bytes[1..];
    assert!(
        wrapped.starts_with(b"\x1bPtmux;"),
        "tmux passthrough opens: {wrapped:?}"
    );
    assert!(
        wrapped.ends_with(b"\x1b\\"),
        "tmux passthrough closes: {wrapped:?}"
    );
    let inner = &wrapped["\x1bPtmux;".len()..wrapped.len() - "\x1b\\".len()];
    assert!(
        inner.starts_with(b"\x1b\x1b]9;Talaria: needs you: approval: ls"),
        "the payload escape is doubled: {inner:?}"
    );
    assert!(
        inner.ends_with(b"\x07"),
        "the payload keeps its bare BEL: {inner:?}"
    );
}
