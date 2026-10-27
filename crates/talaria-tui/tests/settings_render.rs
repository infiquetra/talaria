mod support;

use serde_json::json;
use support::{ByteSink, Vt, probe};
use talaria_core::settings::{Conflict, ReviewRow, Screen, SettingsRow, SettingsView};
use talaria_tui::live::FrameStats;
use talaria_tui::settings::draw_settings;
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn row(key: &str, value: &str, origin: &str, marker: &str, effect: &str) -> SettingsRow {
    SettingsRow {
        key: key.into(),
        value: value.into(),
        origin: origin.into(),
        marker: marker.into(),
        effect_label: effect.into(),
    }
}
fn browse() -> SettingsView {
    SettingsView {
        screen: Screen::Browsing,
        profile: "fixture".into(),
        categories: vec!["general".into(), "display".into()],
        rows: vec![
            row("display.compact", "true", "set", "*", "may need a restart"),
            row("display.skin", "\"mono\"", "set", "*", "applies now"),
        ],
        detail: "display.compact · boolean · Compact display. Default: false.".into(),
        ..Default::default()
    }
}
fn rig(cols: u16, rows: u16) -> (InlineTerminal<ByteSink>, ByteSink, Vt) {
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        probe(false, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .unwrap();
    let mut vt = Vt::new(cols, rows);
    vt.absorb(&sink);
    term.open_overlay().unwrap();
    vt.absorb(&sink);
    (term, sink, vt)
}
fn paint(
    term: &mut InlineTerminal<ByteSink>,
    sink: &ByteSink,
    vt: &mut Vt,
    view: &SettingsView,
) -> String {
    term.draw_overlay(|area, buf| draw_settings(view, area, buf))
        .unwrap();
    vt.absorb(sink);
    vt.rows().join("\n")
}

#[test]
fn browse_review_conflict_160x48() {
    let (mut term, sink, mut vt) = rig(160, 48);
    let mut view = browse();
    let screen = paint(&mut term, &sink, &mut vt, &view);
    for expected in [
        "Settings · profile fixture",
        "display.compact",
        "true",
        "set",
        "may need a restart",
        "display.skin",
        "\"mono\"",
        "applies now",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}: {screen}");
    }
    view.filter = "compact".into();
    view.rows.truncate(1);
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("compact"));
    assert!(!screen.contains("display.skin"));
    view.screen = Screen::Reviewing;
    view.review_rows = vec![ReviewRow {
        key: "display.compact".into(),
        old: "false".into(),
        new: "true".into(),
        effect_label: "may need a restart".into(),
        warnings: vec![],
    }];
    let screen = paint(&mut term, &sink, &mut vt, &view);
    for expected in [
        "Review 1 change(s) · profile fixture",
        "display.compact",
        "false → true",
        "Enter apply · Esc back",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}: {screen}");
    }
    view.screen = Screen::Conflict;
    view.conflict_rows = vec![Conflict {
        key: "display.compact".into(),
        yours: json!(true),
        baseline: Some(json!(false)),
        now: Some(json!("other")),
    }];
    let screen = paint(&mut term, &sink, &mut vt, &view);
    for expected in [
        "1 setting(s) changed outside Talaria since you opened settings.",
        "display.compact",
        "yours",
        "now",
        "o keep yours · t take theirs · Esc back",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}: {screen}");
    }
    view.screen = Screen::Loading;
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("Loading settings for profile fixture…"));
    view.screen = Screen::LoadError;
    view.banner = vec![
        "Settings need the Hermes gateway, and Talaria is not connected. r retry · Esc close"
            .into(),
    ];
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("Settings need the Hermes gateway"));
}

#[test]
fn narrow_and_too_small() {
    let (mut term, sink, mut vt) = rig(80, 24);
    let view = browse();
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("‹ display ›"), "{screen}");
    assert!(screen.contains("display.compact"));
    assert!(screen.contains("may need a restart"));
    let (mut term, sink, mut vt) = rig(59, 11);
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(
        screen.contains("Make the terminal at least 60 by 12 to use settings."),
        "{screen}"
    );
    assert!(!screen.contains("display.compact"));
}

#[test]
fn secrets_masked_and_text_sanitised() {
    let (mut term, sink, mut vt) = rig(160, 48);
    let mut view = browse();
    view.rows = vec![
        row(
            "delegation.api_key",
            "•••••••• (r reveals once)",
            "set",
            "",
            "may need a restart",
        ),
        row(
            "delegation.base_url",
            "${EXAMPLE_URL}",
            "set",
            "",
            "may need a restart",
        ),
    ];
    view.detail = "label \u{1b} test \u{202e} end".into();
    let screen = paint(&mut term, &sink, &mut vt, &view);
    for expected in [
        "delegation.api_key",
        "•••••••• (r reveals once)",
        "${EXAMPLE_URL}",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}: {screen}");
    }
    assert!(!screen.contains("expanded-test-value-1111"));
    assert!(!screen.contains("expanded-test-value-2222"));
    assert!(!screen.contains('\u{1b}'));
    assert!(!screen.contains('\u{202e}'));
    assert!(
        screen.contains(&*talaria_tui::history::sanitize(&view.detail)),
        "{screen}"
    );
}

#[test]
fn no_frames_when_idle() {
    let (mut term, sink, mut vt) = rig(160, 48);
    let view = browse();
    let _ = paint(&mut term, &sink, &mut vt, &view);
    let again = term
        .draw_overlay(|area, buf| draw_settings(&view, area, buf))
        .unwrap();
    assert_eq!(
        again,
        FrameStats {
            drew: false,
            bytes: 0
        }
    );
    assert!(sink.take().is_empty());
}
