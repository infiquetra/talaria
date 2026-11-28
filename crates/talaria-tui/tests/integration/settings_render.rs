use crate::support;

use serde_json::json;
use support::{ByteSink, Vt, probe};
use talaria_core::settings::{
    Conflict, FieldHelp, ReviewRow, Screen, SettingsEditorView, SettingsRow, SettingsView,
};
use talaria_tui::live::FrameStats;
use talaria_tui::settings::{draw_settings, draw_settings_with_theme};
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

#[test]
fn themes_colour_the_settings_surface_selected_row_and_footer() {
    use ratatui::{buffer::Buffer, layout::Rect, style::Color};
    use talaria_tui::theme::{ColorDepth, DepthSource, resolve_with_theme_file};

    let file = talaria_app::theme_file::parse(
        r##"
schema = 1
extends = "dark"
[elements."list.background"]
bg = "#112233"
[elements."list.header"]
fg = "#aabbcc"
[elements."list.selected"]
bg = "#ccbb88"
fg = "#010203"
[elements."list.muted"]
fg = "#8090a0"
"##,
    )
    .unwrap();
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    let theme = crate::view_colours_support::resolve(Some(&file)).theme;
    draw_settings_with_theme(&browse(), area, &mut buffer, &theme.views);
    assert_eq!(buffer[(79, 1)].bg, Color::Rgb(0x11, 0x22, 0x33));
    assert_eq!(buffer[(0, 0)].fg, Color::Rgb(0xaa, 0xbb, 0xcc));
    assert_eq!(buffer[(0, 3)].fg, Color::Rgb(1, 2, 3));
    assert_eq!(buffer[(0, 3)].bg, Color::Rgb(0xcc, 0xbb, 0x88));
    assert_eq!(buffer[(0, 23)].fg, Color::Rgb(0x80, 0x90, 0xa0));
    let inputs = crate::view_colours_support::inputs((ColorDepth::None, DepthSource::Config));
    let theme = resolve_with_theme_file(&inputs, Some(&file)).theme;
    draw_settings_with_theme(&browse(), area, &mut buffer, &theme.views);
    assert!(crate::view_colours_support::no_colour(&buffer).is_none());
    assert!(
        buffer[(0, 3)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );
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
fn narrow_settings_footers_keep_escape() {
    let (mut term, sink, mut vt) = rig(40, 16);
    let mut view = browse();
    let screen = paint(&mut term, &sink, &mut vt, &view);
    let footer = screen.lines().last().unwrap();
    assert!(footer.ends_with("Esc close"), "{footer}");
    assert!(!footer.contains("reload"), "{footer}");

    view.screen = Screen::Conflict;
    let screen = paint(&mut term, &sink, &mut vt, &view);
    let footer = screen.lines().last().unwrap();
    assert!(footer.ends_with("Esc back"), "{footer}");
    assert!(!footer.contains("take theirs"), "{footer}");
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
        "Skin",
        "\"mono\"",
        "Selected setting",
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
fn inspector_gives_the_readable_field_name_and_description_priority() {
    let mut view = browse();
    view.category = "display".into();
    view.rows[0] = row(
        "display.layout.deep_compact_density_for_thin_terminals",
        "First value words continue across a terminal row until value-tail",
        "set",
        "",
        "may need a restart",
    );
    view.field_help = Some(FieldHelp {
        description: "Control the number of settings visible at once.".into(),
        default_value: "standard".into(),
        note: "This applies to the selected display profile.".into(),
    });
    for width in [60, 118] {
        let (mut term, sink, mut vt) = rig(width, 24);
        let screen = paint(&mut term, &sink, &mut vt, &view);
        let panel = if width >= 108 {
            let x = usize::from(width - width / 3 + 2);
            screen
                .lines()
                .skip(1)
                .map(|row| row.chars().skip(x).collect::<String>())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            let start = screen.find("\nLayout › deep compact density").unwrap();
            screen[start..].to_owned()
        };
        let panel = panel.split_whitespace().collect::<Vec<_>>().join(" ");
        let heading = panel
            .find("Layout › deep compact density for thin terminals")
            .unwrap();
        let description = panel
            .find("Control the number of settings visible at once.")
            .unwrap();
        let value = panel
            .find("First value words continue across a terminal row until value-tail")
            .unwrap();
        let key = panel.find("Key display.layout").unwrap();
        let source = panel.find("Source Profile setting").unwrap();
        assert!(
            heading < description && description < value && value < key && key < source,
            "{panel}"
        );
        assert!(
            panel[key..source]
                .split_whitespace()
                .collect::<String>()
                .contains("display.layout.deep_compact_density_for_thin_terminals"),
            "{panel}"
        );
        assert!(
            panel.contains("Default standard") && panel.contains("may need a restart"),
            "{panel}"
        );
        assert!(
            panel.contains("This applies to the selected display profile."),
            "{panel}"
        );
    }
}

#[test]
fn settings_pointer_follows_the_visible_rows_after_resize_and_filtering() {
    use ratatui::{buffer::Buffer, layout::Rect};
    use talaria_core::settings::{
        ConfigSchemaView, EditKey, FieldKind, FieldView, SettingsAction, SettingsInput,
        SettingsKey, SettingsSnapshot, SettingsState, SettingsTarget,
    };
    use talaria_tui::settings::{PointerMotion, pointer_enabled, pointer_key};

    let field = |category: &str, name: &str| FieldView {
        key: format!("{category}.{name}"),
        kind: FieldKind::String,
        description: String::new(),
        category: category.into(),
        options: None,
        searchable: false,
        clearable: false,
    };
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: std::iter::once(field("general", "example"))
                .chain((0..20).map(|i| field("display", &format!("option_{i:02}"))))
                .collect(),
            category_order: vec!["general".into(), "display".into()],
        },
        effective: json!({}),
        raw: json!({}),
        defaults: json!({}),
    }));
    let at = |state: &SettingsState, area: Rect, x: u16, y: u16, motion| {
        pointer_key(&state.view(), area, area.x + x, area.y + y, motion)
    };
    let press = |state: &mut SettingsState, key| {
        assert!(state.update(SettingsInput::Key(key)).is_empty());
    };
    let select = PointerMotion::Select;
    let wide = Rect::new(3, 4, 118, 24);
    let category = at(&state, wide, 0, 3, select).unwrap();
    assert!(
        matches!(&category, SettingsKey::Select(SettingsTarget::Category(name)) if name == "display")
    );
    press(&mut state, category);
    let wheel = at(&state, wide, 20, 2, PointerMotion::Down).unwrap();
    press(&mut state, wheel);
    assert_eq!(state.view().cursor, 1);
    for _ in 0..2 {
        press(&mut state, SettingsKey::Action(SettingsAction::PageDown));
    }

    let compact = Rect::new(5, 7, 60, 18);
    let mut buffer = Buffer::empty(compact);
    draw_settings(&state.view(), compact, &mut buffer);
    let key = at(&state, compact, 2, 3, select).unwrap();
    let SettingsKey::Select(SettingsTarget::Field(ref field)) = key else {
        panic!("expected the top visible field");
    };
    let rendered = (compact.x..compact.right())
        .map(|x| buffer[(x, compact.y + 3)].symbol())
        .collect::<String>();
    let number = field.strip_prefix("display.option_").unwrap();
    assert!(rendered.contains(&format!("Option {number}")));
    press(&mut state, key);
    assert!(state.view().review_rows.is_empty());
    assert!(matches!(
        at(&state, compact, 0, 1, select),
        Some(SettingsKey::Action(SettingsAction::PrevCategory))
    ));
    assert!(at(&state, compact, 0, compact.height - 1, select).is_none());

    for key in [
        SettingsKey::Action(SettingsAction::Filter),
        SettingsKey::Paste("option_18".into()),
        SettingsKey::Edit(EditKey::Enter),
    ] {
        press(&mut state, key);
    }
    assert!(at(&state, compact, 0, 1, select).is_none());
    assert!(
        matches!(at(&state, compact, 2, 4, select), Some(SettingsKey::Select(SettingsTarget::Field(ref key))) if key == "display.option_18")
    );
    let filter = at(&state, compact, 2, 3, select).unwrap();
    assert!(matches!(
        filter,
        SettingsKey::Action(SettingsAction::Filter)
    ));
    press(&mut state, filter);
    assert!(!pointer_enabled(&state.view(), compact));
    assert!(at(&state, compact, 2, 4, PointerMotion::Down).is_none());
    press(&mut state, SettingsKey::Paste("_discovery_timeout".into()));
    let mut buffer = Buffer::empty(wide);
    draw_settings(&state.view(), wide, &mut buffer);
    let filter_row = (wide.x..wide.right())
        .map(|x| buffer[(x, wide.y + 2)].symbol())
        .collect::<String>();
    let footer = (wide.x..wide.right())
        .map(|x| buffer[(x, wide.bottom() - 1)].symbol())
        .collect::<String>();
    assert!(
        footer.contains("show matches · Esc clear filter"),
        "{footer}"
    );
    assert!(
        filter_row.contains("│ / option_18_discovery_timeout"),
        "{filter_row}"
    );
    assert_eq!(
        state.view().filter_cursor,
        Some("option_18_discovery_timeout".len())
    );
}

#[test]
fn narrow_and_too_small() {
    let (mut term, sink, mut vt) = rig(80, 24);
    let view = browse();
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(screen.contains("‹ Display ›"), "{screen}");
    assert!(screen.contains("display.compact"));
    assert!(screen.contains("may need a restart"));
    let (mut term, sink, mut vt) = rig(59, 11);
    let screen = paint(&mut term, &sink, &mut vt, &view);
    assert!(
        screen.contains("Settings needs 40 columns and 12 rows. Esc closes."),
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

#[test]
fn chooser_draws_descriptions_and_marks() {
    use talaria_core::settings::{
        ConfigSchemaView, EditKey, FieldKind, FieldView, SettingsAction, SettingsInput,
        SettingsKey, SettingsSnapshot, SettingsState,
    };
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![FieldView {
                key: "approvals.mode".into(),
                kind: FieldKind::Select,
                description: "Dangerous command approval mode".into(),
                category: "security".into(),
                options: Some(vec![json!("manual"), json!("smart"), json!("off")]),
                searchable: false,
                clearable: false,
            }],
            category_order: vec!["security".into()],
        },
        effective: json!({"approvals": {"mode": "smart"}}),
        raw: json!({"approvals": {"mode": "smart"}}),
        defaults: json!({"approvals": {"mode": "manual"}}),
    }));
    state.update(SettingsInput::Key(SettingsKey::Action(
        SettingsAction::Edit,
    )));
    let (mut term, sink, mut vt) = rig(100, 20);
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    for expected in [
        "> smart (current)",
        "An auxiliary model approves low-risk commands",
        "manual (default)",
        "Always ask before a dangerous command runs.",
        "Never ask (the same as --yolo).",
        "type to filter",
        "Enter choose · Esc cancel",
        "approvals.mode",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}:\n{screen}");
    }
    for ch in "yolo".chars() {
        state.update(SettingsInput::Key(SettingsKey::Char(ch)));
    }
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    assert!(screen.contains("filter: yolo"), "{screen}");
    assert!(screen.contains("> off"), "{screen}");
    assert!(!screen.contains("manual (default)"), "{screen}");
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Esc)));
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    assert!(!screen.contains("type to filter"), "{screen}");
}

#[test]
fn chooser_draws_the_list_failure_across_the_row() {
    use talaria_core::settings::{
        ConfigSchemaView, FieldKind, FieldView, SettingsAction, SettingsInput, SettingsKey,
        SettingsSnapshot, SettingsState,
    };
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![FieldView {
                key: "terminal.backend".into(),
                kind: FieldKind::Select,
                description: "Where commands run".into(),
                category: "terminal".into(),
                options: Some(vec![json!("local"), json!("docker")]),
                searchable: false,
                clearable: false,
            }],
            category_order: vec!["terminal".into()],
        },
        effective: json!({"terminal": {"backend": "local"}}),
        raw: json!({}),
        defaults: json!({"terminal": {"backend": "local"}}),
    }));
    state.update(SettingsInput::Key(SettingsKey::Action(
        SettingsAction::Edit,
    )));
    let (mut term, sink, mut vt) = rig(100, 20);
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    assert!(screen.contains("Loading the list…"), "{screen}");
    assert!(screen.contains("Ctrl+R reload list"), "{screen}");
    state.update(SettingsInput::ChoicesFailed {
        key: "terminal.backend".into(),
        profile: "fixture".into(),
        reason: "HTTP 500".into(),
    });
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    for expected in [
        "> local (current) (default)  Run directly on this machine.",
        "Could not load the list (HTTP 500). Showing built-in names. Ctrl+R retries.",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}:\n{screen}");
    }
}

#[test]
fn a_multiple_choice_list_draws_its_ticks_and_its_footer() {
    use talaria_core::settings::{
        ConfigSchemaView, EditKey, FieldKind, FieldView, SettingsAction, SettingsInput,
        SettingsKey, SettingsSnapshot, SettingsState,
    };
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![FieldView {
                key: "display.runtime_footer.fields".into(),
                kind: FieldKind::List,
                description: "Fields in the footer".into(),
                category: "display".into(),
                options: None,
                searchable: false,
                clearable: false,
            }],
            category_order: vec!["display".into()],
        },
        effective: json!({"display": {"runtime_footer": {"fields": ["model", "cwd"]}}}),
        raw: json!({}),
        defaults: json!({"display": {"runtime_footer": {"fields": ["model", "context_pct", "cwd"]}}}),
    }));
    state.update(SettingsInput::Key(SettingsKey::Action(
        SettingsAction::Edit,
    )));
    let (mut term, sink, mut vt) = rig(100, 20);
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    for expected in [
        "> [x] model",
        "[ ] context_pct",
        "[x] cwd",
        "[ ] latency",
        "Edit as text…",
        "Space tick",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}:\n{screen}");
    }
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Down)));
    state.update(SettingsInput::Key(SettingsKey::Char(' ')));
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    assert!(screen.contains("> [x] context_pct"), "{screen}");
}

#[test]
fn a_ranged_number_editor_draws_its_range_and_the_arrow_hint() {
    use talaria_core::settings::{
        ConfigSchemaView, EditKey, FieldKind, FieldView, SettingsAction, SettingsInput,
        SettingsKey, SettingsSnapshot, SettingsState,
    };
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![FieldView {
                key: "wake_word.sensitivity".into(),
                kind: FieldKind::Number,
                description: "Wake word threshold".into(),
                category: "voice".into(),
                options: None,
                searchable: false,
                clearable: false,
            }],
            category_order: vec!["voice".into()],
        },
        effective: json!({"wake_word": {"sensitivity": 0.6}}),
        raw: json!({}),
        defaults: json!({"wake_word": {"sensitivity": 0.6}}),
    }));
    state.update(SettingsInput::Key(SettingsKey::Action(
        SettingsAction::Edit,
    )));
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Up)));
    let (mut term, sink, mut vt) = rig(100, 20);
    let screen = paint(&mut term, &sink, &mut vt, &state.view());
    for expected in [
        "range 0.0 to 1.0 · step 0.05 · threshold",
        "> 0.65",
        "↑↓ change by 0.05",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}:\n{screen}");
    }
}
