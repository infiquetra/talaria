//! Contrast warnings describe authored values without repairing them.
use ratatui::style::Color;
use std::collections::BTreeMap;
use talaria_app::theme_file::{Base, Chrome, ColorValue, StyleFields, ThemeFile};
use talaria_tui::theme::{self, *};

fn file(base: Base) -> ThemeFile {
    ThemeFile {
        name: None,
        extends: base,
        follow_hermes_skin: false,
        palette: BTreeMap::new(),
        roles: BTreeMap::new(),
        elements: BTreeMap::new(),
        chrome: Chrome::default(),
    }
}
fn resolve(file: &ThemeFile, background: Option<Rgb>) -> theme::ThemeFileResolution {
    theme::resolve_with_theme_file(
        &ThemeInputs {
            skin: None,
            skipped: &[],
            follow_hermes_skin: true,
            polarity: (Polarity::Dark, PolaritySource::Config),
            depth: (ColorDepth::TrueColor, DepthSource::Config),
            motion: (Motion::Full, MotionSource::Default),
            terminal: TerminalColors {
                background,
                foreground: None,
            },
        },
        Some(file),
    )
}

#[test]
fn contrast_math_matches_known_black_white_and_gray_pairs() {
    assert!((theme::contrast_ratio(Rgb(0, 0, 0), Rgb(255, 255, 255)) - 21.0).abs() < 0.0001);
    assert!((theme::contrast_ratio(Rgb(255, 255, 255), Rgb(255, 255, 255)) - 1.0).abs() < 0.0001);
    assert!(theme::contrast_ratio(Rgb(118, 118, 118), Rgb(255, 255, 255)) > 4.5);
    assert!(theme::contrast_ratio(Rgb(119, 119, 119), Rgb(255, 255, 255)) < 4.5);
}

#[test]
fn text_and_border_thresholds_apply_to_token_names_without_rewriting() {
    let mut file = file(Base::Light);
    for (token, gray) in [
        ("transcript.heading.1", 119),
        ("transcript.heading.2", 118),
        ("transcript.rule", 149),
        ("transcript.table.border", 148),
    ] {
        file.elements.insert(
            token.into(),
            StyleFields {
                fg: Some(ColorValue::Rgb(gray, gray, gray)),
                ..StyleFields::default()
            },
        );
    }
    let result = resolve(&file, Some(Rgb(255, 255, 255)));
    let warnings = &result.report.unwrap().warnings;
    for token in ["transcript.heading.1", "transcript.rule"] {
        assert!(
            warnings.iter().any(|warning| warning.contains(token)),
            "missing {token}: {warnings:?}"
        );
    }
    for token in ["transcript.heading.2", "transcript.table.border"] {
        assert!(
            !warnings.iter().any(|warning| warning.contains(token)),
            "unexpected {token}: {warnings:?}"
        );
    }
    assert_eq!(
        result.elements["transcript.heading.1"].fg,
        Some(Color::Rgb(119, 119, 119))
    );
    assert_eq!(
        result.elements["transcript.rule"].fg,
        Some(Color::Rgb(149, 149, 149))
    );
}

#[test]
fn inline_without_trusted_probe_is_unknown_even_for_an_explicit_base() {
    for background in [None, Some(Rgb(0, 0, 0))] {
        let result = resolve(&file(Base::Light), background);
        let report = result.report.unwrap();
        assert!(
            report
                .unknown_contrast
                .iter()
                .any(|token| token == "transcript.heading.1")
        );
        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.contains("transcript.heading.1"))
        );
    }
}

#[test]
fn inline_uses_real_probe_background_instead_of_the_authored_light_background() {
    let result = resolve(&file(Base::Light), Some(Rgb(16, 16, 20)));
    let on_dark = TerminalColors {
        background: Some(Rgb(16, 16, 20)),
        foreground: None,
    };
    let on_light = TerminalColors {
        background: Some(Rgb(255, 255, 255)),
        foreground: None,
    };
    let (canvas, native) = result
        .inline_canvas(on_dark)
        .expect("light viewport on dark terminal");
    assert_eq!(canvas.bg, Some(Color::Rgb(255, 255, 255)));
    assert_eq!(native, Rgb(16, 16, 20));
    assert!(result.inline_canvas(on_light).is_none());
    assert!(
        resolve(&file(Base::Dark), on_light.background)
            .inline_canvas(on_light)
            .is_some()
    );
    assert!(
        result
            .report
            .unwrap()
            .warnings
            .iter()
            .any(|warning| warning.contains("transcript.text"))
    );
    assert_eq!(
        result.elements["transcript.text"].fg,
        Some(Color::Rgb(22, 22, 22))
    );
    assert_eq!(
        result.elements["transcript.text"].bg,
        Some(Color::Rgb(255, 255, 255))
    );
}

#[test]
fn dark_selection_fill_preserves_light_ink_in_an_inline_picker() {
    let mut file = file(Base::Dark);
    file.elements.insert(
        "list.selected".into(),
        StyleFields {
            fg: Some(ColorValue::Rgb(255, 255, 255)),
            bg: Some(ColorValue::Rgb(16, 16, 20)),
            ..StyleFields::default()
        },
    );
    let resolved = resolve(&file, Some(Rgb(16, 16, 20)));
    let selection = resolved.theme.views.inline_selection(
        theme::surfaces::tokens::LIST_SELECTED,
        ratatui::style::Style::default(),
    );
    assert_eq!(selection.fg, Some(Color::Rgb(255, 255, 255)));
    assert_eq!(selection.bg, None);
    assert_eq!(resolved.report.unwrap().warnings, Vec::<String>::new());
}

#[test]
fn terminal_inline_selection_inverts_the_emulators_own_colours() {
    use ratatui::style::{Modifier, Style};
    use talaria_tui::theme::surfaces::tokens;

    let terminal = talaria_app::theme_file::builtin("terminal").unwrap();
    let resolved = resolve(&terminal, None);
    for token in [tokens::LIST_SELECTED, tokens::DIALOG_CLARIFICATION_SELECTED] {
        let selected = resolved
            .theme
            .views
            .inline_selection(token, Style::default());
        assert!(
            selected
                .add_modifier
                .contains(Modifier::REVERSED | Modifier::UNDERLINED)
        );
        assert_eq!(selected.bg, None);
    }
}

#[test]
fn complete_dark_and_light_bases_have_no_measurable_contrast_failures() {
    for (base, background) in [
        (Base::Dark, Rgb(16, 16, 20)),
        (Base::Light, Rgb(255, 255, 255)),
    ] {
        let result = resolve(&file(base), Some(background));
        assert_eq!(
            result.elements.len(),
            talaria_app::theme_file::catalogue().len()
        );
        assert!(result.report.unwrap().warnings.is_empty());
    }
}

#[test]
fn ansi_and_terminal_default_pairs_are_unknown_and_reserved_tokens_warn() {
    let mut file = file(Base::Terminal);
    file.elements.insert(
        "picker.row.match".into(),
        StyleFields {
            bold: Some(true),
            ..StyleFields::default()
        },
    );
    let report = resolve(&file, None).report.unwrap();
    assert!(
        report
            .unknown_contrast
            .iter()
            .any(|token| token == "transcript.text")
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("picker.row.match") && warning.contains("reserved"))
    );
}
