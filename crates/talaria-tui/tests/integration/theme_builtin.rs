//! The three built-in themes, pinned before their files exist.
use ratatui::style::{Color, Modifier};
use talaria_app::theme_file::{
    Base, BorderPreset, ColorValue, DividerPreset, StyleFields, ThemeFile, builtin, catalogue,
};
use talaria_core::appearance::Rgb;
use talaria_tui::theme::color::{ansi_color, to_ansi16};
use talaria_tui::theme::detect::{self, EnvView};
use talaria_tui::theme::{
    self, ColorDepth, ColorsSetting, DepthSource, Motion, MotionSource, Polarity, PolaritySource,
    Skin, TerminalColors, ThemeInputs, contrast_ratio, to_ansi256,
};

const EMBER: &[(&str, u8, u8, u8)] = &[
    ("background", 0x17, 0x14, 0x11),
    ("surface", 0x25, 0x20, 0x1b),
    ("foreground", 0xf5, 0xea, 0xdb),
    ("muted", 0xc4, 0xb5, 0xa2),
    ("disabled", 0xa1, 0x93, 0x83),
    ("border", 0x9c, 0x84, 0x6b),
    ("primary", 0xff, 0xc0, 0x78),
    ("accent", 0xff, 0xb8, 0x6b),
    ("success", 0xa8, 0xd0, 0x8d),
    ("warning", 0xff, 0xd1, 0x66),
    ("error", 0xff, 0x8b, 0x82),
    ("attention", 0xff, 0xd1, 0x66),
];

const PAPER_EXCEPT_DISABLED: &[(&str, u8, u8, u8)] = &[
    ("background", 0xff, 0xfc, 0xf5),
    ("surface", 0xee, 0xe8, 0xdc),
    ("foreground", 0x25, 0x23, 0x1f),
    ("muted", 0x66, 0x5d, 0x50),
    ("border", 0x89, 0x7a, 0x65),
    ("primary", 0x71, 0x40, 0x00),
    ("accent", 0x00, 0x5a, 0x80),
    ("success", 0x28, 0x5c, 0x2a),
    ("warning", 0x70, 0x46, 0x00),
    ("error", 0xa0, 0x18, 0x24),
    ("attention", 0x70, 0x46, 0x00),
];

fn builtin_theme(id: &str) -> ThemeFile {
    builtin(id).unwrap_or_else(|| panic!("{id} built-in is not available"))
}

fn inputs(depth: ColorDepth, follow: bool, skin: Option<&Skin>) -> ThemeInputs<'_> {
    ThemeInputs {
        skin,
        skipped: &[],
        follow_hermes_skin: follow,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (depth, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors::default(),
    }
}

fn rgb(file: &ThemeFile, slot: &str) -> (u8, u8, u8) {
    match file.palette.get(slot) {
        Some(ColorValue::Rgb(r, g, b)) => (*r, *g, *b),
        other => panic!("{slot} should be an rgb colour, got {other:?}"),
    }
}

fn palette_ref(fields: &StyleFields, field: &str) -> String {
    let value = match field {
        "fg" => fields.fg.as_ref(),
        "bg" => fields.bg.as_ref(),
        _ => None,
    };
    match value {
        Some(ColorValue::Palette(slot)) => slot.clone(),
        other => panic!("{field} should reference a palette slot, got {other:?}"),
    }
}

fn study_roles(file: &ThemeFile) {
    for (role, slot) in [
        ("background", "foreground"),
        ("surface", "foreground"),
        ("text", "foreground"),
        ("muted", "muted"),
        ("disabled", "disabled"),
        ("border", "border"),
        ("primary", "primary"),
        ("accent", "accent"),
        ("success", "success"),
        ("warning", "warning"),
        ("error", "error"),
        ("attention", "attention"),
        ("selection", "background"),
        ("focus", "background"),
    ] {
        let fields = file
            .roles
            .get(role)
            .unwrap_or_else(|| panic!("missing role {role}"));
        assert_eq!(palette_ref(fields, "fg"), slot, "{role}.fg");
    }
    for (role, slot) in [
        ("background", "background"),
        ("surface", "surface"),
        ("selection", "accent"),
        ("focus", "accent"),
    ] {
        let fields = &file.roles[role];
        assert_eq!(palette_ref(fields, "bg"), slot, "{role}.bg");
    }
}

fn study_chrome(file: &ThemeFile) {
    assert_eq!(file.chrome.border, BorderPreset::Rounded);
    assert_eq!(file.chrome.section_divider, DividerPreset::Single);
}

fn style_for<'a>(
    resolved: &'a theme::ThemeFileResolution,
    token: &str,
) -> &'a ratatui::style::Style {
    resolved
        .elements
        .get(token)
        .unwrap_or_else(|| panic!("missing style for {token}"))
}

#[test]
fn ember_palette_is_the_studys() {
    let file = builtin_theme("ember");
    assert_eq!(file.name.as_deref(), Some("Ember"));
    assert_eq!(file.extends, Base::Dark);
    assert!(!file.follow_hermes_skin);
    study_chrome(&file);
    assert_eq!(file.palette.len(), EMBER.len());
    for (slot, r, g, b) in EMBER {
        assert_eq!(rgb(&file, slot), (*r, *g, *b), "{slot}");
    }
    study_roles(&file);
}

#[test]
fn paper_palette_is_the_studys_except_disabled() {
    let file = builtin_theme("paper");
    assert_eq!(file.name.as_deref(), Some("Paper"));
    assert_eq!(file.extends, Base::Light);
    assert!(!file.follow_hermes_skin);
    study_chrome(&file);
    assert_eq!(file.palette.len(), 12);
    for (slot, r, g, b) in PAPER_EXCEPT_DISABLED {
        assert_eq!(rgb(&file, slot), (*r, *g, *b), "{slot}");
    }
    assert_ne!(
        rgb(&file, "disabled"),
        (0x76, 0x6a, 0x58),
        "the study disabled colour is below 4.5:1 on the surface"
    );
    study_roles(&file);
}

#[test]
fn paper_disabled_reaches_4_5() {
    let file = builtin_theme("paper");
    let (r, g, b) = rgb(&file, "disabled");
    let disabled = Rgb(r, g, b);
    let surface = Rgb(0xee, 0xe8, 0xdc);
    let background = Rgb(0xff, 0xfc, 0xf5);
    let (mr, mg, mb) = rgb(&file, "muted");
    let muted = Rgb(mr, mg, mb);
    assert!(
        contrast_ratio(disabled, surface) >= 4.5,
        "disabled on surface {:0.2}:1",
        contrast_ratio(disabled, surface)
    );
    assert!(
        contrast_ratio(disabled, background) >= 4.5,
        "disabled on background {:0.2}:1",
        contrast_ratio(disabled, background)
    );
    assert!(
        contrast_ratio(disabled, surface) < contrast_ratio(muted, surface),
        "disabled should stay dimmer than muted on the surface"
    );
}

#[test]
fn no_contrast_warnings() {
    for id in ["ember", "paper"] {
        let file = builtin_theme(id);
        let resolved =
            theme::resolve_with_theme_file(&inputs(ColorDepth::TrueColor, true, None), Some(&file));
        let report = resolved.report.expect("selected theme has a report");
        assert!(
            report.warnings.is_empty(),
            "{id} warnings: {:?}",
            report.warnings
        );
    }
}

#[test]
fn every_token_resolves() {
    assert_eq!(catalogue().len(), 323);
    for id in ["terminal", "ember", "paper"] {
        let file = builtin_theme(id);
        let resolved = theme::resolve_with_theme_file(
            &inputs(ColorDepth::TrueColor, file.follow_hermes_skin, None),
            Some(&file),
        );
        assert_eq!(resolved.elements.len(), 323, "{id}");
        for entry in catalogue() {
            assert!(
                resolved.elements.contains_key(entry.token),
                "{id} left {} without a style",
                entry.token
            );
        }
    }
}

#[test]
fn depth_and_colour_off() {
    let ember = builtin_theme("ember");
    let paper = builtin_theme("paper");
    let terminal = builtin_theme("terminal");
    let primary = Rgb(0xff, 0xc0, 0x78);
    let at = |file: &ThemeFile, depth: ColorDepth| {
        theme::resolve_with_theme_file(&inputs(depth, file.follow_hermes_skin, None), Some(file))
    };

    let ember_256 = at(&ember, ColorDepth::Ansi256);
    assert_eq!(
        style_for(&ember_256, "transcript.heading.1").fg,
        Some(Color::Indexed(to_ansi256(primary)))
    );
    let paper_primary = {
        let (r, g, b) = rgb(&paper, "primary");
        Rgb(r, g, b)
    };
    let paper_256 = at(&paper, ColorDepth::Ansi256);
    assert_eq!(
        style_for(&paper_256, "transcript.heading.1").fg,
        Some(Color::Indexed(to_ansi256(paper_primary)))
    );
    assert_eq!(
        style_for(&at(&terminal, ColorDepth::Ansi256), "transcript.heading.1").fg,
        Some(ansi_color(3))
    );

    for file in [&ember, &paper, &terminal] {
        let resolved = at(file, ColorDepth::Ansi16);
        let heading = style_for(&resolved, "transcript.heading.1");
        if file.extends != Base::Terminal {
            let (r, g, b) = rgb(file, "primary");
            assert_eq!(heading.fg, Some(ansi_color(to_ansi16(Rgb(r, g, b)))));
        }
        let selection = style_for(&resolved, "transcript.selection");
        assert!(
            selection.add_modifier.contains(Modifier::REVERSED),
            "16-colour selection uses reverse video"
        );
        let wide = at(file, ColorDepth::Ansi256);
        if file.extends != Base::Terminal {
            assert!(
                !style_for(&wide, "transcript.selection")
                    .add_modifier
                    .contains(Modifier::REVERSED),
                "256-colour selection keeps its colour pair"
            );
        }
    }

    let env = EnvView::from_pairs(&[("NO_COLOR", "1"), ("TERM", "xterm-256color")]);
    let (depth, source) = detect::depth(&env, ColorsSetting::Auto);
    assert_eq!((depth, source), (ColorDepth::None, DepthSource::NoColorEnv));
    for (id, file) in [
        ("ember", &ember),
        ("paper", &paper),
        ("terminal", &terminal),
    ] {
        let resolved = at(file, depth);
        for (token, style) in &resolved.elements {
            assert_eq!(style.fg, None, "{id} {token} kept a foreground");
            assert_eq!(style.bg, None, "{id} {token} kept a background");
            assert_eq!(
                style.underline_color, None,
                "{id} {token} kept an underline colour"
            );
        }
        let heading = style_for(&resolved, "transcript.heading.1");
        assert!(heading.add_modifier.contains(Modifier::BOLD));
        let reasoning = style_for(&resolved, "transcript.reasoning");
        assert!(reasoning.add_modifier.contains(Modifier::ITALIC));
    }
}

#[test]
fn terminal_uses_default_background_and_named_accents() {
    let file = builtin_theme("terminal");
    assert_eq!(file.name.as_deref(), Some("Terminal"));
    assert_eq!(file.extends, Base::Terminal);
    assert!(file.follow_hermes_skin);
    study_chrome(&file);
    study_roles(&file);
    for (slot, value) in [
        ("background", ColorValue::Default),
        ("surface", ColorValue::Default),
        ("foreground", ColorValue::Default),
        ("muted", ColorValue::Ansi(7)),
        ("disabled", ColorValue::Ansi(8)),
        ("border", ColorValue::Ansi(7)),
        ("primary", ColorValue::Ansi(3)),
        ("accent", ColorValue::Ansi(6)),
        ("success", ColorValue::Ansi(2)),
        ("warning", ColorValue::Ansi(3)),
        ("error", ColorValue::Ansi(1)),
        ("attention", ColorValue::Ansi(3)),
    ] {
        assert_eq!(file.palette.get(slot), Some(&value), "{slot}");
    }
    for role in ["selection", "focus"] {
        assert_eq!(file.roles[role].reverse, Some(true), "{role}");
    }
    let skin = Skin {
        name: "fixture".into(),
        colors: [("ui_accent".into(), "#ff00aa".into())].into(),
        light_colors: Default::default(),
        dark_colors: Default::default(),
    };
    let followed = theme::resolve_with_theme_file(
        &inputs(ColorDepth::TrueColor, true, Some(&skin)),
        Some(&file),
    );
    let ignored = theme::resolve_with_theme_file(
        &inputs(ColorDepth::TrueColor, false, Some(&skin)),
        Some(&file),
    );
    assert!(followed.report.unwrap().skin_participates);
    assert!(!ignored.report.unwrap().skin_participates);
    let resolved =
        theme::resolve_with_theme_file(&inputs(ColorDepth::TrueColor, true, None), Some(&file));
    let text = style_for(&resolved, "transcript.text");
    assert_eq!(text.fg, Some(Color::Reset));
    assert_eq!(text.bg, Some(Color::Reset));
    let selection = style_for(&resolved, "transcript.selection");
    assert!(selection.add_modifier.contains(Modifier::REVERSED));
}
