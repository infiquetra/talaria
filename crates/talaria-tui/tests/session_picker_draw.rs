//! Red-run tests 32 to 34b (#231): drawing the session picker.

mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use support::{ByteSink, Vt, live_view_sections, probe, tagged};
use talaria_core::directory::rows::{DirectoryRow, RowKey, RowOrigin};
use talaria_core::directory::state::{PickerAction, PickerInput, PickerState, update};
use talaria_tui::live::LiveView;
use talaria_tui::session_picker::{PickerTheme, SessionPickerView};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const NOW: f64 = 1_792_700_000.0;
const COLS: u16 = 80;
const ROWS: u16 = 24;

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
fn row(
    profile: &str,
    id: &str,
    title: &str,
    model: &str,
    ago: f64,
    messages: u64,
    source: &str,
    origin: RowOrigin,
    running: bool,
) -> DirectoryRow {
    DirectoryRow {
        key: key(profile, id),
        title: title.to_owned(),
        source: Some(source.to_owned()),
        started_at: Some(NOW - ago),
        message_count: messages,
        running,
        model: Some(model.to_owned()),
        origin,
    }
}

/// The section 5.1 data set: four rows, foreground `s1`, selected `s1`.
fn dataset() -> PickerState {
    let mut state = PickerState::new();
    update(
        &mut state,
        PickerInput::Opened {
            profiles: vec!["default".to_owned(), "fixture".to_owned()],
            foreground: Some(key("fixture", "s1")),
            scope_note: None,
            cached: Vec::new(),
            generation: 1,
        },
        20,
    );
    update(
        &mut state,
        PickerInput::Listed {
            generation: 1,
            profile: "fixture".to_owned(),
            origin: RowOrigin::Live,
            rows: vec![
                row(
                    "fixture",
                    "s1",
                    "Fix the average in calc.py",
                    "ollama-cloud/minimax-m3.1-flash",
                    120.0,
                    12,
                    "tui",
                    RowOrigin::Live,
                    false,
                ),
                row(
                    "fixture",
                    "s2",
                    "Review the parser",
                    "minimax-m3.1-flash",
                    900.0,
                    30,
                    "tui",
                    RowOrigin::Live,
                    true,
                ),
            ],
            skipped: 0,
        },
        20,
    );
    update(
        &mut state,
        PickerInput::Listed {
            generation: 1,
            profile: "default".to_owned(),
            origin: RowOrigin::Stored,
            rows: vec![row(
                "default",
                "s3",
                "",
                "minimax-m3.1-flash",
                259_200.0,
                2,
                "cli",
                RowOrigin::Stored,
                false,
            )],
            skipped: 0,
        },
        20,
    );
    update(
        &mut state,
        PickerInput::Listed {
            generation: 1,
            profile: "fixture".to_owned(),
            origin: RowOrigin::Stored,
            rows: vec![row(
                "fixture",
                "s4",
                "Try the other fix",
                "minimax-m3.1-flash",
                604_800.0,
                8,
                "desktop",
                RowOrigin::Stored,
                false,
            )],
            skipped: 0,
        },
        20,
    );
    state
}

fn render(state: &PickerState, cols: u16, rows: u16) -> Vec<String> {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    SessionPickerView::draw(state, area, &mut buf, NOW, &PickerTheme::new());
    (0..rows)
        .map(|y| {
            (0..cols)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn buffer(state: &PickerState, cols: u16, rows: u16) -> Buffer {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    SessionPickerView::draw(state, area, &mut buf, NOW, &PickerTheme::new());
    buf
}

#[test]
fn loading_and_rows_snapshots() {
    let state = dataset();

    let wide = render(&state, 160, 48);
    assert_eq!(wide[0], " Sessions · all profiles · 4 shown");
    assert_eq!(wide[1], " LIVE (2)");
    assert_eq!(
        wide[2],
        " ▸ ● fixture  Fix the average in calc.py                                                                           minimax-m3.1-flash     2m    12 msgs"
    );
    assert_eq!(
        wide[3],
        "   ◐ fixture  Review the parser                                                                                    minimax-m3.1-flash    15m    30 msgs"
    );
    assert_eq!(wide[4], " EARLIER (2)");
    assert_eq!(
        wide[5],
        "     default  untitled session                                                                                     minimax-m3.1-flash     3d     2 msgs"
    );
    assert_eq!(
        wide[6],
        "     fixture  Try the other fix                                                                                    minimax-m3.1-flash     1w     8 msgs  desktop"
    );
    for row in wide.iter().take(47).skip(7) {
        assert!(row.is_empty(), "blank body row, found {row:?}");
    }
    assert_eq!(
        wide[47],
        " ↑↓ move · Enter open · r rename · b branch · e export · / search · Tab profile · Ctrl+R refresh · Esc close"
    );

    let medium = render(&state, 100, 30);
    assert_eq!(medium[0], " Sessions · all profiles · 4 shown");
    assert_eq!(medium[1], " LIVE (2)");
    assert_eq!(
        medium[2],
        " ▸ ● fixture  Fix the average in calc.py               minimax-m3.1-flash     2m    12 msgs"
    );
    assert_eq!(
        medium[3],
        "   ◐ fixture  Review the parser                        minimax-m3.1-flash    15m    30 msgs"
    );
    assert_eq!(medium[4], " EARLIER (2)");
    assert_eq!(
        medium[5],
        "     default  untitled session                         minimax-m3.1-flash     3d     2 msgs"
    );
    assert_eq!(
        medium[6],
        "     fixture  Try the other fix                        minimax-m3.1-flash     1w     8 msgs  desktop"
    );
    for row in medium.iter().take(29).skip(7) {
        assert!(row.is_empty(), "blank body row, found {row:?}");
    }
    assert_eq!(medium[29], " ↑↓ Enter r b e / Tab Esc");

    let narrow = render(&state, 80, 24);
    assert_eq!(narrow[0], " Sessions · all profiles · 4 shown");
    assert_eq!(narrow[1], " LIVE (2)");
    assert_eq!(
        narrow[2],
        " ▸ ● fixture  Fix the average in calc.py    minimax-m3.1-flash     2m    12 msgs"
    );
    assert_eq!(
        narrow[3],
        "   ◐ fixture  Review the parser             minimax-m3.1-flash    15m    30 msgs"
    );
    assert_eq!(narrow[4], " EARLIER (2)");
    assert_eq!(
        narrow[5],
        "     default  untitled session              minimax-m3.1-flash     3d     2 msgs"
    );
    assert_eq!(
        narrow[6],
        "     fixture  Try the other fix             minimax-m3.1-flash     1w     8 msgs"
    );
    for row in narrow.iter().take(23).skip(7) {
        assert!(row.is_empty(), "blank body row, found {row:?}");
    }
    assert_eq!(narrow[23], " ↑↓ Enter r b e / Tab Esc");

    let tiny = render(&state, 50, 20);
    assert_eq!(tiny[0], " Sessions · all profiles · 4 shown");
    assert_eq!(tiny[1], " LIVE (2)");
    assert_eq!(
        tiny[2],
        " ▸ ● fixture  Fix the average in calc.py        2m"
    );
    assert_eq!(
        tiny[3],
        "   ◐ fixture  Review the parser                15m"
    );
    assert_eq!(tiny[4], " EARLIER (2)");
    assert_eq!(
        tiny[5],
        "     default  untitled session                  3d"
    );
    assert_eq!(
        tiny[6],
        "     fixture  Try the other fix                 1w"
    );
    for row in tiny.iter().skip(7) {
        assert!(row.is_empty(), "blank row, found {row:?}");
    }

    // Loading header with no rows.
    let mut loading = PickerState::new();
    update(
        &mut loading,
        PickerInput::Opened {
            profiles: vec!["fixture".to_owned()],
            foreground: None,
            scope_note: None,
            cached: Vec::new(),
            generation: 1,
        },
        20,
    );
    let rows = render(&loading, 80, 24);
    assert!(rows[0].starts_with(" Sessions · all profiles · 0 shown"));
    assert!(
        rows[0].ends_with("Loading fixture…"),
        "header: {:?}",
        rows[0]
    );
}

#[test]
fn untrusted_text_is_literal() {
    let mut state = PickerState::new();
    update(
        &mut state,
        PickerInput::Opened {
            profiles: vec!["fixture".to_owned()],
            foreground: None,
            scope_note: None,
            cached: Vec::new(),
            generation: 1,
        },
        20,
    );
    update(
        &mut state,
        PickerInput::Listed {
            generation: 1,
            profile: "fixture".to_owned(),
            origin: RowOrigin::Live,
            rows: vec![
                row(
                    "fixture",
                    "s-1",
                    "a\x1b[31mred",
                    "minimax-m3.1-flash",
                    60.0,
                    1,
                    "tui",
                    RowOrigin::Live,
                    false,
                ),
                row(
                    "fixture",
                    "s-2",
                    "b\u{202e}c",
                    "minimax-m3.1-flash",
                    60.0,
                    1,
                    "tui",
                    RowOrigin::Live,
                    false,
                ),
                row(
                    "fixture",
                    "s-3",
                    "d\0e",
                    "minimax-m3.1-flash",
                    60.0,
                    1,
                    "tui",
                    RowOrigin::Live,
                    false,
                ),
            ],
            skipped: 0,
        },
        20,
    );
    let rows = render(&state, 80, 24);
    let body = rows.join("\n");
    assert!(!body.contains('\x1b'), "no escape byte on screen");
    assert!(
        !body.contains('\u{202e}'),
        "no bidirectional control on screen"
    );
    assert!(!body.contains('\0'), "no NUL on screen");
    assert!(body.contains('a'), "the title text is drawn");
    assert!(body.contains('b'), "the title text is drawn");
    assert!(body.contains('d'), "the title text is drawn");
}

#[test]
fn editor_dialog_banner_snapshots() {
    let state = dataset();

    // Rename editor replaces the notice line.
    let mut editing = dataset();
    update(&mut editing, PickerInput::Action(PickerAction::Rename), 20);
    let rows = render(&editing, 80, 24);
    assert_eq!(
        rows[22],
        " New title for \"Fix the average in calc.py\": Fix the average in calc.py▏"
    );
    assert_eq!(rows[23], " Enter rename · Esc cancel");

    // Export dialog replaces the notice and footer lines.
    let mut dialog = dataset();
    update(&mut dialog, PickerInput::Action(PickerAction::Export), 20);
    // The dialog's format row is ninety columns wide, so it is rendered wide
    // enough to hold it.
    let rows = render(&dialog, 120, 24);
    assert_eq!(rows[21], " Export \"Fix the average in calc.py\"");
    assert_eq!(
        rows[22],
        " Format: [m] Markdown   [j] JSON   [h] HTML          Secrets: redacted (x to include them)"
    );
    assert!(
        rows[23].starts_with(" Saves to "),
        "dialog saves line: {:?}",
        rows[23]
    );
    assert!(
        rows[23].ends_with(" · Enter export · Esc cancel"),
        "dialog saves line: {:?}",
        rows[23]
    );
    // The selected format is reversed.
    let buf = buffer(&dialog, 80, 24);
    let markdown_at = rows[22].find("Markdown").expect("the format row");
    let cell = &buf[(markdown_at as u16, 22)];
    assert!(
        cell.modifier.contains(Modifier::REVERSED),
        "the selected format is reversed"
    );
    let json_at = rows[22].find("JSON").expect("the format row");
    assert!(
        !buf[(json_at as u16, 22)]
            .modifier
            .contains(Modifier::REVERSED),
        "other formats are not reversed"
    );

    // Redaction off.
    update(
        &mut dialog,
        PickerInput::Action(PickerAction::ToggleRedact),
        20,
    );
    let rows = render(&dialog, 120, 24);
    assert!(
        rows[22].ends_with("Secrets: included (x to redact them)"),
        "dialog: {:?}",
        rows[22]
    );

    // Banners.
    let mut scoped = PickerState::new();
    update(
        &mut scoped,
        PickerInput::Opened {
            profiles: vec!["fixture".to_owned()],
            foreground: None,
            scope_note: Some(
                "Showing only the profile fixture: Hermes did not allow a connection across profiles (ticket refused).".to_owned(),
            ),
            cached: Vec::new(),
            generation: 1,
        },
        20,
    );
    // The banner is a hundred and two columns wide, so it is rendered wide
    // enough to hold it.
    let rows = render(&scoped, 120, 24);
    assert_eq!(
        rows[1],
        " Showing only the profile fixture: Hermes did not allow a connection across profiles (ticket refused)."
    );
    update(
        &mut scoped,
        PickerInput::SideFailed {
            reason: "ticket refused".to_owned(),
        },
        20,
    );
    let rows = render(&scoped, 80, 24);
    assert!(
        rows.iter().any(|row| row
            == " Could not reach Hermes to list sessions (ticket refused). Ctrl+R tries again."),
        "failed banner shows: {rows:?}"
    );

    // Empty states.
    let mut empty = PickerState::new();
    update(
        &mut empty,
        PickerInput::Opened {
            profiles: vec!["fixture".to_owned()],
            foreground: None,
            scope_note: None,
            cached: Vec::new(),
            generation: 1,
        },
        20,
    );
    for origin in [RowOrigin::Live, RowOrigin::Stored] {
        update(
            &mut empty,
            PickerInput::Listed {
                generation: 1,
                profile: "fixture".to_owned(),
                origin,
                rows: Vec::new(),
                skipped: 0,
            },
            20,
        );
    }
    let rows = render(&empty, 80, 24);
    assert!(rows.contains(&" No sessions found.".to_owned()), "{rows:?}");

    // Row-read and limit notices.
    update(
        &mut empty,
        PickerInput::Listed {
            generation: 2,
            profile: "fixture".to_owned(),
            origin: RowOrigin::Live,
            rows: Vec::new(),
            skipped: 3,
        },
        20,
    );
    let rows = render(&empty, 80, 24);
    assert_eq!(
        rows[22],
        " 3 rows from Hermes could not be read and are not shown."
    );

    // Too small: the single row cut to the width.
    for (cols, rows_count, expected) in [
        (23_u16, 10_u16, " Window too small for …"),
        (40_u16, 5_u16, " Window too small for the session list.…"),
    ] {
        let rows = render(&state, cols, rows_count);
        assert_eq!(rows[0], expected, "too small at {cols} by {rows_count}");
        for row in rows.iter().skip(1) {
            assert!(row.is_empty());
        }
    }
}

fn view_of(sections: &[talaria_tui::live::PlainTextSection]) -> LiveView<'_> {
    LiveView {
        sections: sections
            .iter()
            .map(|s| s as &dyn talaria_tui::live::LiveSection)
            .collect(),
    }
}

#[test]
fn closing_restores_the_screen() {
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (COLS, ROWS),
        2,
        probe(false, false, Some((0, ROWS - 1))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    let setup = sink.take();
    let mut vt = Vt::new(COLS, ROWS);
    vt.feed(&setup);
    let sections = live_view_sections(2);
    term.draw_live(&view_of(&sections)).expect("first frame");
    vt.absorb(&sink);
    term.commit_lines(tagged(3)).expect("commit");
    vt.absorb(&sink);
    let before = vt.rows();

    let state = dataset();
    term.open_overlay().expect("open overlay");
    vt.absorb(&sink);
    let frame = term
        .draw_overlay(|area: Rect, buf: &mut Buffer| {
            SessionPickerView::draw(&state, area, buf, NOW, &PickerTheme::new());
        })
        .expect("draw overlay");
    assert!(frame.drew && frame.bytes > 0);
    vt.absorb(&sink);
    assert!(vt.alternate(), "the overlay is on the alternate screen");
    assert!(
        vt.rows()[0].contains("Sessions · all profiles"),
        "the overlay frame shows the picker header"
    );

    term.close_overlay().expect("close overlay");
    vt.absorb(&sink);
    assert!(!vt.alternate());
    // Closing clears the live region; the next paint restores the screen.
    term.draw_live(&view_of(&sections)).expect("repaint");
    vt.absorb(&sink);
    assert_eq!(vt.rows(), before, "the main screen is restored");
}
