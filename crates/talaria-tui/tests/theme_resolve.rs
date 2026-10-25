//! Resolving a skin into Talaria's styles. Behavior contract: issue 227.

use std::path::PathBuf;

use ratatui::style::{Color, Modifier, Style};
use serde_json::{Value, json};
use talaria_core::appearance::{NotAppliedReason, Skin, SkinMap, SkippedValue};
use talaria_tui::markdown::MarkdownTheme;
use talaria_tui::status::{StatusPalette, Tone};
use talaria_tui::theme::{
    ColorDepth, ComposerStyle, DepthSource, Motion, MotionSource, Polarity, PolaritySource, Rgb,
    TerminalColors, Theme, ThemeInputs,
};

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb(r, g, b)
}

fn terminal(background: Option<Rgb>, foreground: Option<Rgb>) -> TerminalColors {
    TerminalColors {
        background,
        foreground,
    }
}

fn skin_data(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/skins")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn skin_of(name: &str) -> (Skin, Vec<SkippedValue>) {
    let (skin, skipped) = Skin::from_value(&skin_data(name));
    (skin.expect("the fixture is a skin"), skipped)
}

fn dark_terminal() -> TerminalColors {
    terminal(Some(rgb(0x1e, 0x1e, 0x1e)), None)
}

fn light_terminal() -> TerminalColors {
    terminal(Some(rgb(0xff, 0xff, 0xff)), None)
}

#[allow(clippy::too_many_arguments)]
fn inputs<'a>(
    skin: Option<&'a Skin>,
    skipped: &'a [SkippedValue],
    follow_hermes_skin: bool,
    polarity: (Polarity, PolaritySource),
    depth: (ColorDepth, DepthSource),
    motion: (Motion, MotionSource),
    terminal: TerminalColors,
) -> ThemeInputs<'a> {
    ThemeInputs {
        skin,
        skipped,
        follow_hermes_skin,
        polarity,
        depth,
        motion,
        terminal,
    }
}

fn dark_truecolor<'a>(skin: Option<&'a Skin>, skipped: &'a [SkippedValue]) -> ThemeInputs<'a> {
    inputs(
        skin,
        skipped,
        true,
        (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
        ),
        (ColorDepth::TrueColor, DepthSource::Colorterm),
        (Motion::Full, MotionSource::Default),
        dark_terminal(),
    )
}

fn assert_markdown_default(theme: &MarkdownTheme) {
    let default = MarkdownTheme::default();
    assert_eq!(theme.heading1, default.heading1);
    assert_eq!(theme.heading2, default.heading2);
    assert_eq!(theme.heading3, default.heading3);
    assert_eq!(theme.strong, default.strong);
    assert_eq!(theme.emphasis, default.emphasis);
    assert_eq!(theme.strikethrough, default.strikethrough);
    assert_eq!(theme.inline_code, default.inline_code);
    assert_eq!(theme.link_text, default.link_text);
    assert_eq!(theme.link_destination, default.link_destination);
    assert_eq!(theme.image_alt, default.image_alt);
    assert_eq!(theme.quote_bar, default.quote_bar);
    assert_eq!(theme.rule, default.rule);
    assert_eq!(theme.code_label, default.code_label);
    assert_eq!(theme.table_border, default.table_border);
    assert_eq!(theme.table_header, default.table_header);
}

fn assert_palette_default(palette: &talaria_tui::status::StatusPalette) {
    let default = StatusPalette::default();
    for tone in [
        Tone::Text,
        Tone::Muted,
        Tone::Separator,
        Tone::Success,
        Tone::Warning,
        Tone::Error,
        Tone::Attention,
    ] {
        assert_eq!(palette.style(tone), default.style(tone), "{tone:?}");
    }
}

fn assert_phase_one(theme: &Theme) {
    assert_markdown_default(&theme.markdown);
    assert_palette_default(&theme.status);
    assert_eq!(theme.composer, ComposerStyle::default());
    assert_eq!(theme.reasoning, Style::default());
    assert_eq!(theme.tool, talaria_tui::tool_cell::ToolTheme::default());
    assert!(theme.color);
}

#[test]
fn no_skin_gives_phase_one_defaults_exactly() {
    let theme = talaria_tui::theme::resolve(&dark_truecolor(None, &[]));
    assert_phase_one(&theme);
    assert_eq!(theme.motion, Motion::Full);
    assert_eq!(theme.report.skin, None);
    assert!(theme.report.ignored.is_empty());
}

#[test]
fn default_skin_overlay_follows_polarity() {
    let (skin, skipped) = skin_of("default.json");
    assert!(skipped.is_empty());

    let theme = talaria_tui::theme::resolve(&dark_truecolor(Some(&skin), &skipped));
    assert_eq!(
        theme.markdown.link_text.fg,
        Some(Color::Rgb(0xff, 0xbf, 0x00))
    );
    assert_eq!(theme.composer.border.fg, Some(Color::Rgb(0xcd, 0x7f, 0x32)));
    assert_eq!(
        theme.markdown.table_border.fg,
        Some(Color::Rgb(0xcd, 0x7f, 0x32))
    );

    let inputs = inputs(
        Some(&skin),
        &skipped,
        true,
        (
            Polarity::Light,
            PolaritySource::TerminalBackground(rgb(0xff, 0xff, 0xff)),
        ),
        (ColorDepth::TrueColor, DepthSource::Colorterm),
        (Motion::Full, MotionSource::Default),
        light_terminal(),
    );
    let theme = talaria_tui::theme::resolve(&inputs);
    assert_eq!(
        theme.markdown.link_text.fg,
        Some(Color::Rgb(0xd8, 0x9b, 0x04))
    );
    assert_eq!(
        theme.status.style(Tone::Text).fg,
        Some(Color::Rgb(0x6f, 0x6f, 0x6f))
    );
    assert_eq!(theme.composer.border.fg, Some(Color::Rgb(0xcd, 0x7f, 0x32)));
}

#[test]
fn floors_follow_role_kind_and_reference_background() {
    let mut data = skin_data("ares.json");
    data["colors"]["status_bar_good"] = json!("#F1E6CF");
    data["colors"]["prompt"] = json!("#F1E6CF");
    let (skin, skipped) = Skin::from_value(&data);
    let skin = skin.unwrap();
    assert!(skipped.is_empty());

    // Light by override while the probe saw dark: the reference is white.
    let inputs = inputs(
        Some(&skin),
        &skipped,
        true,
        (Polarity::Light, PolaritySource::Command),
        (ColorDepth::TrueColor, DepthSource::Colorterm),
        (Motion::Full, MotionSource::Default),
        dark_terminal(),
    );
    let theme = talaria_tui::theme::resolve(&inputs);
    assert_eq!(
        theme.composer.marker.fg,
        Some(Color::Rgb(0xf1, 0xe6, 0xcf)),
        "a display role keeps its colour"
    );
    assert_eq!(
        theme.status.style(Tone::Success).fg,
        Some(Color::Rgb(0xc2, 0xba, 0xa7)),
        "a semantic role is lifted"
    );
}

#[test]
fn role_key_chains_fall_back_in_order() {
    let (skin, skipped) =
        Skin::from_value(&json!({"name": "x", "colors": {"banner_accent": "#123456"}}));
    let skin = skin.unwrap();
    assert!(skipped.is_empty());
    // #123456 is below the dark display floor on #1e1e1e (about 1.3:1) and would be lifted, so
    // this fallback case reads it on a light terminal, where it clears the floor unchanged.
    let theme = talaria_tui::theme::resolve(&inputs(
        Some(&skin),
        &[],
        true,
        (
            Polarity::Light,
            PolaritySource::TerminalBackground(rgb(0xff, 0xff, 0xff)),
        ),
        (ColorDepth::TrueColor, DepthSource::Colorterm),
        (Motion::Full, MotionSource::Default),
        light_terminal(),
    ));
    assert_eq!(
        theme.markdown.link_text.fg,
        Some(Color::Rgb(0x12, 0x34, 0x56))
    );
    assert!(theme.report.ignored.is_empty());

    let (skin, skipped) = Skin::from_value(&json!({
        "name": "x",
        "colors": {"ui_accent": "red", "banner_accent": "#654321"},
    }));
    let skin = skin.unwrap();
    assert!(skipped.is_empty());
    let theme = talaria_tui::theme::resolve(&dark_truecolor(Some(&skin), &[]));
    assert_eq!(
        theme.markdown.link_text.fg,
        Some(Color::Rgb(0x65, 0x43, 0x21))
    );
    assert_eq!(theme.report.ignored, ["ui_accent"]);

    let (skin, _) = Skin::from_value(&json!({"name": "x", "colors": {}}));
    let skin = skin.unwrap();
    let skipped = [
        SkippedValue {
            map: SkinMap::Colors,
            key: "ui_accent".to_owned(),
        },
        SkippedValue {
            map: SkinMap::Colors,
            key: "completion_menu_bg".to_owned(),
        },
    ];
    let theme = talaria_tui::theme::resolve(&dark_truecolor(Some(&skin), &skipped));
    assert_eq!(
        theme.report.ignored,
        ["ui_accent"],
        "keys outside every chain are never counted"
    );

    let (skin, skipped) = Skin::from_value(&json!({
        "name": "x",
        "colors": {"ui_ok": null},
        "light_colors": {"ui_ok": "#4caf50"},
    }));
    let skin = skin.unwrap();
    assert_eq!(skipped.len(), 1);
    let inputs = inputs(
        Some(&skin),
        &skipped,
        true,
        (
            Polarity::Light,
            PolaritySource::TerminalBackground(rgb(0xff, 0xff, 0xff)),
        ),
        (ColorDepth::TrueColor, DepthSource::Colorterm),
        (Motion::Full, MotionSource::Default),
        light_terminal(),
    );
    let theme = talaria_tui::theme::resolve(&inputs);
    assert!(
        theme.report.ignored.is_empty(),
        "the overlay supplies the key: {:?}",
        theme.report.ignored
    );
    assert_eq!(
        theme.status.style(Tone::Success).fg,
        Some(Color::Rgb(0x4c, 0xaf, 0x50))
    );
}

#[test]
fn sixteen_colour_terminal_ignores_skin() {
    let (skin, skipped) = skin_of("ares.json");
    let inputs = inputs(
        Some(&skin),
        &skipped,
        true,
        (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
        ),
        (ColorDepth::Ansi16, DepthSource::Default),
        (Motion::Full, MotionSource::Default),
        dark_terminal(),
    );
    let theme = talaria_tui::theme::resolve(&inputs);
    assert_phase_one(&theme);
    assert_eq!(theme.report.skin.as_deref(), Some("ares"));
    assert_eq!(
        theme.report.not_applied,
        Some(NotAppliedReason::SixteenColours)
    );
}

#[test]
fn no_color_removes_colour_keeps_modifiers() {
    let (skin, skipped) = skin_of("ares.json");
    let inputs = inputs(
        Some(&skin),
        &skipped,
        true,
        (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
        ),
        (ColorDepth::None, DepthSource::NoColorEnv),
        (Motion::Full, MotionSource::Default),
        dark_terminal(),
    );
    let theme = talaria_tui::theme::resolve(&inputs);
    assert!(!theme.color);
    for tone in [
        Tone::Text,
        Tone::Muted,
        Tone::Separator,
        Tone::Success,
        Tone::Warning,
        Tone::Error,
        Tone::Attention,
    ] {
        let style = theme.status.style(tone);
        assert_eq!(
            (style.fg, style.bg, style.underline_color),
            (None, None, None)
        );
    }
    for style in [
        theme.composer.border,
        theme.composer.marker,
        theme.reasoning,
    ] {
        assert_eq!(
            (style.fg, style.bg, style.underline_color),
            (None, None, None)
        );
    }
    assert!(
        theme
            .markdown
            .heading1
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(theme.markdown.heading1.fg, None);
    assert_eq!(theme.report.skin.as_deref(), Some("ares"));
    assert_eq!(theme.report.not_applied, Some(NotAppliedReason::ColoursOff));
}

#[test]
fn no_style_blinks_and_motion_is_reported() {
    for name in ["default.json", "ares.json"] {
        let (skin, skipped) = skin_of(name);
        for motion in [Motion::Full, Motion::Reduced] {
            let inputs = inputs(
                Some(&skin),
                &skipped,
                true,
                (
                    Polarity::Dark,
                    PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
                ),
                (ColorDepth::TrueColor, DepthSource::Colorterm),
                (motion, MotionSource::Default),
                dark_terminal(),
            );
            let theme = talaria_tui::theme::resolve(&inputs);
            assert_eq!(theme.motion, motion);
            assert_eq!(theme.report.motion.0, motion);
            assert_eq!(theme.report.motion.1, MotionSource::Default);
            let markdown = &theme.markdown;
            let styles = [
                markdown.heading1,
                markdown.heading2,
                markdown.heading3,
                markdown.strong,
                markdown.emphasis,
                markdown.strikethrough,
                markdown.inline_code,
                markdown.link_text,
                markdown.link_destination,
                markdown.image_alt,
                markdown.quote_bar,
                markdown.rule,
                markdown.code_label,
                markdown.table_border,
                markdown.table_header,
                theme.composer.border,
                theme.composer.marker,
                theme.reasoning,
            ];
            for style in styles {
                assert!(
                    !style
                        .add_modifier
                        .intersects(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK),
                    "{name:?} {motion:?}: no emitted style blinks"
                );
            }
            for tone in [
                Tone::Text,
                Tone::Muted,
                Tone::Separator,
                Tone::Success,
                Tone::Warning,
                Tone::Error,
                Tone::Attention,
            ] {
                assert!(
                    !theme
                        .status
                        .style(tone)
                        .add_modifier
                        .intersects(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK)
                );
            }
        }
    }
}

#[test]
fn follow_hermes_skin_false_uses_defaults() {
    let (skin, skipped) = skin_of("ares.json");
    let mut dark = dark_truecolor(Some(&skin), &skipped);
    dark.follow_hermes_skin = false;
    let theme = talaria_tui::theme::resolve(&dark);
    assert_phase_one(&theme);
    assert_eq!(theme.report.skin.as_deref(), Some("ares"));
    assert_eq!(theme.report.not_applied, Some(NotAppliedReason::FollowOff));
}
