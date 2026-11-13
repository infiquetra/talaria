use talaria_app::theme_file::{Base, BorderPreset, ColorValue, parse};

#[test]
fn study_examples_parse() {
    let minimal = parse(include_str!("../data/themes/minimal.toml")).unwrap();
    assert_eq!(minimal.extends, Base::Dark);
    assert_eq!(minimal.palette["accent"], ColorValue::Rgb(255, 184, 107));
    assert_eq!(minimal.elements["composer.border.focused"].bold, Some(true));
    let full = parse(include_str!("../data/themes/ember.toml")).unwrap();
    assert_eq!(full.palette.len(), 12);
    assert_eq!(full.roles.len(), 14);
    assert_eq!(full.chrome.border, BorderPreset::Rounded);
    assert!(!full.follow_hermes_skin);
}

#[test]
fn invalid_input_is_rejected_with_key_and_location() {
    let cases = [
        ("schema = 1\nunknown = 1\n", "unknown"),
        ("schema = 1\n[unknown]\nx = 1\n", "unknown"),
        (
            "schema = 1\n[elements.\"transcript.hedding\"]\nfg = \"default\"\n",
            "transcript.hedding",
        ),
        ("schema = 1\n[roles.typo]\nbold = true\n", "typo"),
        ("schema = 1\n[palette]\ntypo = \"default\"\n", "typo"),
        ("schema = 1\n[palette]\naccent = \"#12345\"\n", "accent"),
        ("schema = 1\n[palette]\naccent = \"ansi:16\"\n", "accent"),
        ("schema = 1\n[palette]\naccent = \"index:256\"\n", "accent"),
        ("schema = 1\n[palette]\naccent = \"rgb(1,2,3)\"\n", "accent"),
        ("schema = 1\n[roles.text]\nbold = \"yes\"\n", "bold"),
        ("schema = 1\n[roles.text]\nblink = true\n", "blink"),
        ("schema = 2\n", "schema"),
        ("name = \"missing schema\"\n", "schema"),
        ("schema = 1\nextends = \"ember\"\n", "extends"),
        (
            "schema = 1\n[palette]\naccent = \"$palette.primary\"\n",
            "accent",
        ),
        (
            "schema = 1\n[roles.text]\nfg = \"$palette.missing\"\n",
            "fg",
        ),
        ("schema = 1\n[chrome]\nborder = \"dotted\"\n", "border"),
        ("schema = 1\n[roles.text]\nfg = false\n", "fg"),
        ("schema = 1\n[roles.text]\nbold = 1\n", "bold"),
        ("schema = 1\n[roles.text]\nhidden = true\n", "hidden"),
        (
            "schema = 1\n[chrome]\nsection_divider = 'rounded'\n",
            "section_divider",
        ),
        ("schema = 1\n[roles.text]\nfg = '$HOME'\n", "fg"),
        (
            "schema = 1\n[elements.\"transcript.*\"]\nfg='default'\n",
            "transcript.*",
        ),
    ];
    for (input, key) in cases {
        let error = parse(input).unwrap_err();
        assert!(error.key.contains(key), "{input}: {error:?}");
        assert!(error.line > 0 && error.column > 0, "{input}: {error:?}");
        assert!(error.to_string().contains(key));
    }
    let error = parse("schema = [\n").unwrap_err();
    assert!(error.line > 0 && error.column > 0);
}

#[test]
fn semantic_error_locations_point_to_the_offending_key() {
    let error = parse("schema = 1\n[palette]\n  accent = \"ansi:16\"\n").unwrap_err();
    assert_eq!((error.line, error.column), (3, 3));
}
