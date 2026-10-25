//! The history search box: matching, keys, and rendering.
//!
//! Behavior contract: issue 228.

mod support;

use std::sync::Arc;
use std::time::SystemTime;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use support::{ByteSink, probe};
use talaria_core::prompt_history::HistoryEntry;
use talaria_tui::history_search::{HistorySearch, SearchOutcome};
use talaria_tui::keys::{Key, KeyChord, KeyCode, KeyContext, KeyKind, Mods};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn entry(text: &str, ts: i64, this_folder: bool) -> HistoryEntry {
    HistoryEntry {
        text: text.to_owned(),
        ts,
        this_folder,
    }
}

fn search_chords() -> Vec<KeyChord> {
    vec![KeyChord::parse("ctrl+r", KeyContext::Composer).unwrap()]
}

fn open(entries: Vec<HistoryEntry>) -> HistorySearch {
    HistorySearch::open(
        Arc::from(entries),
        "saved draft".to_owned(),
        search_chords(),
    )
}

fn press(code: KeyCode) -> Key {
    Key {
        code,
        mods: Mods::default(),
        kind: KeyKind::Press,
    }
}

fn ctrl(letter: char) -> Key {
    Key {
        code: KeyCode::Char(letter),
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
        kind: KeyKind::Press,
    }
}

fn typed(text: &str, search: &mut HistorySearch, now: SystemTime) {
    for character in text.chars() {
        let out = search.handle_key(
            Key {
                code: KeyCode::Char(character),
                mods: Mods::default(),
                kind: KeyKind::Press,
            },
            now,
        );
        assert_eq!(out, SearchOutcome::Changed);
    }
}

/// Accept the current selection after these keys, on a fresh box.
fn accept_after(entries: Vec<HistoryEntry>, query: &str, keys: &[Key]) -> SearchOutcome {
    let now = SystemTime::now();
    let mut search = open(entries);
    typed(query, &mut search, now);
    for key in keys {
        let out = search.handle_key(*key, now);
        assert!(
            matches!(out, SearchOutcome::Changed | SearchOutcome::Unchanged),
            "moving never accepts: {out:?}"
        );
    }
    search.handle_key(press(KeyCode::Enter), now)
}

fn match_entries(now_secs: i64) -> Vec<HistoryEntry> {
    vec![
        entry("alpha one", now_secs - 3 * 86_400, true),
        entry("beta two", now_secs - 2 * 86_400, true),
        entry("Alpha three", now_secs - 86_400, true),
        entry("alpha four", now_secs - 3_600, false),
    ]
}

#[test]
fn matching_and_order() {
    let now_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Case-insensitive substring, newest first, this folder first.
    let out = accept_after(match_entries(now_secs), "alp", &[]);
    assert_eq!(out, SearchOutcome::Accept("Alpha three".to_owned()));

    let out = accept_after(match_entries(now_secs), "ALP", &[]);
    assert_eq!(out, SearchOutcome::Accept("Alpha three".to_owned()));

    // Older, then newer.
    let out = accept_after(match_entries(now_secs), "alp", &[press(KeyCode::Up)]);
    assert_eq!(out, SearchOutcome::Accept("alpha one".to_owned()));
    let out = accept_after(
        match_entries(now_secs),
        "alp",
        &[press(KeyCode::Up), press(KeyCode::Down)],
    );
    assert_eq!(out, SearchOutcome::Accept("Alpha three".to_owned()));

    // Tab widens the scope to every folder.
    let now = SystemTime::now();
    let mut search = open(match_entries(now_secs));
    typed("alp", &mut search, now);
    assert_eq!(
        search.handle_key(press(KeyCode::Tab), now),
        SearchOutcome::Changed
    );
    assert_eq!(
        search.handle_key(press(KeyCode::Enter), now),
        SearchOutcome::Accept("alpha four".to_owned())
    );

    // Each distinct text shows once, at its newest position.
    let mut entries = match_entries(now_secs);
    entries.push(entry("alpha one", now_secs - 100, true));
    let out = accept_after(entries, "alp", &[]);
    assert_eq!(out, SearchOutcome::Accept("alpha one".to_owned()));
    let mut entries = match_entries(now_secs);
    entries.push(entry("alpha one", now_secs - 100, true));
    let out = accept_after(entries, "alp", &[press(KeyCode::Up)]);
    assert_eq!(
        out,
        SearchOutcome::Accept("Alpha three".to_owned()),
        "the older duplicate is skipped"
    );
}

#[test]
fn keys_accept_cancel() {
    let now_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let now = SystemTime::now();

    // The search chord itself moves older, like Up.
    let out = accept_after(match_entries(now_secs), "alp", &[ctrl('r')]);
    assert_eq!(out, SearchOutcome::Accept("alpha one".to_owned()));

    // Enter with no match changes nothing.
    let mut search = open(match_entries(now_secs));
    typed("zzzz", &mut search, now);
    assert_eq!(
        search.handle_key(press(KeyCode::Enter), now),
        SearchOutcome::Unchanged
    );

    // Esc and Ctrl+C cancel with the draft.
    for key in [press(KeyCode::Esc), ctrl('c')] {
        let mut search = open(match_entries(now_secs));
        typed("alp", &mut search, now);
        assert_eq!(search.handle_key(key, now), SearchOutcome::Cancel);
        assert_eq!(search.saved_draft(), "saved draft");
    }

    // A paste adds its first line, controls removed.
    let mut search = open(match_entries(now_secs));
    search.handle_paste("alp\nbeta\x07");
    assert_eq!(
        search.handle_key(press(KeyCode::Enter), now),
        SearchOutcome::Accept("Alpha three".to_owned())
    );

    // Backspace removes the last grapheme; anything else is ignored.
    let mut search = open(match_entries(now_secs));
    typed("alpx", &mut search, now);
    assert_eq!(
        search.handle_key(press(KeyCode::Backspace), now),
        SearchOutcome::Changed
    );
    assert_eq!(
        search.handle_key(press(KeyCode::Enter), now),
        SearchOutcome::Accept("Alpha three".to_owned())
    );
    let mut search = open(match_entries(now_secs));
    assert_eq!(
        search.handle_key(press(KeyCode::Backspace), now),
        SearchOutcome::Unchanged,
        "backspace on an empty query"
    );
    assert_eq!(
        search.handle_key(press(KeyCode::F(5)), now),
        SearchOutcome::Unchanged,
        "anything else is ignored"
    );
}

/// Draw one section through the real terminal and return its raw bytes.
fn terminal_bytes(section: &dyn LiveSection, cols: u16, rows: u16) -> Vec<u8> {
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        10,
        probe(true, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .unwrap();
    let view = LiveView {
        sections: vec![section],
    };
    term.draw_live(&view).unwrap();
    sink.take()
}

fn screen_rows(bytes: &[u8], cols: u16, rows: u16) -> Vec<String> {
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(bytes);
    parser
        .screen()
        .rows(0, cols)
        .map(|row| row.trim_end().to_string())
        .collect()
}

fn search_with_matches(now: SystemTime, now_secs: i64) -> HistorySearch {
    let entries = vec![
        entry("fix the failing test", now_secs - 7_230, true),
        entry(
            "run the tests and summarise failures\nsecond\nthird\nfourth",
            now_secs - 86_430,
            true,
        ),
        entry("add a test for average()", now_secs - 259_230, true),
    ];
    let mut search = open(entries);
    typed("test", &mut search, now);
    search
}

#[test]
fn renders_at_80_columns() {
    let now = SystemTime::now();
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let search = search_with_matches(now, now_secs);
    let section = search.section(now);
    let bytes = terminal_bytes(&section, 80, 24);
    let rows = screen_rows(&bytes, 80, 24);
    let screen = rows.join("\n");
    assert!(screen.contains("search history"), "{screen}");
    assert!(screen.contains("this folder"), "{screen}");
    assert!(screen.contains("tab: all folders"), "{screen}");
    assert!(screen.contains("fix the failing test"), "{screen}");
    assert!(
        screen.contains("run the tests and summarise failures"),
        "{screen}"
    );
    assert!(screen.contains("(+3 lines)"), "{screen}");
    assert!(screen.contains("2h ago"), "{screen}");
    assert!(screen.contains("1d ago"), "{screen}");
    assert!(screen.contains("3d ago"), "{screen}");
    assert!(screen.contains("enter: put in composer"), "{screen}");
    assert!(screen.contains("esc: cancel"), "{screen}");

    // The matched part is bold; the selected row is reversed.
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(&bytes);
    let text: Vec<String> = parser
        .screen()
        .rows(0, 80)
        .map(|row| row.trim_end().to_string())
        .collect();
    let row = text
        .iter()
        .position(|row| row.contains("failing test"))
        .expect("the first match row");
    let byte = text[row].find("test").expect("the matched word");
    let col = unicode_width::UnicodeWidthStr::width(&text[row][..byte]) as u16;
    for offset in 0..4 {
        let cell = parser
            .screen()
            .cell(row as u16, col + offset)
            .expect("a match cell");
        assert!(cell.bold(), "the matched part is bold");
        assert!(cell.inverse(), "the selected row is reversed");
    }

    // Both empty states.
    let search = open(Vec::new());
    let section = search.section(now);
    let bytes = terminal_bytes(&section, 80, 24);
    let screen = screen_rows(&bytes, 80, 24).join("\n");
    assert!(screen.contains("No prompt history yet."), "{screen}");
    assert!(screen.contains("this folder"), "{screen}");

    let mut search = open(match_entries(now_secs));
    typed(&"x".repeat(40), &mut search, now);
    let section = search.section(now);
    let bytes = terminal_bytes(&section, 80, 24);
    let screen = screen_rows(&bytes, 80, 24).join("\n");
    let shown = "x".repeat(30);
    assert!(
        screen.contains(&format!("No earlier prompt matches \"{shown}\".")),
        "{screen}"
    );

    // Under 20 columns: no border, the query row and one match row.
    let search = search_with_matches(now, now_secs);
    let section = search.section(now);
    let area = Rect::new(0, 0, 19, 5);
    let mut buffer = Buffer::empty(area);
    section.render(area, &mut buffer);
    let rows: Vec<String> = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let narrow = rows.join("\n");
    assert!(!narrow.contains('╭'), "{narrow}");
    assert!(narrow.contains("test"), "{narrow}");
    assert!(narrow.contains("fix the failing te…"), "{narrow}");
}

#[test]
fn hostile_entries_render_literally() {
    let now = SystemTime::now();
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // A title-setting sequence and a bidi override must reach the screen as text.
    let search = open(vec![entry(
        "\u{1b}]0;pwned\u{7} x\u{202e}y",
        now_secs - 60,
        true,
    )]);
    let section = search.section(now);
    let bytes = terminal_bytes(&section, 80, 24);

    // vt100 exposes no title; the check is that no title sequence is emitted.
    assert!(
        !bytes
            .windows(4)
            .any(|window| window == [0x1b, b']', b'0', b';']),
        "no title-setting sequence leaves the box"
    );
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("pwned"), "the text stays visible");
    assert!(
        !text.contains('\u{202e}'),
        "the override becomes a stand-in"
    );
    let screen = screen_rows(&bytes, 80, 24).join("\n");
    assert!(screen.contains("pwned"), "{screen}");
    let entry = screen
        .lines()
        .find(|line| line.contains("pwned"))
        .expect("the entry row");
    let x = entry.find('x').expect("x shows");
    let y = entry.find('y').expect("y shows");
    assert!(x < y, "order is kept: {screen}");
}
