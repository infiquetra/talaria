//! The full-screen overlay: it takes the alternate screen and hands the inline view back exactly
//! as it was, and lines committed or screen sizes changed while it is open take effect once, after
//! it closes.

mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use support::{
    Alac, ByteSink, Vt, assert_live_only_in_viewport, assert_tags_once_in_order,
    live_view_sections, probe, tagged,
};
use talaria_tui::live::{FrameStats, LiveSection, LiveView, PlainTextSection};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// What the overlay tests draw into the alternate screen.
const OVERLAY_TEXT: &str = "OVERLAY - press Esc";

/// A live view that borrows the given sections.
fn view_of(sections: &[PlainTextSection]) -> LiveView<'_> {
    LiveView {
        sections: sections
            .iter()
            .map(|section| section as &dyn LiveSection)
            .collect(),
    }
}

/// An 80x24 terminal with a two-row viewport at the bottom, the sink it writes into and the
/// bytes its set-up wrote.
fn open_terminal() -> (InlineTerminal<ByteSink>, ByteSink, Vec<u8>) {
    let sink = ByteSink::default();
    let term = InlineTerminal::new(
        sink.clone(),
        (COLS, ROWS),
        2,
        probe(false, false, Some((0, ROWS - 1))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    let setup = sink.take();
    (term, sink, setup)
}

/// Draw the overlay text in the top left corner of the full-screen buffer.
fn draw_overlay_text(term: &mut InlineTerminal<ByteSink>, size: (u16, u16)) -> FrameStats {
    term.draw_overlay(|area: Rect, buf: &mut Buffer| {
        assert_eq!(
            area,
            Rect::new(0, 0, size.0, size.1),
            "the overlay covers the whole screen"
        );
        buf.set_string(0, 0, OVERLAY_TEXT, Style::default());
    })
    .expect("draw overlay")
}

/// The 24 screen rows with the given rows starting at `first`, blank elsewhere.
fn screen_with(first: usize, rows: &[&str]) -> Vec<String> {
    let below = usize::from(ROWS) - first - rows.len();
    std::iter::repeat_n("", first)
        .chain(rows.iter().copied())
        .chain(std::iter::repeat_n("", below))
        .map(String::from)
        .collect()
}

#[test]
fn closing_overlay_restores_inline_view() {
    let (mut term, sink, setup) = open_terminal();
    let mut vt = Vt::new(COLS, ROWS);
    vt.feed(&setup);
    let sections = live_view_sections(2);

    term.draw_live(&view_of(&sections)).expect("first frame");
    vt.absorb(&sink);
    term.commit_lines(tagged(3)).expect("commit");
    vt.absorb(&sink);

    let before = vt.rows();
    assert_eq!(
        before,
        screen_with(
            19,
            &["line 0001", "line 0002", "line 0003", "LIVE# 1", "LIVE# 2"]
        ),
        "the inline view before the overlay"
    );
    let cursor_before = vt.cursor();
    assert!(vt.cursor_hidden());
    assert!(!vt.alternate());

    term.open_overlay().expect("open overlay");
    vt.absorb(&sink);
    let frame = draw_overlay_text(&mut term, (COLS, ROWS));
    assert!(frame.drew && frame.bytes > 0);
    vt.absorb(&sink);
    assert!(vt.alternate(), "the overlay is on the alternate screen");
    assert_eq!(
        vt.rows(),
        screen_with(0, &[OVERLAY_TEXT]),
        "only the overlay shows while it is open"
    );

    let idle = term.draw_live(&view_of(&sections)).expect("draw_live");
    assert_eq!(
        idle,
        FrameStats {
            drew: false,
            bytes: 0
        },
        "the live region is not drawn under an overlay"
    );
    assert!(sink.is_empty());

    term.close_overlay().expect("close overlay");
    vt.absorb(&sink);
    term.draw_live(&view_of(&sections))
        .expect("frame after close");
    vt.absorb(&sink);

    assert!(!vt.alternate(), "the main screen is back");
    assert_eq!(vt.rows(), before, "the inline view is exactly as it was");
    assert_eq!(vt.cursor(), cursor_before);
    assert!(vt.cursor_hidden());
}

#[test]
fn commits_during_overlay_appear_once_after_close() {
    let (mut term, sink, setup) = open_terminal();
    let mut alac = Alac::with_shell_history(COLS, ROWS);
    alac.feed(&setup);
    let sections = live_view_sections(2);

    term.draw_live(&view_of(&sections)).expect("first frame");
    alac.absorb(&sink);
    assert_eq!(term.viewport(), Rect::new(0, 22, COLS, 2));

    term.open_overlay().expect("open overlay");
    let mut during = alac.absorb(&sink);
    term.commit_lines(tagged(3)).expect("commit while open");
    draw_overlay_text(&mut term, (COLS, ROWS));
    during.extend(alac.absorb(&sink));
    assert!(alac.alternate());

    alac.resize(COLS, 30);
    term.resize(COLS, 30).expect("resize while open");
    during.extend(alac.absorb(&sink));
    assert_eq!(
        term.viewport(),
        Rect::new(0, 28, COLS, 2),
        "the model follows the growth at once: six rows came back from scrollback"
    );
    draw_overlay_text(&mut term, (COLS, 30));
    during.extend(alac.absorb(&sink));

    assert!(alac.alternate(), "still on the alternate screen");
    let showing = alac.screen();
    assert!(
        showing.iter().all(|row| !row.contains("line 0")),
        "a committed line showed while the overlay was open: {showing:?}"
    );
    assert!(
        !String::from_utf8_lossy(&during).contains("line 0"),
        "a committed line was written while the overlay was open"
    );

    term.close_overlay().expect("close overlay");
    alac.absorb(&sink);
    term.draw_live(&view_of(&sections))
        .expect("frame after close");
    alac.absorb(&sink);

    assert!(!alac.alternate(), "the main screen is back");
    let everything = alac.history_and_screen();
    assert_tags_once_in_order(&everything, 3);
    for number in 1..=100 {
        let shell = format!("shell {number:03}");
        let found = everything.iter().filter(|row| **row == shell).count();
        assert_eq!(found, 1, "`{shell}` should survive once, found {found}");
    }
    let screen = alac.screen();
    assert_live_only_in_viewport(&screen, term.viewport());
    assert_eq!(screen[28], "LIVE# 1");
    assert_eq!(screen[29], "LIVE# 2");
    assert_eq!(
        screen[25..28],
        ["line 0001", "line 0002", "line 0003"],
        "the three queued lines sit directly above the live region"
    );
}
