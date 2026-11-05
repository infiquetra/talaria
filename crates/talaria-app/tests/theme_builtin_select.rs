//! Selecting the built-in themes, pinned before they can be selected.
use std::fs;

use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::appearance::{
    AppearanceService, ThemeFileSession, background_restored_line, theme_file_lines, theme_lines,
    theme_set_line,
};
use talaria_app::config::parse;
use talaria_app::theme_file::ColorValue;
use talaria_core::appearance::{
    AppearanceChange, ColorDepth, ColorMap, DepthSource, Motion, MotionSource, Polarity,
    PolaritySource, Rgb, Skin, TerminalColors, ThemeReport,
};
use talaria_tui::theme::{self, ThemeInputs};
use tempfile::TempDir;

fn fixture(config: &str) -> (TempDir, ThemeFileSession) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("themes")).unwrap();
    fs::write(dir.path().join("config.toml"), config).unwrap();
    fs::write(
        dir.path().join("themes/global.toml"),
        "schema = 1\nname = 'global'\nextends = 'dark'\n[palette]\naccent = '#ffb86b'\n",
    )
    .unwrap();
    let loaded = parse(config);
    assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
    let session = ThemeFileSession::new(
        dir.path().join("config.toml"),
        loaded.config.theme,
        "fixture",
    );
    (dir, session)
}

fn accent(file: &talaria_app::theme_file::ThemeFile) -> Option<&ColorValue> {
    file.palette.get("accent")
}

#[test]
fn builtins_select_without_a_file_on_disk() {
    for id in ["terminal", "ember", "paper"] {
        let (dir, mut session) = fixture("");
        assert!(!dir.path().join(format!("themes/{id}.toml")).exists());
        let (lines, change) = session.command(&format!("use {id}"));
        assert_eq!(change, AppearanceChange::Changed, "{id}: {lines:?}");
        assert_eq!(lines, [format!("Theme file: themes/{id}.toml.")]);
        assert_eq!(session.active_id(), Some(id));
        let file = session
            .file()
            .unwrap_or_else(|| panic!("{id} was not selected"));
        match id {
            "ember" => {
                assert_eq!(file.name.as_deref(), Some("Ember"));
                assert_eq!(accent(file), Some(&ColorValue::Rgb(0xff, 0xb8, 0x6b)));
            }
            "paper" => {
                assert_eq!(file.name.as_deref(), Some("Paper"));
                assert_eq!(accent(file), Some(&ColorValue::Rgb(0x00, 0x5a, 0x80)));
            }
            "terminal" => {
                assert_eq!(file.name.as_deref(), Some("Terminal"));
                assert_eq!(accent(file), Some(&ColorValue::Ansi(6)));
            }
            _ => unreachable!(),
        }

        let (dir, mut session) = fixture(&format!("[theme]\nfile = '{id}'\n"));
        assert!(!dir.path().join(format!("themes/{id}.toml")).exists());
        let notices = session.startup();
        assert!(
            notices
                .iter()
                .all(|line| !line.contains("reserved") && !line.contains("cannot open")),
            "{id} startup: {notices:?}"
        );
        assert_eq!(session.active_id(), Some(id));
        assert!(
            session.file().is_some(),
            "{id} config did not select a theme"
        );
    }
}

#[test]
fn user_file_named_after_a_builtin_is_not_loaded() {
    let (dir, mut session) = fixture("[theme]\nfile = 'global'\n");
    session.startup();
    fs::write(
        dir.path().join("themes/ember.toml"),
        "schema = 1\nname = 'Not Ember'\nextends = 'dark'\n[palette]\naccent = '#010101'\n",
    )
    .unwrap();
    let (lines, change) = session.command("use ember");
    assert_eq!(change, AppearanceChange::Changed, "{lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("themes/ember.toml")),
        "the notice should name the user file: {lines:?}"
    );
    let file = session
        .file()
        .unwrap_or_else(|| panic!("ember was not selected: {lines:?}"));
    assert_eq!(file.name.as_deref(), Some("Ember"));
    assert_ne!(file.name.as_deref(), Some("Not Ember"));
    assert_ne!(accent(file), Some(&ColorValue::Rgb(1, 1, 1)));
}

#[test]
fn dark_and_light_are_bases() {
    for id in ["dark", "light"] {
        let notice = format!("{id} is a base for extends, not a theme.");
        let (dir, mut session) = fixture(&format!("[theme]\nfile = '{id}'\n"));
        let path = dir.path().join("config.toml");
        let before = fs::read(&path).unwrap();
        let lines = session.startup();
        assert_eq!(lines, [notice.as_str()]);
        assert!(session.file().is_none());
        assert_eq!(session.active_id(), None);
        assert_eq!(fs::read(&path).unwrap(), before);

        let (dir, mut session) = fixture("[theme]\nfile = 'global'\n");
        assert!(session.startup().is_empty());
        let kept = session.file().unwrap().clone();
        let path = dir.path().join("config.toml");
        let before = fs::read(&path).unwrap();
        let (lines, change) = session.command(&format!("use {id}"));
        assert_eq!(lines, [notice.as_str()]);
        assert_eq!(change, AppearanceChange::Unchanged);
        assert_eq!(session.active_id(), Some("global"));
        assert_eq!(session.selected_id(), Some("global"));
        assert_eq!(session.file(), Some(&kept));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn light_dark_and_auto_behave_as_before() {
    let startup = (
        Polarity::Light,
        PolaritySource::TerminalBackground(Rgb(255, 255, 255)),
    );
    let mut service = AppearanceService::new(
        TerminalColors {
            background: Some(Rgb(255, 255, 255)),
            foreground: None,
        },
        startup,
    );
    let (lines, change) = service.theme_command("dark");
    assert_eq!(lines, [theme_set_line(Polarity::Dark)]);
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(
        service.polarity(),
        (Polarity::Dark, PolaritySource::Command)
    );
    let (lines, change) = service.theme_command("light");
    assert_eq!(lines, [theme_set_line(Polarity::Light)]);
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(
        service.polarity(),
        (Polarity::Light, PolaritySource::Command)
    );
    let (lines, change) = service.theme_command("auto");
    assert_eq!(
        lines,
        [background_restored_line(
            Polarity::Light,
            &startup.1.label()
        )]
    );
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(service.polarity(), startup);

    for command in ["/theme light", "/theme dark", "/theme auto"] {
        let mut state = AppState::new("fixture");
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Submit(command.into())),
        );
        let kept_background = effects.iter().any(|effect| match effect {
            AppEffect::CommitLines(lines) => lines.iter().any(|line| line.contains("Background")),
            _ => false,
        });
        assert!(
            kept_background,
            "{command} did not keep the background notice"
        );
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, AppEffect::ThemeFileCommand(_))),
            "{command} was handled as a theme file"
        );
    }
}

#[test]
fn selection_never_writes_config_toml() {
    for id in ["terminal", "ember", "paper"] {
        let (dir, mut session) = fixture("");
        let path = dir.path().join("config.toml");
        let before = fs::read(&path).unwrap();
        let (lines, change) = session.command(&format!("use {id}"));
        assert_eq!(change, AppearanceChange::Changed, "{id}: {lines:?}");
        assert_eq!(session.active_id(), Some(id));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!dir.path().join(format!("themes/{id}.toml")).exists());
    }
}

fn plain_report() -> ThemeReport {
    ThemeReport {
        skin: None,
        not_applied: None,
        polarity: (Polarity::Dark, PolaritySource::Default),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        ignored: vec![],
    }
}

fn resolve_lines(
    file: &talaria_app::theme_file::ThemeFile,
    follow: bool,
    skin: Option<&Skin>,
) -> String {
    let resolved = theme::resolve_with_theme_file(
        &ThemeInputs {
            skin,
            skipped: &[],
            follow_hermes_skin: follow,
            polarity: (Polarity::Dark, PolaritySource::Default),
            depth: (ColorDepth::TrueColor, DepthSource::Config),
            motion: (Motion::Full, MotionSource::Default),
            terminal: TerminalColors::default(),
        },
        Some(file),
    );
    let report = resolved
        .report
        .unwrap_or_else(|| panic!("bare /theme has no file report"));
    let mut lines = theme_lines(&plain_report());
    lines.extend(theme_file_lines(&report));
    lines.join("\n")
}

#[test]
fn bare_theme_names_the_builtin_its_base_and_skin() {
    let skin = Skin {
        name: "fixture".into(),
        colors: ColorMap::from([("ui_accent".into(), "#ff00aa".into())]),
        light_colors: ColorMap::new(),
        dark_colors: ColorMap::new(),
    };
    for (id, name, base, participates) in [
        ("ember", "Ember", "dark", false),
        ("paper", "Paper", "light", false),
        ("terminal", "Terminal", "terminal", true),
    ] {
        let (_dir, mut session) = fixture("");
        let (lines, change) = session.command(&format!("use {id}"));
        assert_eq!(change, AppearanceChange::Changed, "{lines:?}");
        let file = session
            .file()
            .unwrap_or_else(|| panic!("{id} was not selected: {lines:?}"));
        assert_eq!(file.name.as_deref(), Some(name));
        assert_eq!(file.extends.as_str(), base);
        let text = resolve_lines(file, true, Some(&skin));
        assert!(text.contains(name), "{text}");
        assert!(text.contains(&format!("Base: {base}")), "{text}");
        assert!(text.contains("Hermes skin"), "{text}");
        let word = if participates {
            "participates"
        } else {
            "not applied"
        };
        assert!(text.contains(word), "{id}: {text}");
    }

    let (_dir, mut session) = fixture("[theme]\nfollow_hermes_skin = false\n");
    let (lines, change) = session.command("use terminal");
    assert_eq!(change, AppearanceChange::Changed, "{lines:?}");
    let file = session
        .file()
        .unwrap_or_else(|| panic!("terminal was not selected: {lines:?}"));
    let text = resolve_lines(file, false, Some(&skin));
    assert!(text.contains("Terminal"), "{text}");
    assert!(text.contains("Base: terminal"), "{text}");
    assert!(
        text.contains("not applied"),
        "the operator setting decides skin participation: {text}"
    );
}
