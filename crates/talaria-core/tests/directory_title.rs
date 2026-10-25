//! Red-run test 6 (#231): title cleanup.

use talaria_core::directory::title::{TitleError, clean_title};

#[test]
fn clean_title_matches_hermes_rules() {
    // Each control range is removed.
    for raw in [
        "a\x00b", "a\x08b", "a\x0bb", "a\x0cb", "a\x0eb", "a\x1fb", "a\x7fb",
    ] {
        assert_eq!(clean_title(raw), Ok("ab".to_owned()), "input {raw:?}");
    }
    // Each invisible range is removed.
    for raw in [
        "a\u{200b}b",
        "a\u{200f}b",
        "a\u{2028}b",
        "a\u{202e}b",
        "a\u{2060}b",
        "a\u{2069}b",
        "a\u{feff}b",
        "a\u{fffc}b",
        "a\u{fff9}b",
        "a\u{fffb}b",
    ] {
        assert_eq!(clean_title(raw), Ok("ab".to_owned()), "input {raw:?}");
    }
    // Tab, newline and carriage return survive removal, then collapse.
    assert_eq!(clean_title("a\t\tb"), Ok("a b".to_owned()));
    assert_eq!(clean_title("a\nb"), Ok("a b".to_owned()));
    assert_eq!(clean_title("a\rb"), Ok("a b".to_owned()));
    // Removal runs before the whitespace collapse, then the trim.
    assert_eq!(clean_title("a \u{200b} b"), Ok("a b".to_owned()));
    assert_eq!(clean_title("  padded  "), Ok("padded".to_owned()));

    // Counted in code points: a four-byte character is one.
    let hundred = format!("{}😀", "x".repeat(99));
    assert_eq!(hundred.chars().count(), 100);
    assert_eq!(clean_title(&hundred), Ok(hundred.clone()));
    let hundred_one = format!("{hundred}y");
    assert_eq!(
        clean_title(&hundred_one),
        Err(TitleError::TooLong { chars: 101 })
    );

    // Nothing left means no title.
    assert_eq!(clean_title(""), Err(TitleError::Empty));
    assert_eq!(clean_title("   "), Err(TitleError::Empty));
    assert_eq!(clean_title("\u{200b}\u{feff}"), Err(TitleError::Empty));
}
