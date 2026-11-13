use std::collections::BTreeSet;
use talaria_app::theme_file::{PALETTE_SLOTS, ROLES, area_background, catalogue};

#[test]
fn catalogue_is_complete_and_self_consistent() {
    let entries = catalogue();
    assert_eq!(entries.len(), 323);
    assert_eq!(PALETTE_SLOTS.len(), 12);
    assert_eq!(PALETTE_SLOTS.iter().collect::<BTreeSet<_>>().len(), 12);
    assert_eq!(ROLES.iter().collect::<BTreeSet<_>>().len(), 14);
    let names: BTreeSet<_> = entries.iter().map(|entry| entry.token).collect();
    assert_eq!(names.len(), 323);
    assert_eq!(entries.iter().filter(|entry| entry.reserved).count(), 30);
    for entry in entries {
        assert!(ROLES.contains(&entry.role), "{}", entry.token);
        assert!(!entry.description.is_empty());
        assert!(entry.token.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        }));
        assert_eq!(entry.reserved, entry.token.starts_with("picker."));
        for attr in entry.attributes.split(',').filter(|attr| !attr.is_empty()) {
            assert!(
                [
                    "bold",
                    "dim",
                    "italic",
                    "underline",
                    "reverse",
                    "strikethrough"
                ]
                .contains(&attr),
                "{}: {attr}",
                entry.token
            );
        }
        assert!(
            matches!(area_background(entry.token), Some("background" | "surface")),
            "{}",
            entry.token
        );
        let family = entry.token.split('.').next().unwrap();
        let area = if [
            "transcript",
            "composer",
            "status",
            "tool",
            "diff",
            "workspace",
        ]
        .contains(&family)
        {
            "background"
        } else {
            "surface"
        };
        assert_eq!(area_background(entry.token), Some(area));
        assert_eq!(entry.area, area);
    }
}

#[test]
fn documentation_lists_exactly_the_catalogue_names() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/theme-tokens.md");
    let doc = std::fs::read_to_string(path).unwrap();
    let expected: BTreeSet<_> = catalogue().iter().map(|entry| entry.token).collect();
    let found: BTreeSet<_> = doc
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split_once('`').map(|(name, _)| name))
        .collect();
    assert_eq!(found, expected);
}
