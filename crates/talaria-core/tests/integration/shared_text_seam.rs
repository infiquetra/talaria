//! Shared text safety and ownership.

use std::borrow::Cow;
use std::collections::BTreeSet;

use unicode_general_category::{GeneralCategory, UNICODE_VERSION, get_general_category};

/// Exact neutralized set. A deletion here has to be an intentional edit, recorded in
/// `docs/engineering-journal/DECISIONS.md` in the same change. Unicode 16.0 `Cf` data is
/// `unicode-general-category` 1.1.0.
const NEUTRALIZED_SNAPSHOT: &[(u32, u32)] = &[
    (0x0000, 0x001F),
    (0x007F, 0x00A0),
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x2000, 0x200F),
    (0x2028, 0x202F),
    (0x205F, 0x2064),
    (0x2066, 0x206F),
    (0x2800, 0x2800),
    (0x3000, 0x3000),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0D),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE007F),
    (0xE0100, 0xE01EF),
];

#[test]
fn shared_sanitizer_preserves_visible_replacements_and_single_table() {
    let input = "a\u{1b}b\u{7}c\u{202e}d";
    assert_eq!(talaria_core::text::sanitize(input), "a␛b␇c‹U+202E›d");
    let core = include_str!("../../src/text.rs");
    let terminal = include_str!("../../../talaria-tui/src/history.rs");
    assert!(
        core.contains("fn stand_in"),
        "the character table belongs to core"
    );
    assert!(
        !terminal.contains("fn stand_in"),
        "the renderer must not keep a second table"
    );
}

#[test]
fn neutralized_set_matches_the_category_pin_and_snapshot() {
    assert_eq!(UNICODE_VERSION, (16, 0, 0));
    let snapshot = expand(NEUTRALIZED_SNAPSHOT);
    let expected = expected_neutralized();
    assert_eq!(
        snapshot, expected,
        "the checked-in snapshot drifted from the Cf pin and the named ranges"
    );
    assert_eq!(snapshot.len(), 549);

    for code in 0..=0x10FFFF {
        let Some(c) = char::from_u32(code) else {
            continue;
        };
        assert_eq!(
            talaria_core::text::is_neutralized(c),
            snapshot.contains(&code),
            "U+{code:04X}"
        );
    }

    for code in &snapshot {
        let c = char::from_u32(*code).expect("snapshot code is a scalar");
        assert_visible_stand_in(c);
        let marked = format!("rm{c}-rf");
        let rendered = talaria_core::text::sanitize(&marked);
        assert_ne!(rendered.as_ref(), "rm -rf", "U+{code:04X} drew as a space");
        assert_ne!(rendered.as_ref(), "rm-rf", "U+{code:04X} disappeared");
        assert!(
            !rendered.contains(c),
            "U+{code:04X} survived in {rendered:?}"
        );
    }
}

#[test]
fn presentation_selectors_pass_through() {
    for c in ['\u{FE0E}', '\u{FE0F}'] {
        assert!(!talaria_core::text::is_neutralized(c));
        let text = c.to_string();
        assert!(matches!(talaria_core::text::sanitize(&text), Cow::Borrowed(same) if same == text));
    }
    let heart = "\u{2764}\u{FE0F}";
    assert!(matches!(talaria_core::text::sanitize(heart), Cow::Borrowed(same) if same == heart));
}

#[test]
fn every_invisible_table_uses_the_shared_predicate() {
    let files = [
        (
            "attention",
            include_str!("../../src/attention.rs"),
            "text::is_neutralized",
        ),
        (
            "fleet",
            include_str!("../../src/fleet_attention.rs"),
            "text::is_neutralized",
        ),
        (
            "rewind",
            include_str!("../../src/rewind/text.rs"),
            "text::is_neutralized",
        ),
        (
            "composer",
            include_str!("../../../talaria-tui/src/composer/buffer.rs"),
            "is_neutralized",
        ),
    ];
    for (name, source, needle) in files {
        assert!(
            source.contains(needle),
            "{name} must consume the shared neutralized set"
        );
    }
    assert!(
        !include_str!("../../src/fleet_attention.rs").contains("fn is_invisible"),
        "fleet attention kept a private invisible table"
    );
    assert!(
        !include_str!("../../src/attention.rs").contains(r"'\u{200b}'"),
        "alert sanitizing kept a private invisible table"
    );
    assert!(
        include_str!("../../../talaria-tui/src/history.rs").contains("is_neutralized"),
        "the composer reaches the shared set through history"
    );
    assert!(
        !include_str!("../../../talaria-tui/src/composer/buffer.rs").contains("talaria_core"),
        "the composer must not name talaria_core"
    );
}

fn assert_visible_stand_in(c: char) {
    let raw = c.to_string();
    let got = talaria_core::text::sanitize(&raw);
    let code = u32::from(c);
    if c == '\t' {
        assert_eq!(got.as_ref(), "    ");
        return;
    }
    if (0x00..=0x1F).contains(&code) {
        let picture = char::from_u32(0x2400 + code).expect("control picture");
        assert_eq!(got.as_ref(), picture.to_string());
        return;
    }
    if code == 0x7F {
        assert_eq!(got.as_ref(), "\u{2421}");
        return;
    }
    assert_eq!(
        got.as_ref(),
        format!("\u{2039}U+{code:04X}\u{203a}"),
        "U+{code:04X}"
    );
    assert!(
        got.chars().any(|ch| !ch.is_whitespace()),
        "U+{code:04X} had no visible stand-in"
    );
}

fn expand(ranges: &[(u32, u32)]) -> BTreeSet<u32> {
    let mut set = BTreeSet::new();
    for &(start, end) in ranges {
        for code in start..=end {
            set.insert(code);
        }
    }
    set
}

/// C0/C1, U+2028/U+2029, the tag block, every `Cf`, every variation selector except
/// U+FE0E and U+FE0F, the default-ignorable marks U+034F and U+17B4..=U+17B5, the
/// Hangul fillers, and the blank-width spaces.
fn expected_neutralized() -> BTreeSet<u32> {
    let mut set = BTreeSet::new();
    for code in 0x00..=0x1F {
        set.insert(code);
    }
    set.insert(0x7F);
    for code in 0x80..=0x9F {
        set.insert(code);
    }
    set.insert(0x2028);
    set.insert(0x2029);
    for code in 0xE0000..=0xE007F {
        set.insert(code);
    }
    for code in 0xFE00..=0xFE0D {
        set.insert(code);
    }
    for code in 0x180B..=0x180F {
        set.insert(code);
    }
    for code in 0xE0100..=0xE01EF {
        set.insert(code);
    }
    set.insert(0x034F);
    set.insert(0x17B4);
    set.insert(0x17B5);
    for code in [0x115F, 0x1160, 0x3164, 0xFFA0] {
        set.insert(code);
    }
    set.insert(0x00A0);
    for code in 0x2000..=0x200A {
        set.insert(code);
    }
    for code in [0x202F, 0x205F, 0x2800, 0x3000] {
        set.insert(code);
    }
    for code in 0..=0x10FFFF {
        let Some(c) = char::from_u32(code) else {
            continue;
        };
        if get_general_category(c) == GeneralCategory::Format {
            set.insert(code);
        }
    }
    set
}
