//! The overlay cursor: show, move and hide. Issue #253.

mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use support::{ByteSink, Vt};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// A terminal with a two-row viewport, its sink and a fed emulator.
fn open_terminal(cols: u16, rows: u16) -> (InlineTerminal<ByteSink>, ByteSink, Vt) {
    let sink = ByteSink::default();
    let term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        support::probe(false, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    let mut vt = Vt::new(cols, rows);
    vt.absorb(&sink);
    (term, sink, vt)
}

fn paint(
    text: &str,
    cursor: Option<(u16, u16)>,
) -> impl FnOnce(Rect, &mut Buffer) -> Option<(u16, u16)> + '_ {
    move |area, buf| {
        let row = Rect::new(area.x, area.y + cursor.map_or(0, |(_, y)| y), area.width, 1);
        ratatui::widgets::Widget::render(Paragraph::new(text), row, buf);
        cursor
    }
}

#[test]
fn overlay_cursor_shows_at_the_given_position() {
    let (mut term, sink, mut vt) = open_terminal(COLS, ROWS);
    term.open_overlay().expect("open");
    vt.absorb(&sink);
    term.draw_overlay_with_cursor(paint("cursor here", Some((12, 3))))
        .expect("draw with cursor");
    vt.absorb(&sink);
    assert!(vt.alternate());
    assert!(!vt.cursor_hidden(), "a position shows the cursor");
    assert_eq!(vt.cursor(), (12, 3));
    assert!(vt.rows()[3].starts_with("cursor here"));
}

#[test]
fn overlay_cursor_moves_without_redrawing_content() {
    let (mut term, sink, mut vt) = open_terminal(COLS, ROWS);
    term.open_overlay().expect("open");
    vt.absorb(&sink);
    term.draw_overlay_with_cursor(paint("same", Some((1, 1))))
        .expect("first draw");
    let first = vt.absorb(&sink);
    assert!(!first.is_empty());
    term.draw_overlay_with_cursor(paint("same", Some((5, 1))))
        .expect("second draw");
    let second = vt.absorb(&sink);
    assert!(!second.is_empty(), "a cursor-only move still writes");
    assert_eq!(vt.cursor(), (5, 1));
    assert!(vt.rows()[1].starts_with("same"));
}

#[test]
fn overlay_cursor_hides_on_none_and_on_close() {
    let (mut term, sink, mut vt) = open_terminal(COLS, ROWS);
    term.open_overlay().expect("open");
    vt.absorb(&sink);
    term.draw_overlay_with_cursor(paint("shown", Some((2, 2))))
        .expect("draw with cursor");
    vt.absorb(&sink);
    assert!(!vt.cursor_hidden());
    term.draw_overlay_with_cursor(paint("shown", None))
        .expect("draw hidden");
    vt.absorb(&sink);
    assert!(vt.cursor_hidden(), "None hides the cursor");
    term.draw_overlay_with_cursor(paint("shown", Some((2, 2))))
        .expect("draw with cursor");
    vt.absorb(&sink);
    assert!(!vt.cursor_hidden());
    term.close_overlay().expect("close");
    vt.absorb(&sink);
    assert!(!vt.alternate());
    assert!(vt.cursor_hidden(), "closing hides the cursor");
}
