use talaria_app::config::parse;

#[test]
fn theme_file_and_profile_settings_are_read() {
    let loaded =
        parse("[theme]\nfile = \"quiet\"\n[theme.profiles.\"fixture\"]\nfile = \"other\"\n");
    assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
    assert_eq!(loaded.config.theme.file.as_deref(), Some("quiet"));
    assert_eq!(
        loaded
            .config
            .theme
            .profiles
            .get("fixture")
            .map(String::as_str),
        Some("other")
    );
    let defaults = parse("");
    assert_eq!(defaults.config.theme.file, None);
    assert!(defaults.config.theme.profiles.is_empty());
}

#[test]
fn invalid_settings_have_one_notice_and_obsolete_name_is_unchanged() {
    for input in [
        "[theme]\nfile = \"../bad\"\n",
        "[theme]\nfile = 1\n",
        "[theme.profiles.fixture]\nextra = 1\n",
    ] {
        let loaded = parse(input);
        assert_eq!(loaded.notices.len(), 1, "{input}: {:?}", loaded.notices);
    }
    let loaded = parse("[theme]\nname = \"old\"\n");
    assert_eq!(
        loaded.notices,
        ["theme.name is no longer used: Talaria follows the Hermes skin (see /skin and /theme)."]
    );
}
