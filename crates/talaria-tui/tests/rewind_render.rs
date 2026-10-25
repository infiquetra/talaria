//! Rewind overlay drawing and keys. Issue 233. Checkpoint rewind and diff.
//!
//! Tests 37 to 43 call `draw_rewind` first. Test 44 calls `key_to_rewind` first. The diff
//! render options type is not on this branch, so the draw takes the view's own document.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use support::{Alac, ByteSink, probe};
use talaria_core::rewind::{
    DiffDoc, LineStyle, RewindKey, RewindScreen, RewindState, RewindView, ViewLine, ViewRow,
};
use talaria_tui::rewind::{RewindTheme, draw_rewind, key_to_rewind};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn theme() -> RewindTheme {
    RewindTheme {
        selected: Style::default(),
        dim: Style::default(),
        warning: Style::default(),
        accent: Style::default(),
    }
}

fn draw(view: &RewindView<'_>, cols: u16, rows: u16) -> Buffer {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    draw_rewind(view, area, &mut buf, &theme());
    buf
}

fn row_text(buf: &Buffer, y: u16) -> String {
    let mut text = String::new();
    for x in 0..buf.area.width {
        text.push_str(buf[(x, y)].symbol());
    }
    text.trim_end().to_string()
}

fn rows_of(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height).map(|y| row_text(buf, y)).collect()
}

fn line(text: &str, style: LineStyle) -> ViewLine {
    ViewLine {
        text: text.to_string(),
        style,
    }
}

fn turns_view() -> RewindView<'static> {
    RewindView {
        screen: RewindScreen::Turns,
        title: "Rewind · choose a prompt to go back to".to_string(),
        rows: vec![
            ViewRow {
                mark: true,
                label: "#3".to_string(),
                age: "2m ago".to_string(),
                clock: String::new(),
                text: "change alpha to beta in notes.txt",
                tail: "files",
            },
            ViewRow {
                mark: false,
                label: "#2".to_string(),
                age: "5m ago".to_string(),
                clock: String::new(),
                text: "create notes.txt with the line alpha",
                tail: "files",
            },
            ViewRow {
                mark: false,
                label: "#1".to_string(),
                age: "9m ago".to_string(),
                clock: String::new(),
                text: "hello",
                tail: "·",
            },
            ViewRow {
                mark: false,
                label: "#4".to_string(),
                age: String::new(),
                clock: String::new(),
                text: "no time",
                tail: "",
            },
        ],
        selected: 0,
        first_visible: 0,
        body: Vec::new(),
        diff: None,
        diff_scroll: 0,
        footer: "↑↓ choose · enter open · esc close".to_string(),
    }
}

#[test]
fn turn_list_layout_80x24() {
    let buf = draw(&turns_view(), 80, 24);
    let rows = rows_of(&buf);
    assert_eq!(rows[0], " Rewind · choose a prompt to go back to");
    assert_eq!(
        rows[1],
        " › #3  2m ago    change alpha to beta in notes.txt                        files"
    );
    assert_eq!(
        rows[2],
        "   #2  5m ago    create notes.txt with the line alpha                     files"
    );
    assert_eq!(
        rows[3],
        "   #1  9m ago    hello                                                        ·"
    );
    assert!(
        rows[4].contains("no time") && !rows[4].contains("ago"),
        "a missing time leaves the age blank: {}",
        rows[4]
    );
    assert_eq!(rows[23], "↑↓ choose · enter open · esc close");
}

#[test]
fn actions_and_confirm_panels() {
    let state = RewindState::default();
    let mut view = state.view(0);
    view.screen = RewindScreen::Actions;
    view.title = "Prompt #3 · 2m ago".to_string();
    view.body = vec![
        line("change alpha to beta in notes.txt", LineStyle::Normal),
        line(
            "c  Rewind the conversation to before this prompt (files stay as they are)",
            LineStyle::Normal,
        ),
        line(
            "f  Also restore files to checkpoint abcdef01 · 14:02 · before patch",
            LineStyle::Normal,
        ),
        line("d  Show file changes since this prompt", LineStyle::Normal),
        line(
            "This is the oldest checkpoint Hermes still keeps. Edits made after this prompt but before 14:02 may not be undone.",
            LineStyle::Warning,
        ),
        line(
            "Checkpoints belong to the folder: edits other sessions made here since then are undone too. Files changed since Hermes last wrote them (by hand, or by a shell command) are kept; /rollback 3 --all overwrites them.",
            LineStyle::Warning,
        ),
        line("esc back", LineStyle::Dim),
    ];
    let buf = draw(&view, 80, 24);
    let text = rows_of(&buf).join("\n");
    assert!(text.contains("Also restore files to checkpoint abcdef01"));
    assert!(text.contains("oldest checkpoint Hermes still keeps"));
    assert!(text.contains("Checkpoints belong to the folder"));

    view.screen = RewindScreen::Confirm;
    view.body = vec![
        line(
            "Remove prompt #3 and everything after it (6 messages) from the conversation?",
            LineStyle::Normal,
        ),
        line(
            "Hermes will not rewind while a reply runs or prompts are queued (2 queued).",
            LineStyle::Warning,
        ),
        line("esc back", LineStyle::Dim),
    ];
    let blocked = draw(&view, 80, 24);
    let blocked_text = rows_of(&blocked).join("\n");
    assert!(blocked_text.contains("2 queued"));
    assert!(!blocked_text.contains("enter confirm"));
}

#[test]
fn checkpoint_list_layout() {
    let view = RewindView {
        screen: RewindScreen::Checkpoints,
        title: "Checkpoints · newest first".to_string(),
        rows: vec![
            ViewRow {
                mark: true,
                label: "1".to_string(),
                age: "2m ago".to_string(),
                clock: "14:02".to_string(),
                text: "before patch",
                tail: "",
            },
            ViewRow {
                mark: false,
                label: "2".to_string(),
                age: "5m ago".to_string(),
                clock: "13:58".to_string(),
                text: "before write_file",
                tail: "",
            },
            ViewRow {
                mark: false,
                label: "3".to_string(),
                age: "9m ago".to_string(),
                clock: "13:51".to_string(),
                text: "before write_file",
                tail: "",
            },
        ],
        selected: 0,
        first_visible: 0,
        body: Vec::new(),
        diff: None,
        diff_scroll: 0,
        footer: "↑↓ choose · enter diff · r restore · esc close".to_string(),
    };
    let buf = draw(&view, 80, 24);
    let rows = rows_of(&buf);
    assert_eq!(rows[0], "Checkpoints · newest first");
    assert!(rows[1].contains('1') && rows[1].contains("14:02") && rows[1].contains("before patch"));
    assert!(rows[2].contains("13:58") && rows[2].contains("before write_file"));
    assert!(rows[3].contains("13:51"));
    assert_eq!(rows[23], "↑↓ choose · enter diff · r restore · esc close");
}

#[test]
fn diff_view_renders_with_p2_01_and_scrolls() {
    let title = "Changes since the oldest kept checkpoint".to_string();
    let doc = DiffDoc::Text {
        title: title.clone(),
        subtitle: Some(
            "Includes every change in this folder since then: the agent's, yours and other sessions'."
                .to_string(),
        ),
        stat: " notes.txt | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)".to_string(),
        diff: "--- a/notes.txt\n+++ b/notes.txt\n@@ -1 +1 @@\n-alpha\n+beta\n".to_string(),
        stat_only: false,
        cut_note: Some(
            "Hermes cut this diff to fit one message: showing 300 of 900 lines.".to_string(),
        ),
    };
    let view = RewindView {
        screen: RewindScreen::Diff,
        title,
        rows: Vec::new(),
        selected: 0,
        first_visible: 0,
        body: vec![line("Comparing files…", LineStyle::Dim)],
        diff: Some(&doc),
        diff_scroll: 0,
        footer: "↑↓ scroll · pgup/pgdn page · home/end · esc back".to_string(),
    };
    // The 88-character subtitle fits one row at 100 columns.
    let buf = draw(&view, 100, 24);
    let text = rows_of(&buf).join("\n");
    assert!(text.contains(
        "Includes every change in this folder since then: the agent's, yours and other sessions'."
    ));
    assert!(text.contains("showing 300 of 900 lines"));
    assert!(text.contains("- alpha"));
    assert!(text.contains("+ beta"));

    let loading_state = RewindState::default();
    let mut loading = loading_state.view(0);
    loading.screen = RewindScreen::Loading;
    loading.body = vec![line("Comparing files…", LineStyle::Dim)];
    let loading_buf = draw(&loading, 80, 24);
    assert!(
        rows_of(&loading_buf)
            .join("\n")
            .contains("Comparing files…")
    );
}

#[test]
fn narrow_and_tiny_terminals() {
    let wide = draw(&turns_view(), 49, 24);
    let mid = rows_of(&wide).join("\n");
    assert!(
        !mid.contains("2m ago"),
        "below 50 columns the age column is dropped"
    );
    assert!(mid.contains("#3"));
    assert!(mid.contains("change alpha to beta"));

    let narrow = draw(&turns_view(), 29, 24);
    let slim = rows_of(&narrow).join("\n");
    assert!(slim.contains("#3"));
    assert!(!slim.contains("14:02"));
    assert!(!slim.contains("2m ago"));

    let tiny = draw(&turns_view(), 80, 2);
    assert!(
        rows_of(&tiny).iter().all(|row| row.is_empty()),
        "under 3 rows the overlay draws nothing"
    );
}

#[test]
fn hostile_text_never_reaches_the_terminal() {
    let prompt = "a\u{1b}]0;pwned\u{7}b";
    let message = "clear\u{1b}[2Jnow";
    let diff_line = "left\u{202e}right";
    let rendered = "\u{1b}[31msecret from rendered\u{1b}[0m";
    let doc = DiffDoc::Text {
        title: "Checkpoint abcd1234 compared with the files now".to_string(),
        subtitle: None,
        stat: message.to_string(),
        diff: diff_line.to_string(),
        stat_only: false,
        cut_note: None,
    };
    let view = RewindView {
        screen: RewindScreen::Diff,
        title: prompt.to_string(),
        rows: vec![ViewRow {
            mark: true,
            label: "#1".to_string(),
            age: String::new(),
            clock: String::new(),
            text: prompt,
            tail: "",
        }],
        selected: 0,
        first_visible: 0,
        body: vec![line(message, LineStyle::Normal)],
        diff: Some(&doc),
        diff_scroll: 0,
        footer: "esc back".to_string(),
    };
    let buf = draw(&view, 80, 24);
    let text = rows_of(&buf).join("\n");
    assert!(text.contains("␛]0;pwned␇"), "{text}");
    assert!(text.contains("␛[2J"), "{text}");
    assert!(text.contains("‹U+202E›"), "{text}");
    assert!(!text.contains(rendered));
    assert!(!text.contains("secret from rendered"));
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let symbol = buf[(x, y)].symbol();
            assert!(!symbol.contains('\u{1b}'));
            assert!(!symbol.contains('\u{7}'));
            assert!(!symbol.contains('\u{202e}'));
        }
    }
}

#[test]
fn closing_overlay_leaves_nothing_in_scrollback() {
    let mut view = turns_view();
    view.title = "REWIND OVERLAY MARKER".to_string();
    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);
    draw_rewind(&view, area, &mut buf, &theme());

    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .expect("terminal");
    let mut alac = Alac::new(80, 24);
    term.open_overlay().expect("open overlay");
    let opened = alac.absorb(&sink);
    term.draw_overlay(|overlay_area, overlay_buf| {
        draw_rewind(&view, overlay_area, overlay_buf, &theme());
    })
    .expect("draw overlay");
    let drawn = alac.absorb(&sink);
    term.close_overlay().expect("close overlay");
    let closed = alac.absorb(&sink);
    term.commit_lines(vec![Line::from("kept scrollback cell")])
        .expect("commit");
    let committed = alac.absorb(&sink);
    let bytes = [&opened[..], &drawn[..], &closed[..], &committed[..]].concat();
    let screen = alac.history_and_screen().join("\n");
    assert!(screen.contains("kept scrollback cell"));
    assert!(!screen.contains("REWIND OVERLAY MARKER"));
    // The overlay opens and leaves the alternate screen; drawing it switches no mode.
    assert!(closed.windows(8).any(|window| window == b"\x1b[?1049l"));
    assert!(!drawn.windows(8).any(|window| window == b"\x1b[?1049h"));
    assert!(!drawn.windows(8).any(|window| window == b"\x1b[?2004h"));
    assert!(!bytes.windows(2).any(|window| window == b"\x1b]"));
}

fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

fn plain(code: KeyCode) -> KeyEvent {
    press(code, KeyModifiers::empty())
}

#[test]
fn keys_map_to_inputs() {
    let up = key_to_rewind(&plain(KeyCode::Up), RewindScreen::Turns);
    assert_eq!(up, Some(RewindKey::Up));
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('k')), RewindScreen::Turns),
        Some(RewindKey::Up)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('j')), RewindScreen::Checkpoints),
        Some(RewindKey::Down)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('y')), RewindScreen::Turns),
        None
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Tab), RewindScreen::Actions),
        None
    );
    assert_eq!(
        key_to_rewind(
            &press(KeyCode::Char('c'), KeyModifiers::CONTROL),
            RewindScreen::Diff
        ),
        Some(RewindKey::Close)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('c')), RewindScreen::Actions),
        Some(RewindKey::C)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('f')), RewindScreen::Actions),
        Some(RewindKey::F)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('d')), RewindScreen::Checkpoints),
        Some(RewindKey::D)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('r')), RewindScreen::Checkpoints),
        Some(RewindKey::R)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Enter), RewindScreen::Confirm),
        Some(RewindKey::Enter)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char('n')), RewindScreen::CheckpointConfirm),
        Some(RewindKey::Esc)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Esc), RewindScreen::Loading),
        Some(RewindKey::Esc)
    );
    assert_eq!(
        key_to_rewind(&plain(KeyCode::Char(' ')), RewindScreen::Diff),
        Some(RewindKey::PageDown)
    );
}
