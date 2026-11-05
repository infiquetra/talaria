//! Theme files layer fields without changing the legacy path.
use ratatui::style::{Color, Modifier};
use std::collections::BTreeMap;
use talaria_app::theme_file::{Base, Chrome, ColorValue, StyleFields, ThemeFile, catalogue};
use talaria_tui::theme::{self, *};

fn inputs(skin: Option<&Skin>) -> ThemeInputs<'_> {
    ThemeInputs {
        skin,
        skipped: &[],
        follow_hermes_skin: true,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Reduced, MotionSource::Config),
        terminal: TerminalColors::default(),
    }
}
fn file(base: Base) -> ThemeFile {
    ThemeFile {
        name: Some("Fixture".into()),
        extends: base,
        follow_hermes_skin: true,
        palette: BTreeMap::new(),
        roles: BTreeMap::new(),
        elements: BTreeMap::new(),
        chrome: Chrome::default(),
    }
}

#[test]
fn no_file_calls_the_unchanged_resolver_for_real_skins_and_all_depths() {
    let skins = ["default", "ares"].map(|name| {
        let bytes = std::fs::read_to_string(format!(
            "{}/tests/data/skins/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        Skin::from_value(&serde_json::from_str(&bytes).unwrap())
            .0
            .unwrap()
    });
    for skin in [None, Some(&skins[0]), Some(&skins[1])] {
        for depth in [
            ColorDepth::None,
            ColorDepth::Ansi16,
            ColorDepth::Ansi256,
            ColorDepth::TrueColor,
        ] {
            let mut input = inputs(skin);
            input.depth.0 = depth;
            let expected = theme::resolve(&input);
            let actual = theme::resolve_with_theme_file(&input, None);
            assert_eq!(format!("{:?}", actual.theme), format!("{expected:?}"));
            assert!(actual.report.is_none());
        }
    }
}

#[test]
fn all_bases_resolve_every_catalogue_token_and_nine_sources() {
    for base in [Base::Terminal, Base::Dark, Base::Light] {
        let result = theme::resolve_with_theme_file(&inputs(None), Some(&file(base)));
        assert_eq!(result.elements.len(), 323);
        let report = result.report.unwrap();
        assert_eq!(report.base, base.as_str());
        assert_eq!(report.file, "Fixture");
        assert!(!report.skin_participates);
        for entry in catalogue() {
            assert!(result.elements.contains_key(entry.token));
            let sources = &report.sources[entry.token];
            for field in [
                "fg",
                "bg",
                "underline_color",
                "bold",
                "dim",
                "italic",
                "underline",
                "reverse",
                "strikethrough",
            ] {
                assert!(sources.contains_key(field), "{}.{field}", entry.token);
            }
        }
    }
}

#[test]
fn palette_role_element_fields_and_false_default_obey_exact_precedence() {
    let mut file = file(Base::Dark);
    file.palette
        .insert("primary".into(), ColorValue::Rgb(11, 22, 33));
    file.roles.insert(
        "primary".into(),
        StyleFields {
            fg: Some(ColorValue::Rgb(44, 55, 66)),
            bg: Some(ColorValue::Rgb(1, 2, 3)),
            underline_color: Some(ColorValue::Rgb(4, 5, 6)),
            italic: Some(true),
            ..StyleFields::default()
        },
    );
    file.elements.insert(
        "transcript.heading.1".into(),
        StyleFields {
            fg: Some(ColorValue::Default),
            bold: Some(false),
            underline_color: Some(ColorValue::Palette("accent".into())),
            ..StyleFields::default()
        },
    );
    let result = theme::resolve_with_theme_file(&inputs(None), Some(&file));
    let first = result.elements["transcript.heading.1"];
    let second = result.elements["transcript.heading.2"];
    assert_eq!(first.fg, Some(Color::Reset));
    assert_eq!(first.bg, Some(Color::Rgb(1, 2, 3)));
    assert_eq!(first.underline_color, Some(Color::Rgb(0x80, 0xdc, 0xff)));
    assert!(!first.add_modifier.contains(Modifier::BOLD));
    assert!(first.add_modifier.contains(Modifier::ITALIC));
    assert_eq!(second.fg, Some(Color::Rgb(44, 55, 66)));
    assert_eq!(second.underline_color, Some(Color::Rgb(4, 5, 6)));
    let sources = &result.report.unwrap().sources;
    assert_eq!(
        sources["transcript.heading.1"]["fg"],
        "elements.transcript.heading.1.fg"
    );
    assert_eq!(
        sources["transcript.heading.1"]["bold"],
        "elements.transcript.heading.1.bold"
    );
    assert_eq!(sources["transcript.heading.2"]["fg"], "roles.primary.fg");
    assert_eq!(
        sources["transcript.heading.2"]["italic"],
        "roles.primary.italic"
    );
}

#[test]
fn false_clears_every_attribute_and_default_resets_every_colour_field() {
    let mut file = file(Base::Dark);
    file.roles.insert(
        "primary".into(),
        StyleFields {
            fg: Some(ColorValue::Rgb(1, 2, 3)),
            bg: Some(ColorValue::Rgb(4, 5, 6)),
            underline_color: Some(ColorValue::Rgb(7, 8, 9)),
            bold: Some(true),
            dim: Some(true),
            italic: Some(true),
            underline: Some(true),
            reverse: Some(true),
            strikethrough: Some(true),
        },
    );
    file.elements.insert(
        "transcript.heading.1".into(),
        StyleFields {
            fg: Some(ColorValue::Default),
            bg: Some(ColorValue::Default),
            underline_color: Some(ColorValue::Default),
            bold: Some(false),
            dim: Some(false),
            italic: Some(false),
            underline: Some(false),
            reverse: Some(false),
            strikethrough: Some(false),
        },
    );
    let result = theme::resolve_with_theme_file(&inputs(None), Some(&file));
    let mask = Modifier::BOLD
        | Modifier::DIM
        | Modifier::ITALIC
        | Modifier::UNDERLINED
        | Modifier::REVERSED
        | Modifier::CROSSED_OUT;
    assert!(
        result.elements["transcript.heading.2"]
            .add_modifier
            .contains(mask)
    );
    let style = result.elements["transcript.heading.1"];
    assert!(!style.add_modifier.intersects(mask));
    assert_eq!(
        (style.fg, style.bg, style.underline_color),
        (Some(Color::Reset), Some(Color::Reset), Some(Color::Reset))
    );
}

#[test]
fn minimal_accent_override_retains_unrelated_ares_skin_distinctions() {
    let bytes = include_str!("data/skins/ares.json");
    let skin = Skin::from_value(&serde_json::from_str(bytes).unwrap())
        .0
        .unwrap();
    let input = inputs(Some(&skin));
    let old = theme::resolve(&input);
    let mut file = file(Base::Dark);
    file.palette
        .insert("accent".into(), ColorValue::Rgb(255, 184, 107));
    file.elements.insert(
        "composer.border.focused".into(),
        StyleFields {
            fg: Some(ColorValue::Palette("accent".into())),
            bold: Some(true),
            ..StyleFields::default()
        },
    );
    let result = theme::resolve_with_theme_file(&input, Some(&file));
    assert_eq!(
        result.theme.composer.border.fg,
        Some(Color::Rgb(255, 184, 107))
    );
    assert!(
        result
            .theme
            .composer
            .border
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(
        result.theme.markdown.inline_code.fg,
        Some(Color::Rgb(255, 184, 107))
    );
    assert_eq!(result.theme.markdown.heading1.fg, old.markdown.heading1.fg);
    assert_eq!(
        result.theme.composer.marker.fg,
        Some(Color::Rgb(255, 184, 107))
    );
    assert_eq!(
        result.theme.tool.diff_hunk.fg,
        Some(Color::Rgb(255, 184, 107))
    );
    assert_eq!(result.theme.tool.diff_file.fg, old.tool.diff_file.fg);
    assert_eq!(
        result.theme.status.style(talaria_tui::status::Tone::Text),
        old.status.style(talaria_tui::status::Tone::Text)
    );
    assert!(result.report.unwrap().skin_participates);
    file.follow_hermes_skin = false;
    let detached = theme::resolve_with_theme_file(&input, Some(&file));
    assert!(!detached.report.unwrap().skin_participates);
    assert_eq!(
        detached.theme.markdown.heading1.fg,
        Some(Color::Rgb(0xaa, 0xcc, 0xff))
    );
}

#[test]
fn palette_source_and_area_fallback_and_underline_foreground_are_tracked() {
    let mut file = file(Base::Light);
    file.palette
        .insert("primary".into(), ColorValue::Rgb(10, 20, 30));
    let result = theme::resolve_with_theme_file(&inputs(None), Some(&file));
    assert_eq!(
        result.elements["transcript.heading.1"].fg,
        Some(Color::Rgb(10, 20, 30))
    );
    assert_eq!(
        result.elements["transcript.heading.1"].bg,
        Some(Color::Rgb(255, 255, 255))
    );
    assert_eq!(
        result.elements["code.inline"].bg,
        Some(Color::Rgb(240, 240, 240))
    );
    assert_eq!(
        result.elements["transcript.heading.1"].underline_color,
        Some(Color::Rgb(10, 20, 30))
    );
    assert_eq!(
        result.report.unwrap().sources["transcript.heading.1"]["fg"],
        "palette.primary"
    );
    assert!(
        result.theme.markdown.heading1.bg.is_none(),
        "inline backgrounds are never painted"
    );
}
