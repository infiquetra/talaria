mod support;

use serde_json::json;
use support::{ByteSink, Vt, probe};
use talaria_core::settings::{
    Conflict, ReviewRow, Screen, SettingsEditorView, SettingsRow, SettingsView,
};
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
fn long_editor_follows_and_marks_the_caret() {
    use ratatui::{buffer::Buffer, layout::Rect, style::Modifier};
    let text = format!("START-{}-END", "x".repeat(150));
    let mut current = browse();
    current.screen = Screen::Editing;
    for (cursor, visible) in [(0, "START"), (text.chars().count(), "END")] {
        current.editor = Some(SettingsEditorView {
            text: text.clone(),
            cursor,
            secret: false,
        });
        let area = Rect::new(0, 0, 60, 12);
        let mut buffer = Buffer::empty(area);
        draw_settings(&current, area, &mut buffer);
        let screen: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .map(|pos| buffer[pos].symbol())
            .collect();
        assert!(screen.contains(visible), "{screen}");
        assert!(
            (0..area.height).any(|y| (0..area.width).any(|x| buffer[(x, y)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED))),
            "caret missing: {screen}"
        );
    }
}

#[test]
fn settings_editor_follows_reducer_navigation_and_preserves_value() {
    use talaria_core::settings::{
        ConfigSchemaView, EditKey, FieldKind, FieldView, SettingsAction, SettingsInput,
        SettingsKey, SettingsSnapshot, SettingsState,
    };
    let value = format!("https://例.test/{}-END", "x".repeat(130));
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![FieldView {
                key: "general.url".into(),
                kind: FieldKind::String,
                description: String::new(),
                category: "general".into(),
                options: None,
                searchable: false,
                clearable: false,
            }],
            category_order: vec!["general".into()],
        },
        effective: json!({"general": {"url": value}}),
        raw: json!({}),
        defaults: json!({"general": {"url": ""}}),
    }));
    state.update(SettingsInput::Key(SettingsKey::Action(
        SettingsAction::Edit,
    )));
    let (mut term, sink, mut vt) = rig(60, 12);
    for (key, visible) in [(EditKey::End, "-END"), (EditKey::Home, "https://例.test/")] {
        state.update(SettingsInput::Key(SettingsKey::Edit(key)));
        let screen = paint(&mut term, &sink, &mut vt, &state.view());
        assert!(screen.contains(visible), "{screen}");
    }
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Right)));
    state.update(SettingsInput::Key(SettingsKey::Char('Z')));
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Left)));
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Delete)));
    let editor = state.view().editor.unwrap();
    assert_eq!(editor.text, value);
    assert_eq!(editor.cursor, 1);
}

#[test]
fn review_wraps_unbroken_values_and_warnings() {
    let (mut term, sink, mut vt) = rig(60, 12);
    let mut view = browse();
    view.screen = Screen::Reviewing;
    view.review_rows = vec![ReviewRow {
        key: "display.long".into(),
        old: "old".into(),
        new: format!("{}VALUE-END", "界".repeat(45)),
        effect_label: String::new(),
        warnings: vec![format!("{}WARNING-END", "w".repeat(70))],
    }];
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("VALUE-END"), "{screen}");
    assert!(screen.contains("WARNING-END"), "{screen}");
    assert!(screen.contains("Enter apply"), "{screen}");
}

#[test]
fn review_scrolls_to_later_changes_before_apply() {
    use talaria_core::settings::{
        ConfigSchemaView, EditKey, FieldKind, FieldView, SettingsInput, SettingsKey,
        SettingsSnapshot, SettingsState,
    };
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    let fields: Vec<_> = (0..12)
        .map(|n| FieldView {
            key: format!("setting_{n:02}"),
            kind: FieldKind::Boolean,
            description: String::new(),
            category: "general".into(),
            options: None,
            searchable: false,
            clearable: false,
        })
        .collect();
    let effective = serde_json::Value::Object(
        (0..12)
            .map(|n| (format!("setting_{n:02}"), json!(false)))
            .collect(),
    );
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields,
            category_order: vec!["general".into()],
        },
        effective: effective.clone(),
        raw: json!({}),
        defaults: effective,
    }));
    let args = (0..12)
        .map(|n| format!("setting_{n:02}=true"))
        .collect::<Vec<_>>()
        .join(" ");
    state.update(SettingsInput::Command {
        profile: "fixture".into(),
        args,
    });
    assert_eq!(state.view().screen, Screen::Reviewing);
    let (mut term, sink, mut vt) = rig(60, 12);
    for _ in 0..25 {
        state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Down)));
    }
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    assert!(screen.contains("setting_11"), "{screen}");
    assert!(screen.contains("Enter apply"), "{screen}");
    let area = ratatui::layout::Rect::new(0, 0, 60, 12);
    let view = state.view();
    let max =
        talaria_tui::settings::review_scroll_target(&view, &SettingsKey::Edit(EditKey::End), area)
            .unwrap();
    let up =
        talaria_tui::settings::review_scroll_target(&view, &SettingsKey::Edit(EditKey::Up), area)
            .unwrap();
    assert_eq!(
        up,
        max.saturating_sub(1),
        "up moves immediately from the visible bottom"
    );
    state.update(SettingsInput::ReviewScroll(up));
    assert_eq!(state.view().review_scroll, up);
}

#[test]
fn review_and_conflict_scroll_show_late_warnings_and_choices() {
    let (mut term, sink, mut vt) = rig(60, 12);
    let mut view = browse();
    view.screen = Screen::Reviewing;
    view.review_scroll = 100;
    view.review_rows = (0..12)
        .map(|n| ReviewRow {
            key: format!("setting_{n:02}"),
            old: "old".into(),
            new: format!("{}TAIL-{n}", "wide界".repeat(20)),
            effect_label: String::new(),
            warnings: vec![format!("{}WARNING-{n}", "warning ".repeat(12))],
        })
        .collect();
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("TAIL-11"), "{screen}");
    assert!(screen.contains("WARNING-11"), "{screen}");
    assert!(screen.contains("Enter apply"), "{screen}");

    view.screen = Screen::Conflict;
    view.conflict_rows = (0..12)
        .map(|n| Conflict {
            key: format!("setting_{n:02}"),
            yours: json!(format!("{}YOURS-{n}", "x".repeat(65))),
            baseline: Some(json!("old")),
            now: Some(json!(format!("NEW-{n}"))),
        })
        .collect();
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("YOURS-11"), "{screen}");
    assert!(screen.contains("NEW-11"), "{screen}");
    assert!(screen.contains("o keep yours"), "{screen}");
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
