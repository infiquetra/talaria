//! Terminal detection: colour replies, polarity, depth and motion.
//!
//! Behavior contract: issue 227.

use talaria_tui::theme::detect::{
    ColorsSetting, EnvView, ThemeMode, depth, env_notices, motion, parse_osc_color, polarity,
};
use talaria_tui::theme::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, Rgb, TerminalColors,
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

#[test]
fn parse_osc_color_reply_forms() {
    let white = rgb(255, 255, 255);
    assert_eq!(
        parse_osc_color(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\"),
        Some((11, white))
    );
    assert_eq!(
        parse_osc_color(b"\x1b]11;rgb:1e/1e/1e\x07"),
        Some((11, rgb(0x1e, 0x1e, 0x1e)))
    );
    assert_eq!(
        parse_osc_color(b"\x1b]10;rgb:f/0/0\x1b\\"),
        Some((10, rgb(255, 0, 0)))
    );
    assert_eq!(
        parse_osc_color(b"\x1b]11;rgb:ff/0000/f\x07"),
        Some((11, rgb(255, 0, 255)))
    );
    assert_eq!(
        parse_osc_color(b"\x1b]11;#102030\x1b\\"),
        Some((11, rgb(0x10, 0x20, 0x30)))
    );
    assert_eq!(parse_osc_color(b"\x1b]11;rgba:0/0/0/0\x1b\\"), None);
    assert_eq!(parse_osc_color(b"\x1b]11;rgb:ff"), None);
    assert_eq!(parse_osc_color(b""), None);
}

#[test]
fn polarity_precedence_order() {
    let dark_bg = terminal(Some(rgb(0x1e, 0x1e, 0x1e)), None);
    let light_bg = terminal(Some(rgb(0xf0, 0xf0, 0xf0)), None);
    let no_colors = terminal(None, None);

    // Each row beats a contrary lower one.
    let env = EnvView::from_pairs(&[("TALARIA_THEME", "dark")]);
    assert_eq!(
        polarity(Some(Polarity::Light), &env, ThemeMode::Dark, &dark_bg),
        (Polarity::Light, PolaritySource::Command)
    );
    let env = EnvView::from_pairs(&[("TALARIA_THEME", "LiGhT")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Dark, &dark_bg),
        (Polarity::Light, PolaritySource::Environment)
    );
    let env = EnvView::from_pairs(&[("TALARIA_THEME", "  dark  ")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Light, &light_bg),
        (Polarity::Dark, PolaritySource::Environment)
    );
    for value in ["auto", "", "AUTO"] {
        let env = EnvView::from_pairs(&[("TALARIA_THEME", value)]);
        assert_eq!(
            polarity(None, &env, ThemeMode::Light, &dark_bg),
            (Polarity::Light, PolaritySource::Config),
            "{value:?} falls through to the config"
        );
        assert!(env_notices(&env).is_empty());
    }
    let env = EnvView::from_pairs(&[("TALARIA_THEME", "purple")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Light, &dark_bg),
        (Polarity::Light, PolaritySource::Config),
        "an invalid value falls through"
    );
    assert_eq!(
        env_notices(&env),
        ["TALARIA_THEME must be light, dark or auto; ignoring it."]
    );

    assert_eq!(
        polarity(None, &EnvView::from_pairs(&[]), ThemeMode::Dark, &light_bg),
        (Polarity::Dark, PolaritySource::Config)
    );
    assert_eq!(
        polarity(None, &EnvView::from_pairs(&[]), ThemeMode::Auto, &dark_bg),
        (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e))
        )
    );

    // The probe background decides both ways over every later hint.
    let env = EnvView::from_pairs(&[("COLORFGBG", "0;15")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &dark_bg),
        (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e))
        )
    );
    let env = EnvView::from_pairs(&[("TERM_PROGRAM", "Apple_Terminal")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &dark_bg),
        (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e))
        )
    );
    assert_eq!(
        polarity(None, &EnvView::from_pairs(&[]), ThemeMode::Auto, &light_bg),
        (
            Polarity::Light,
            PolaritySource::TerminalBackground(rgb(0xf0, 0xf0, 0xf0))
        )
    );

    // Foreground, COLORFGBG, TERM_PROGRAM, default.
    let fg = terminal(None, Some(rgb(0xe0, 0xe0, 0xe0)));
    let env = EnvView::from_pairs(&[("COLORFGBG", "15;0")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &fg),
        (
            Polarity::Dark,
            PolaritySource::TerminalForeground(rgb(0xe0, 0xe0, 0xe0))
        )
    );
    let env = EnvView::from_pairs(&[("COLORFGBG", "0;15")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &no_colors),
        (Polarity::Light, PolaritySource::Colorfgbg)
    );
    let env = EnvView::from_pairs(&[("COLORFGBG", "15;0")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &no_colors),
        (Polarity::Dark, PolaritySource::Colorfgbg)
    );
    let env = EnvView::from_pairs(&[("COLORFGBG", "7")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &no_colors),
        (Polarity::Light, PolaritySource::Colorfgbg)
    );
    let env = EnvView::from_pairs(&[("COLORFGBG", "bogus"), ("TERM_PROGRAM", "Apple_Terminal")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &no_colors),
        (Polarity::Light, PolaritySource::TermProgram)
    );
    assert_eq!(
        polarity(None, &EnvView::from_pairs(&[]), ThemeMode::Auto, &no_colors),
        (Polarity::Dark, PolaritySource::Default)
    );

    let env = EnvView::from_pairs(&[("COLORFGBG", "15;0")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &light_bg),
        (
            Polarity::Light,
            PolaritySource::TerminalBackground(rgb(0xf0, 0xf0, 0xf0))
        )
    );

    // Each source reports its 9.2 label.
    for (source, label) in [
        (PolaritySource::Command, "set with /theme"),
        (PolaritySource::Environment, "TALARIA_THEME"),
        (PolaritySource::Config, "config.toml"),
        (
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
            "terminal background #1e1e1e",
        ),
        (
            PolaritySource::TerminalForeground(rgb(0xe0, 0xe0, 0xe0)),
            "terminal text colour #e0e0e0",
        ),
        (PolaritySource::Colorfgbg, "COLORFGBG"),
        (PolaritySource::TermProgram, "TERM_PROGRAM"),
        (PolaritySource::Default, "default (no signal)"),
    ] {
        assert_eq!(source.label(), label);
    }
}

#[test]
fn pure_black_background_is_distrusted() {
    let env = EnvView::from_pairs(&[]);
    let black = Some(rgb(0, 0, 0));
    let bright = terminal(black, Some(rgb(0xe0, 0xe0, 0xe0)));
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &bright),
        (
            Polarity::Dark,
            PolaritySource::TerminalForeground(rgb(0xe0, 0xe0, 0xe0))
        )
    );
    let dim = terminal(black, Some(rgb(0x20, 0x20, 0x20)));
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &dim),
        (
            Polarity::Light,
            PolaritySource::TerminalForeground(rgb(0x20, 0x20, 0x20))
        )
    );
    let white = terminal(black, Some(rgb(255, 255, 255)));
    let env = EnvView::from_pairs(&[("COLORFGBG", "0;15")]);
    assert_eq!(
        polarity(None, &env, ThemeMode::Auto, &white),
        (Polarity::Light, PolaritySource::Colorfgbg)
    );
}

#[test]
fn color_depth_and_motion_from_env_and_config() {
    let env = EnvView::from_pairs(&[("NO_COLOR", "1")]);
    assert_eq!(
        depth(&env, ColorsSetting::TrueColor),
        (ColorDepth::None, DepthSource::NoColorEnv)
    );
    let env = EnvView::from_pairs(&[("NO_COLOR", "")]);
    assert_eq!(
        depth(&env, ColorsSetting::Auto),
        (ColorDepth::Ansi16, DepthSource::Default),
        "an empty NO_COLOR is ignored"
    );
    let env = EnvView::from_pairs(&[("COLORTERM", "truecolor")]);
    assert_eq!(
        depth(&env, ColorsSetting::Ansi256),
        (ColorDepth::Ansi256, DepthSource::Config)
    );
    let env = EnvView::from_pairs(&[("TERM", "dumb"), ("COLORTERM", "truecolor")]);
    assert_eq!(
        depth(&env, ColorsSetting::Auto),
        (ColorDepth::None, DepthSource::TermDumb)
    );
    let env = EnvView::from_pairs(&[("COLORTERM", "24bit")]);
    assert_eq!(
        depth(&env, ColorsSetting::Auto),
        (ColorDepth::TrueColor, DepthSource::Colorterm)
    );
    let env = EnvView::from_pairs(&[("TERM", "xterm-256color")]);
    assert_eq!(
        depth(&env, ColorsSetting::Auto),
        (ColorDepth::Ansi256, DepthSource::Term)
    );
    let env = EnvView::from_pairs(&[("TERM", "xterm")]);
    assert_eq!(
        depth(&env, ColorsSetting::Auto),
        (ColorDepth::Ansi16, DepthSource::Default)
    );
    assert_eq!(
        depth(&EnvView::from_pairs(&[]), ColorsSetting::None),
        (ColorDepth::None, DepthSource::Config)
    );

    for (value, expected) in [
        ("1", Motion::Reduced),
        ("true", Motion::Reduced),
        ("YES", Motion::Reduced),
        ("on", Motion::Reduced),
        ("0", Motion::Full),
        ("false", Motion::Full),
        ("no", Motion::Full),
        ("off", Motion::Full),
    ] {
        let env = EnvView::from_pairs(&[("TALARIA_REDUCED_MOTION", value)]);
        assert_eq!(
            motion(&env, Some(expected != Motion::Reduced)),
            (expected, MotionSource::Environment),
            "{value:?} beats the config"
        );
        assert!(env_notices(&env).is_empty());
    }
    assert_eq!(
        motion(&EnvView::from_pairs(&[]), Some(true)),
        (Motion::Reduced, MotionSource::Config)
    );
    assert_eq!(
        motion(&EnvView::from_pairs(&[]), Some(false)),
        (Motion::Full, MotionSource::Config)
    );
    assert_eq!(
        motion(&EnvView::from_pairs(&[]), None),
        (Motion::Full, MotionSource::Default)
    );
    let env = EnvView::from_pairs(&[("TALARIA_REDUCED_MOTION", "maybe")]);
    assert_eq!(
        motion(&env, Some(true)),
        (Motion::Reduced, MotionSource::Config),
        "an invalid value falls through to the config"
    );
    assert_eq!(
        env_notices(&env),
        ["TALARIA_REDUCED_MOTION must be 1, true, yes, on, 0, false, no or off; ignoring it."]
    );
}
