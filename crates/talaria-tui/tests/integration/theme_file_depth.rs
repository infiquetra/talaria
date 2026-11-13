//! Rendering constraints apply after every authored colour field.
use ratatui::style::{Color, Modifier};
use std::collections::BTreeMap;
use talaria_app::theme_file::{Base, Chrome, ColorValue, StyleFields, ThemeFile};
use talaria_tui::theme::detect::{self, EnvView};
use talaria_tui::theme::{self, *};

fn resolve(depth: (ColorDepth, DepthSource), color: ColorValue) -> theme::ThemeFileResolution {
    let file = ThemeFile {
        name: None,
        extends: Base::Dark,
        follow_hermes_skin: false,
        palette: BTreeMap::new(),
        roles: BTreeMap::new(),
        chrome: Chrome::default(),
        elements: [(
            "transcript.heading.1".into(),
            StyleFields {
                fg: Some(color.clone()),
                bg: Some(color.clone()),
                underline_color: Some(color),
                bold: Some(true),
                ..StyleFields::default()
            },
        )]
        .into(),
    };
    theme::resolve_with_theme_file(
        &ThemeInputs {
            skin: None,
            skipped: &[],
            follow_hermes_skin: true,
            polarity: (Polarity::Light, PolaritySource::Config),
            depth,
            motion: (Motion::Reduced, MotionSource::Environment),
            terminal: TerminalColors::default(),
        },
        Some(&file),
    )
}

#[test]
fn truecolor_and_xterm256_convert_all_three_color_fields() {
    for (depth, expected) in [
        (ColorDepth::TrueColor, Color::Rgb(255, 0, 0)),
        (
            ColorDepth::Ansi256,
            Color::Indexed(theme::to_ansi256(Rgb(255, 0, 0))),
        ),
    ] {
        let result = resolve((depth, DepthSource::Config), ColorValue::Rgb(255, 0, 0));
        let style = result.elements["transcript.heading.1"];
        assert_eq!(
            (style.fg, style.bg, style.underline_color),
            (Some(expected), Some(expected), Some(expected))
        );
        assert_eq!(result.theme.motion, Motion::Reduced);
    }
}

#[test]
fn sixteen_colors_use_conventional_palette_and_convert_high_indexes() {
    for value in [
        ColorValue::Rgb(255, 0, 0),
        ColorValue::Index(196),
        ColorValue::Ansi(9),
    ] {
        let result = resolve((ColorDepth::Ansi16, DepthSource::Config), value);
        let style = result.elements["transcript.heading.1"];
        assert_eq!(
            (style.fg, style.bg, style.underline_color),
            (
                Some(Color::LightRed),
                Some(Color::LightRed),
                Some(Color::LightRed)
            )
        );
        assert!(
            result.elements["transcript.selection"]
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }
}

#[test]
fn nonempty_no_color_and_config_none_strip_entire_catalogue_but_keep_attributes() {
    for depth in [
        detect::depth(
            &EnvView::from_pairs(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]),
            ColorsSetting::Auto,
        ),
        detect::depth(&EnvView::default(), ColorsSetting::None),
    ] {
        let result = resolve(depth, ColorValue::Rgb(12, 34, 56));
        assert_eq!(result.elements.len(), 323);
        for (token, style) in &result.elements {
            assert_eq!(
                (style.fg, style.bg, style.underline_color),
                (None, None, None),
                "{token}"
            );
        }
        assert!(
            result.elements["transcript.heading.1"]
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(
            result.report.unwrap().sources["transcript.heading.1"]["fg"],
            "constraints"
        );
        assert!(!result.theme.color);
        let concrete = theme::surfaces::bound_styles(&result.theme);
        assert_eq!(concrete.len(), 42);
        for (field, style) in concrete {
            assert_eq!(
                (style.fg, style.bg, style.underline_color),
                (None, None, None),
                "{field}"
            );
        }
        assert_eq!(result.theme.motion, Motion::Reduced);
    }
}

#[test]
fn empty_no_color_does_not_disable_authored_colors() {
    let depth = detect::depth(
        &EnvView::from_pairs(&[("NO_COLOR", ""), ("COLORTERM", "truecolor")]),
        ColorsSetting::Auto,
    );
    let result = resolve(depth, ColorValue::Rgb(12, 34, 56));
    assert_eq!(
        result.elements["transcript.heading.1"].fg,
        Some(Color::Rgb(12, 34, 56))
    );
}
