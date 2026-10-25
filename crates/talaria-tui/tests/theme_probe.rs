//! The start-up probe learns the terminal's colours. Behavior contract: issue 227.

use talaria_tui::terminal::{PROBE_QUERY, parse_probe_reply};
use talaria_tui::theme::Rgb;

fn position(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap_or_else(|| panic!("{needle:?} not in the probe query"))
}

#[test]
fn probe_asks_for_colours_before_device_attributes() {
    let background = b"\x1b]11;?\x1b\\";
    let foreground = b"\x1b]10;?\x1b\\";
    let end = b"\x1b[c";
    assert!(position(PROBE_QUERY, background) < position(PROBE_QUERY, end));
    assert!(position(PROBE_QUERY, foreground) < position(PROBE_QUERY, end));

    let cursor = b"\x1b[24;1R".as_slice();
    let bg = b"\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\".as_slice();
    let fg = b"\x1b]10;rgb:e0e0/e0e0/e0e0\x07".as_slice();
    let marker = b"\x1b[?62;22c".as_slice();
    for order in [
        [cursor, bg, fg, marker],
        [fg, cursor, bg, marker],
        [bg, fg, cursor, marker],
    ] {
        let reply = order.concat();
        let (parsed, complete) = parse_probe_reply(&reply);
        assert!(complete, "the end marker ends the reply");
        assert_eq!(
            parsed.colors.background,
            Some(Rgb(0x1e, 0x1e, 0x1e)),
            "the background parses in any order"
        );
        assert_eq!(
            parsed.colors.foreground,
            Some(Rgb(0xe0, 0xe0, 0xe0)),
            "the foreground parses in any order"
        );
        assert_eq!(parsed.cursor, Some((0, 23)));
    }

    let reply = [cursor, marker].concat();
    let (parsed, complete) = parse_probe_reply(&reply);
    assert!(complete);
    assert_eq!(parsed.colors.background, None);
    assert_eq!(parsed.colors.foreground, None);

    // A colour reply after the marker is drained, never parsed.
    let reply = [cursor, marker, bg].concat();
    let (parsed, complete) = parse_probe_reply(&reply);
    assert!(complete);
    assert_eq!(parsed.colors.background, None);
    assert_eq!(parsed.colors.foreground, None);
}
