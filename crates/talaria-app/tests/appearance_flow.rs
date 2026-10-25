//! Appearance effects against the mock gateway: commands, changes, reconnects.
//!
//! A `Changed` decision redraws once and an `Unchanged` one draws nothing; that
//! wiring lives in the application loop, so these tests assert the decisions.
//! Behavior contract: issue 227.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column as AlacColumn, Line as AlacLine};
use alacritty_terminal::term::test::TermSize as AlacTermSize;
use alacritty_terminal::term::{Config as AlacConfig, Term as AlacTerm};
use serde_json::Value;
use talaria_app::appearance::{AppearanceService, THEME_USAGE, theme_lines};
use talaria_core::appearance::{
    AppearanceChange, AppearanceInput, ColorDepth, DepthSource, Motion, MotionSource,
    NotAppliedReason, Polarity, PolaritySource, Rgb, Skin, SkippedValue, TerminalColors,
    ThemeReport,
};
use talaria_tui::markdown::{MarkdownOptions, MarkdownStream, render_markdown};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb(r, g, b)
}

fn dark_terminal() -> TerminalColors {
    TerminalColors {
        background: Some(rgb(0x1e, 0x1e, 0x1e)),
        foreground: None,
    }
}

fn light_terminal() -> TerminalColors {
    TerminalColors {
        background: Some(rgb(0xff, 0xff, 0xff)),
        foreground: None,
    }
}

fn dark_polarity() -> (Polarity, PolaritySource) {
    (
        Polarity::Dark,
        PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
    )
}

fn light_polarity() -> (Polarity, PolaritySource) {
    (
        Polarity::Light,
        PolaritySource::TerminalBackground(rgb(0xff, 0xff, 0xff)),
    )
}

fn new_service() -> AppearanceService {
    AppearanceService::new(dark_terminal(), dark_polarity())
}

fn skin_data(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../talaria-tui/tests/data/skins")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn default_report() -> ThemeReport {
    ThemeReport {
        skin: Some("default".to_owned()),
        not_applied: None,
        polarity: (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
        ),
        depth: (ColorDepth::TrueColor, DepthSource::Colorterm),
        motion: (Motion::Full, MotionSource::Default),
        ignored: Vec::new(),
    }
}

fn resolve_theme(skin: &Skin, skipped: &[SkippedValue]) -> talaria_tui::theme::Theme {
    talaria_tui::theme::resolve(&talaria_tui::theme::ThemeInputs {
        skin: Some(skin),
        skipped,
        follow_hermes_skin: true,
        polarity: (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
        ),
        depth: (ColorDepth::TrueColor, DepthSource::Colorterm),
        motion: (Motion::Full, MotionSource::Default),
        terminal: dark_terminal(),
    })
}

#[tokio::test]
async fn skin_changed_redraws_once() {
    // The launch skin carries a null colour: the launch still completes and the
    // value is not counted, because ui_label is in no chain.
    let mut service = new_service();
    let mut launch = skin_data("default.json");
    launch["colors"]["ui_label"] = Value::Null;
    let change = service.update(AppearanceInput::Handshake {
        ready: None,
        describe: Some(launch.clone()),
    });
    assert_eq!(change, AppearanceChange::Changed);
    let skin = service.skin().expect("the launch skin applies");
    assert_eq!(skin.name, "default");
    let report = resolve_theme(skin, service.skipped()).report;
    assert!(
        !theme_lines(&report)
            .iter()
            .any(|line| line.contains("Ignored skin values")),
        "no ignored line for a key outside every chain"
    );

    let catalog = HashMap::from([
        ("default".to_owned(), launch),
        ("ares".to_owned(), skin_data("ares.json")),
    ]);
    let (mock, _client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(catalog);

    mock.send_event("skin.changed", skin_data("ares.json"))
        .await;
    let change = service.update(AppearanceInput::SkinChanged {
        payload: skin_data("ares.json"),
        replayed: false,
    });
    assert_eq!(change, AppearanceChange::Changed, "one frame");

    let change = service.update(AppearanceInput::SkinChanged {
        payload: skin_data("ares.json"),
        replayed: false,
    });
    assert_eq!(change, AppearanceChange::Unchanged, "zero frames");
    assert!(mock.violations().is_empty());
}

#[tokio::test]
async fn skin_command_sets_then_describes() {
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("ares".to_owned(), skin_data("ares.json")),
    ]));
    let mut service = new_service();

    // With a session the set carries its id, then a describe follows.
    let lines = service
        .skin_command(
            &client,
            Some("ares"),
            Some(mock_gateway::SESSION_ID),
            "fixture",
        )
        .await;
    assert_eq!(
        lines,
        ["Skin set to \"ares\" for profile fixture. \
          Hermes windows with a session open on this profile change too."]
    );
    let sets = mock.requests("config.set");
    assert_eq!(sets.len(), 1);
    assert_eq!(
        sets[0].params.get("session_id").and_then(Value::as_str),
        Some(mock_gateway::SESSION_ID)
    );
    assert_eq!(
        sets[0].params.get("key").and_then(Value::as_str),
        Some("skin")
    );
    assert_eq!(
        sets[0].params.get("value").and_then(Value::as_str),
        Some("ares")
    );
    let describes = mock.requests("runtime.describe");
    assert_eq!(describes.len(), 1);
    assert!(
        sets[0].index < describes[0].index,
        "the describe follows the set"
    );
    assert_eq!(service.skin().map(|skin| skin.name.as_str()), Some("ares"));

    // Without a session there is no session id.
    let lines = service
        .skin_command(&client, Some("default"), None, "fixture")
        .await;
    assert_eq!(
        lines,
        ["Skin set to \"default\" for profile fixture. \
          Hermes windows with a session open on this profile change too."]
    );
    let sets = mock.requests("config.set");
    assert_eq!(sets.len(), 2);
    assert!(
        sets[1].params.get("session_id").is_none(),
        "no session, no session id"
    );

    // A bare /skin reads the configured name, then describes.
    let lines = service
        .skin_command(&client, None, Some(mock_gateway::SESSION_ID), "fixture")
        .await;
    assert_eq!(lines, ["Skin: default (Hermes profile fixture)"]);
    let gets = mock.requests("config.get");
    assert_eq!(gets.len(), 1);
    assert_eq!(
        gets[0].params.get("key").and_then(Value::as_str),
        Some("skin")
    );
    assert_eq!(
        gets[0].params.get("session_id").and_then(Value::as_str),
        Some(mock_gateway::SESSION_ID)
    );
    let describes = mock.requests("runtime.describe");
    assert_eq!(describes.len(), 3);
    assert!(
        gets[0].index < describes[2].index,
        "the describe follows the get"
    );
    assert!(mock.violations().is_empty());
}

#[tokio::test]
async fn skin_command_names_and_unknown_skin() {
    let mut nord = skin_data("default.json");
    nord["name"] = Value::String("Nord Dark".to_owned());
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("nord".to_owned(), nord),
        ("broken".to_owned(), Value::Object(Default::default())),
    ]));
    let mut service = new_service();
    service.update(AppearanceInput::Handshake {
        ready: None,
        describe: Some(skin_data("default.json")),
    });

    // Hermes falls back to default for an unknown name: the unknown line, once.
    let lines = service
        .skin_command(&client, Some("nosuch"), None, "fixture")
        .await;
    assert_eq!(
        lines,
        [
            "Skin set to \"nosuch\" for profile fixture. \
             Hermes windows with a session open on this profile change too.",
            "Hermes has no skin named \"nosuch\"; it is using \"default\". \
             Run /skin default to clear the setting.",
        ]
    );
    assert_eq!(
        service.skin().map(|skin| skin.name.as_str()),
        Some("default")
    );

    // A skin whose name differs from its configured name.
    let lines = service
        .skin_command(&client, Some("nord"), None, "fixture")
        .await;
    assert_eq!(
        lines,
        [
            "Skin set to \"nord\" for profile fixture. \
             Hermes windows with a session open on this profile change too.",
            "Hermes calls this skin \"Nord Dark\".",
        ]
    );

    // A describe without a skin: /theme then shows none.
    let lines = service
        .skin_command(&client, Some("broken"), None, "fixture")
        .await;
    assert_eq!(
        lines,
        [
            "Skin set to \"broken\" for profile fixture. \
             Hermes windows with a session open on this profile change too.",
            "Hermes sent no skin for \"broken\", so Talaria uses its own colours.",
        ]
    );
    assert_eq!(service.skin(), None);
    let report = talaria_tui::theme::resolve(&talaria_tui::theme::ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: true,
        polarity: dark_polarity(),
        depth: (ColorDepth::TrueColor, DepthSource::Colorterm),
        motion: (Motion::Full, MotionSource::Default),
        terminal: dark_terminal(),
    })
    .report;
    assert!(
        theme_lines(&report)
            .iter()
            .any(|line| line == "  Skin: none"),
        "no skin reads as none"
    );

    // A bare /skin when nothing is configured: the configured name is default.
    mock.set_config_skin("default");
    mock.set_config_skin_null();
    let lines = service.skin_command(&client, None, None, "fixture").await;
    assert_eq!(lines, ["Skin: default (Hermes profile fixture)"]);

    // The profile asks for an unknown name while Hermes uses default.
    mock.set_config_skin("nosuch");
    let lines = service.skin_command(&client, None, None, "fixture").await;
    assert_eq!(
        lines,
        [
            "Skin: default. The profile asks for \"nosuch\", which Hermes could not find, \
          so it uses default."
        ]
    );
    assert!(mock.violations().is_empty());
}

#[tokio::test]
async fn skin_command_refused_dropped_and_describe_failed() {
    async fn refused(behavior: mock_gateway::Behavior) -> Vec<String> {
        let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
        mock.set_skin_catalog(HashMap::from([
            ("default".to_owned(), skin_data("default.json")),
            ("ares".to_owned(), skin_data("ares.json")),
        ]));
        let mut service = new_service();
        service.update(AppearanceInput::Handshake {
            ready: None,
            describe: Some(skin_data("default.json")),
        });
        mock.set_behavior("config.set", behavior);
        let lines = service
            .skin_command(&client, Some("ares"), None, "fixture")
            .await;
        assert_eq!(
            service.skin().map(|skin| skin.name.as_str()),
            Some("default"),
            "a refusal keeps the skin"
        );
        assert_eq!(mock.requests("config.set").len(), 1, "a single attempt");
        assert!(
            mock.requests("runtime.describe").is_empty(),
            "no describe after a refusal"
        );
        assert!(mock.violations().is_empty());
        lines
    }

    for (behavior, expected) in [
        (
            mock_gateway::Behavior::Refuse("invalid_params".to_owned()),
            vec!["Hermes refused to change the skin (invalid_params)."],
        ),
        (
            mock_gateway::Behavior::Refuse("permission_denied".to_owned()),
            vec![
                "Hermes refused to change the skin (permission_denied).",
                "This connection may not change settings.",
            ],
        ),
        (
            mock_gateway::Behavior::Refuse("profile_mismatch".to_owned()),
            vec!["Hermes refused to change the skin (profile_mismatch)."],
        ),
        (
            mock_gateway::Behavior::RefuseCode {
                reason: "storage_unavailable".to_owned(),
                code: 5001,
            },
            vec!["Hermes refused to change the skin (storage_unavailable)."],
        ),
        (
            mock_gateway::Behavior::Refuse("runtime_draining".to_owned()),
            vec!["Hermes refused to change the skin (runtime_draining)."],
        ),
        (
            mock_gateway::Behavior::RefuseBare {
                message: "the store is locked".to_owned(),
            },
            vec!["Hermes refused to change the skin (the store is locked)."],
        ),
    ] {
        let lines = refused(behavior).await;
        assert_eq!(
            lines.iter().map(String::as_str).collect::<Vec<_>>(),
            expected
        );
    }

    // The connection drops before the reply: the outcome is unknown.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("ares".to_owned(), skin_data("ares.json")),
    ]));
    let mut service = new_service();
    service.update(AppearanceInput::Handshake {
        ready: None,
        describe: Some(skin_data("default.json")),
    });
    mock.set_silent("config.set", true);
    let attempt = tokio::spawn(async move {
        let mut service = service;
        let lines = service
            .skin_command(&client, Some("ares"), None, "fixture")
            .await;
        (service, lines)
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    mock.kill_connection().await;
    let (service, lines) = attempt.await.unwrap();
    assert_eq!(
        lines,
        ["The skin change may or may not have been saved. Run /skin to check."]
    );
    assert_eq!(
        service.skin().map(|skin| skin.name.as_str()),
        Some("default")
    );

    // Not connected: nothing is sent.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("ares".to_owned(), skin_data("ares.json")),
    ]));
    let mut service = new_service();
    service.update(AppearanceInput::Handshake {
        ready: None,
        describe: Some(skin_data("default.json")),
    });
    client.close().await;
    let lines = service
        .skin_command(&client, Some("ares"), None, "fixture")
        .await;
    assert_eq!(
        lines,
        ["Talaria could not reach Hermes, so nothing was changed. \
          Try again when the status line shows it is connected."]
    );
    assert!(mock.requests("config.set").is_empty());
    assert!(mock.requests("config.get").is_empty());
    assert!(mock.requests("runtime.describe").is_empty());
    assert!(mock.violations().is_empty());

    // The set lands but the describe is refused.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("ares".to_owned(), skin_data("ares.json")),
    ]));
    let mut service = new_service();
    service.update(AppearanceInput::Handshake {
        ready: None,
        describe: Some(skin_data("default.json")),
    });
    mock.queue_behavior(
        "runtime.describe",
        mock_gateway::Behavior::Refuse("permission_denied".to_owned()),
    );
    let lines = service
        .skin_command(&client, Some("ares"), None, "fixture")
        .await;
    assert_eq!(
        lines,
        [
            "Skin set to \"ares\" for profile fixture. \
             Hermes windows with a session open on this profile change too.",
            "Talaria could not fetch the new skin (permission_denied). Run /skin to refresh.",
        ]
    );
    assert!(mock.violations().is_empty());
}

#[tokio::test]
async fn bad_names_and_bad_theme_args_send_nothing() {
    const SKIN_USAGE: &str = "Usage: /skin [name]. A skin name is letters, digits, dot, dash or \
                              underscore, up to 64 characters.";
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("ares".to_owned(), skin_data("ares.json")),
    ]));
    let mut service = new_service();

    let long = "a".repeat(65);
    for bad in ["../x", long.as_str(), "a/b", "", "  "] {
        let lines = service
            .skin_command(&client, Some(bad), None, "fixture")
            .await;
        assert_eq!(lines, [SKIN_USAGE], "{bad:?} is refused");
    }

    let (lines, change) = service.theme_command("purple");
    assert_eq!(lines, [THEME_USAGE]);
    assert_eq!(change, AppearanceChange::Unchanged);

    assert!(mock.requests("config.set").is_empty());
    assert!(mock.requests("config.get").is_empty());
    assert!(mock.requests("runtime.describe").is_empty());
    assert!(mock.violations().is_empty());
}

#[test]
fn theme_command_reports_and_overrides() {
    // The 9.2 lines.
    let report = default_report();
    assert_eq!(
        theme_lines(&report),
        [
            "Theme",
            "  Skin: default from Hermes",
            "  Background: dark (terminal background #1e1e1e)",
            "  Colours: 24-bit (COLORTERM)",
            "  Motion: full (default)",
        ]
    );

    // One case per background source label.
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
        let mut report = default_report();
        report.polarity = (Polarity::Dark, source);
        assert_eq!(
            theme_lines(&report)[2],
            format!("  Background: dark ({label})")
        );
    }

    // One case per colours source label.
    for (depth, source, word, label) in [
        (ColorDepth::None, DepthSource::NoColorEnv, "off", "NO_COLOR"),
        (
            ColorDepth::TrueColor,
            DepthSource::Config,
            "24-bit",
            "config.toml",
        ),
        (ColorDepth::None, DepthSource::TermDumb, "off", "TERM=dumb"),
        (
            ColorDepth::TrueColor,
            DepthSource::Colorterm,
            "24-bit",
            "COLORTERM",
        ),
        (ColorDepth::Ansi256, DepthSource::Term, "256", "TERM"),
        (ColorDepth::Ansi16, DepthSource::Default, "16", "default"),
    ] {
        assert_eq!(source.label(), label);
        let mut report = default_report();
        report.depth = (depth, source);
        assert_eq!(
            theme_lines(&report)[3],
            format!("  Colours: {word} ({label})")
        );
    }

    // One case per motion source label.
    for (motion, source, word, label) in [
        (
            Motion::Reduced,
            MotionSource::Environment,
            "reduced",
            "TALARIA_REDUCED_MOTION",
        ),
        (
            Motion::Reduced,
            MotionSource::Config,
            "reduced",
            "config.toml",
        ),
        (Motion::Full, MotionSource::Default, "full", "default"),
    ] {
        assert_eq!(source.label(), label);
        let mut report = default_report();
        report.motion = (motion, source);
        assert_eq!(
            theme_lines(&report)[4],
            format!("  Motion: {word} ({label})")
        );
    }

    // Each skin state.
    for (skin, not_applied, state) in [
        (None, None, "none"),
        (
            Some("ares".to_owned()),
            Some(NotAppliedReason::ColoursOff),
            "ares (not applied: colours off)",
        ),
        (
            Some("ares".to_owned()),
            Some(NotAppliedReason::FollowOff),
            "ares (not applied: follow_hermes_skin = false)",
        ),
        (
            Some("ares".to_owned()),
            Some(NotAppliedReason::SixteenColours),
            "ares (not applied: 16-colour terminal)",
        ),
        (Some("ares".to_owned()), None, "ares from Hermes"),
    ] {
        let mut report = default_report();
        report.skin = skin;
        report.not_applied = not_applied;
        assert_eq!(theme_lines(&report)[1], format!("  Skin: {state}"));
    }

    // The ignored line shows only when non-zero, with the first three keys.
    let mut report = default_report();
    assert_eq!(theme_lines(&report).len(), 5);
    report.ignored = vec!["ui_accent".to_owned()];
    let lines = theme_lines(&report);
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[5], "  Ignored skin values: 1 (ui_accent)");
    report.ignored = ["a", "b", "c", "d"]
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        theme_lines(&report)[5],
        "  Ignored skin values: 4 (a, b, c)"
    );

    // /theme dark on a light probe flips polarity and redraws once.
    let mut service = AppearanceService::new(light_terminal(), light_polarity());
    let (lines, change) = service.theme_command("dark");
    assert_eq!(
        lines,
        ["Background set to dark for this session. \
          Lines already in scrollback keep their colours."]
    );
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(service.polarity_override(), Some(Polarity::Dark));

    // /theme auto restores the start-up detection.
    let (lines, change) = service.theme_command("auto");
    assert_eq!(
        lines,
        ["Background detection restored: light (terminal background #ffffff)."]
    );
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(service.polarity_override(), None);
}

/// A minimal scrollback emulator: feed bytes, read text and colours.
struct Scrollback {
    term: AlacTerm<VoidListener>,
    processor: vte::ansi::Processor,
}

impl Scrollback {
    fn new(cols: u16, rows: u16) -> Self {
        let config = AlacConfig {
            scrolling_history: 100,
            ..AlacConfig::default()
        };
        Self {
            term: AlacTerm::new(
                config,
                &AlacTermSize::new(cols.into(), rows.into()),
                VoidListener,
            ),
            processor: vte::ansi::Processor::new(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    /// The foreground of the first cell holding `needle`, oldest row first.
    fn fg_of(&self, needle: &str) -> alacritty_terminal::vte::ansi::Color {
        let grid = self.term.grid();
        let history = grid.history_size() as i32;
        let screen = grid.screen_lines() as i32;
        for line in -history..screen {
            let mut text = String::new();
            for column in 0..grid.columns() {
                text.push(grid[AlacLine(line)][AlacColumn(column)].c);
            }
            if let Some(index) = text.find(needle) {
                return grid[AlacLine(line)][AlacColumn(index)].fg;
            }
        }
        panic!("{needle:?} is not in the scrollback")
    }
}

#[test]
fn scrollback_and_streaming_cell_keep_their_theme() {
    use talaria_tui::terminal::{ProbeReply, TerminalCaps};
    let (default, default_skipped) = {
        let (skin, skipped) = Skin::from_value(&skin_data("default.json"));
        (skin.unwrap(), skipped)
    };
    let (ares, ares_skipped) = {
        let (skin, skipped) = Skin::from_value(&skin_data("ares.json"));
        (skin.unwrap(), skipped)
    };
    let old = resolve_theme(&default, &default_skipped);
    let new = resolve_theme(&ares, &ares_skipped);

    let old_options = MarkdownOptions {
        width: 80,
        theme: old.markdown.clone(),
        color: old.color,
        base_style: ratatui::style::Style::default(),
    };
    let old_heading = render_markdown("# Old head\n", &old_options);

    // The skin changes mid-stream; the tail keeps the old theme.
    let mut stream = MarkdownStream::new(old_options);
    let update = stream.push("# Strea");
    assert!(
        update.committed.is_empty(),
        "a partial heading stays in the tail"
    );
    let update = stream.push("m head\n");
    stream.finish();
    let tail_text: String = update
        .tail
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.to_string())
        .collect();
    assert!(tail_text.contains("Stream head"), "{tail_text:?}");
    for line in &update.tail {
        for span in &line.spans {
            if span.content.contains("Stream") {
                assert_eq!(
                    span.style.fg,
                    Some(ratatui::style::Color::Rgb(0xff, 0xd7, 0x00)),
                    "the tail keeps the old heading colour"
                );
            }
        }
    }

    let new_options = MarkdownOptions {
        width: 80,
        theme: new.markdown.clone(),
        color: new.color,
        base_style: ratatui::style::Style::default(),
    };
    let new_heading = render_markdown("# New head\n", &new_options);

    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        4,
        ProbeReply {
            caps: TerminalCaps::default(),
            cursor: Some((0, 23)),
            ..Default::default()
        },
        RestoreLink::detached(),
    )
    .unwrap();
    let mut scrollback = Scrollback::new(80, 24);
    term.commit_lines(old_heading).unwrap();
    scrollback.feed(&sink.take());
    term.commit_lines(update.tail).unwrap();
    scrollback.feed(&sink.take());
    term.commit_lines(new_heading).unwrap();
    scrollback.feed(&sink.take());

    use alacritty_terminal::vte::ansi::{Color as AlacColor, Rgb as AlacRgb};
    assert_eq!(
        scrollback.fg_of("Old head"),
        AlacColor::Spec(AlacRgb {
            r: 0xff,
            g: 0xd7,
            b: 0x00
        })
    );
    assert_eq!(
        scrollback.fg_of("Stream head"),
        AlacColor::Spec(AlacRgb {
            r: 0xff,
            g: 0xd7,
            b: 0x00
        })
    );
    assert_eq!(
        scrollback.fg_of("New head"),
        AlacColor::Spec(AlacRgb {
            r: 0xc7,
            g: 0xa9,
            b: 0x6b
        })
    );
}

#[derive(Clone, Default)]
struct ByteSink {
    bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl ByteSink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut self.bytes.lock().unwrap())
    }
}

impl std::io::Write for ByteSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn reconnect_describes_again_and_ignores_replayed_skin() {
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_skin_catalog(HashMap::from([
        ("default".to_owned(), skin_data("default.json")),
        ("ares".to_owned(), skin_data("ares.json")),
    ]));
    let mut service = new_service();

    service.connection_opened(&client, 2).await;
    assert_eq!(
        service.skin().map(|skin| skin.name.as_str()),
        Some("default")
    );
    assert_eq!(mock.requests("runtime.describe").len(), 1);

    mock.queue_behavior(
        "runtime.describe",
        mock_gateway::Behavior::Refuse("boom".to_owned()),
    );
    mock.set_config_skin("ares");
    service.connection_opened(&client, 3).await;
    let describes = mock.requests("runtime.describe");
    assert_eq!(describes.len(), 3, "the retry describes once more");
    let gap = describes[2].at.saturating_duration_since(describes[1].at);
    assert!(
        gap >= Duration::from_secs(1),
        "the retry waits a second, waited {gap:?}"
    );
    assert_eq!(
        service.skin().map(|skin| skin.name.as_str()),
        Some("ares"),
        "the retry's skin applies"
    );

    let change = service.update(AppearanceInput::SkinChanged {
        payload: skin_data("default.json"),
        replayed: true,
    });
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(
        service.skin().map(|skin| skin.name.as_str()),
        Some("ares"),
        "a replayed event naming the old skin changes nothing"
    );
    assert!(mock.violations().is_empty());
}
