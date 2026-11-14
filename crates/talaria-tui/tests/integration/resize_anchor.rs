//! A resize leaves exactly one live region and every committed line, whatever the terminal does
//! with its own screen when its size changes.
//!
//! Terminals disagree about that. Some pull rows back from scrollback when they grow and wrap
//! long rows when they narrow; others add blank rows at the bottom and cut long rows. Two
//! emulators stand for the two families: `alacritty_terminal` (pulls back, wraps) and `vt100`
//! (adds blank rows, cuts). The writer must not depend on which family it is talking to.

use crate::support;

use std::io;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use support::{Alac, ByteSink, FailingWriter, assert_tags_once_in_order, probe, tagged_from};
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const PLACEHOLDER: &str = "Type a message. Enter sends, Shift+Enter adds a line.";

/// A four row boxed composer as wide as the screen: top border, placeholder, bottom border and
/// a status row.
fn composer(cols: u16) -> Vec<PlainTextSection> {
    boxed_draft(cols, &[PLACEHOLDER])
}

/// A boxed composer holding `draft` lines as wide as the screen, ending with the bottom border
/// and status row.
fn boxed_draft(cols: u16, draft: &[&str]) -> Vec<PlainTextSection> {
    let inner = usize::from(cols) - 2;
    let mut sections = vec![PlainTextSection::new(&format!("╭{}╮", "─".repeat(inner)))];
    for (index, entry) in draft.iter().enumerate() {
        let prefix = if index == 0 { " > " } else { "   " };
        let text = format!("{prefix}{entry}");
        sections.push(PlainTextSection::new(&format!(
            "│{text}{}│",
            " ".repeat(inner - text.chars().count())
        )));
    }
    sections.push(PlainTextSection::new(&format!("╰{}╯", "─".repeat(inner))));
    sections.push(PlainTextSection::new(" status"));
    sections
}

/// `rows` streaming rows above the composer, as a running turn shows them.
fn streaming(cols: u16, rows: u16) -> Vec<PlainTextSection> {
    let mut sections: Vec<PlainTextSection> = (1..=rows)
        .map(|n| PlainTextSection::new(&format!("STREAM {n}")))
        .collect();
    sections.extend(composer(cols));
    sections
}

fn view_of(sections: &[PlainTextSection]) -> LiveView<'_> {
    LiveView {
        sections: sections
            .iter()
            .map(|section| section as &dyn LiveSection)
            .collect(),
    }
}

/// What a test needs from an emulator.
trait Screen {
    fn feed(&mut self, bytes: &[u8]);
    fn resize(&mut self, cols: u16, rows: u16);
    /// The visible rows, trailing blanks trimmed.
    fn visible(&self) -> Vec<String>;
    /// History, oldest first, then the visible rows.
    fn everything(&self) -> Vec<String>;
}

impl Screen for Alac {
    fn feed(&mut self, bytes: &[u8]) {
        Alac::feed(self, bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        Alac::resize(self, cols, rows);
    }

    fn visible(&self) -> Vec<String> {
        self.screen()
    }

    fn everything(&self) -> Vec<String> {
        self.history_and_screen()
    }
}

/// The `vt100` grid: a grow adds blank rows at the bottom, a shrink cuts rows from the bottom
/// and cells from the right, nothing wraps and there is no scrollback to pull back.
struct Cutting {
    parser: vt100::Parser,
    cols: u16,
}

impl Cutting {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, 0),
            cols,
        }
    }
}

impl Screen for Cutting {
    fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.parser.screen_mut().set_size(rows, cols);
        self.cols = cols;
    }

    fn visible(&self) -> Vec<String> {
        self.parser
            .screen()
            .rows(0, self.cols)
            .map(|row| row.trim_end().to_string())
            .collect()
    }

    fn everything(&self) -> Vec<String> {
        self.visible()
    }
}

/// The terminal under test, its sink and the emulator that reads the sink.
struct Rig<S: Screen> {
    term: InlineTerminal<ByteSink>,
    sink: ByteSink,
    screen: S,
    cols: u16,
    committed: usize,
}

impl<S: Screen> Rig<S> {
    /// `screen` already holds `shell` rows of earlier output, so the cursor is on row `shell`.
    fn new(mut screen: S, size: (u16, u16), shell: u16) -> Self {
        for n in 1..=shell {
            screen.feed(format!("shell {n:03}\r\n").as_bytes());
        }
        let sink = ByteSink::default();
        let term = InlineTerminal::new(
            sink.clone(),
            size,
            4,
            probe(false, false, Some((0, shell))),
            RestoreLink::detached(),
        )
        .expect("new terminal");
        let mut rig = Self {
            term,
            sink,
            screen,
            cols: size.0,
            committed: 0,
        };
        rig.pump();
        rig
    }

    fn pump(&mut self) {
        let bytes = self.sink.take();
        self.screen.feed(&bytes);
    }

    fn commit(&mut self, count: usize) {
        self.term
            .commit_lines(tagged_from(self.committed + 1, count))
            .expect("commit");
        self.committed += count;
        self.pump();
    }

    fn commit_wide(&mut self, width: usize) {
        self.committed += 1;
        let tag = format!("line {:04} ", self.committed);
        let line = Line::from(format!("{tag}{}", "w".repeat(width - tag.len())));
        self.term.commit_lines(vec![line]).expect("commit wide");
        self.pump();
    }

    fn draw(&mut self, streaming_rows: u16) {
        let sections = streaming(self.cols, streaming_rows);
        self.term.draw_live(&view_of(&sections)).expect("draw");
        self.pump();
    }

    /// One turn: the live region grows while text streams, the lines are committed, and the
    /// region shrinks back to the composer, which then sits above blank rows.
    fn turn(&mut self, streamed: u16, lines: usize) {
        self.draw(streamed);
        self.commit(lines);
        self.draw(0);
    }

    /// Grow the boxed composer with a multiline draft, then clear it back to the placeholder box.
    fn draft_and_clear(&mut self, draft: &[&str]) {
        let sections = boxed_draft(self.cols, draft);
        self.term
            .draw_live(&view_of(&sections))
            .expect("draw draft");
        self.pump();
        self.draw(0);
    }

    /// Stream rows above the composer and interrupt without committing, leaving blank rows below
    /// the shrunken composer.
    fn interrupt(&mut self, streamed: u16) {
        self.draw(streamed);
        self.draw(0);
    }

    fn overlay_round_trip(&mut self) {
        self.term.open_overlay().expect("open overlay");
        self.term
            .draw_overlay(|area: Rect, buf: &mut Buffer| {
                buf.set_string(area.x, area.y, "OVERLAY", Style::default());
            })
            .expect("draw overlay");
        self.term.close_overlay().expect("close overlay");
        self.pump();
        self.draw(0);
    }

    /// The terminal changes size first; the program hears of it and redraws.
    fn resize(&mut self, cols: u16, rows: u16) {
        self.screen.resize(cols, rows);
        self.term.resize(cols, rows).expect("resize");
        self.cols = cols;
        self.pump();
        self.draw(0);
    }
}

/// Rows that hold the given piece of the composer.
fn rows_with(rows: &[String], needle: &str) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.contains(needle))
        .map(|(index, _)| index)
        .collect()
}

/// One composer in everything the emulator holds, drawn as four consecutive visible rows.
fn assert_one_composer<S: Screen>(rig: &Rig<S>, step: &str) -> usize {
    let all = rig.screen.everything();
    for piece in ["╭", PLACEHOLDER, "╰"] {
        let found = rows_with(&all, piece);
        assert_eq!(
            found.len(),
            1,
            "{step}: `{piece}` should be on one row, found rows {found:?} in {all:#?}"
        );
    }
    let visible = rig.screen.visible();
    let top = rows_with(&visible, "╭")[0];
    assert!(
        visible[top + 1].contains(PLACEHOLDER),
        "{step}: {visible:#?}"
    );
    assert!(visible[top + 2].starts_with('╰'), "{step}: {visible:#?}");
    assert_eq!(visible[top + 3], " status", "{step}: {visible:#?}");
    top
}

/// The last committed line sits on the row directly above the composer.
fn assert_history_ends_at_composer<S: Screen>(rig: &Rig<S>, top: usize, step: &str) {
    let visible = rig.screen.visible();
    let last = format!("line {:04}", rig.committed);
    assert!(top >= 1, "{step}: no row above the composer");
    assert!(
        visible[top - 1].starts_with(&last),
        "{step}: `{last}` should be directly above the composer: {visible:#?}"
    );
}

/// A window that grows by two rows and shrinks back, five times, while the composer sits above
/// blank rows. A terminal that adds blank rows at the bottom moves nothing, so the composer
/// must stay where it is.
#[test]
fn growing_rows_without_scrollback_pull_keeps_one_composer() {
    let mut rig = Rig::new(Cutting::new(145, 41), (145, 41), 25);
    rig.draw(0);
    rig.commit(10);
    rig.overlay_round_trip();
    rig.draft_and_clear(&["one", "two"]);
    rig.turn(8, 14);
    rig.interrupt(8);
    rig.commit(1);
    let top = assert_one_composer(&rig, "before any resize");
    assert_history_ends_at_composer(&rig, top, "before any resize");

    for cycle in 1..=5 {
        for rows in [43, 41] {
            rig.resize(145, rows);
            let step = format!("cycle {cycle}, {rows} rows");
            let top = assert_one_composer(&rig, &step);
            assert_history_ends_at_composer(&rig, top, &step);
        }
    }

    // The session goes on: an overlay, a draft clear, an interrupt, a turn and a commit after the
    // resizes.
    rig.overlay_round_trip();
    rig.draft_and_clear(&["after", "resize"]);
    rig.interrupt(4);
    rig.turn(6, 3);
    let top = assert_one_composer(&rig, "after the resizes");
    assert_history_ends_at_composer(&rig, top, "after the resizes");
}

/// The same growth on a terminal that does pull rows back, when it holds history the program
/// never counted: a second launch in one window starts mid-screen above old scrollback.
#[test]
fn growing_rows_with_uncounted_scrollback_keeps_history_and_one_composer() {
    let mut alac = Alac::with_shell_history(145, 41);
    // The earlier program left the cursor on row 29 with nothing below it.
    Alac::feed(&mut alac, b"\x1b[30;1H\x1b[J");
    let sink = ByteSink::default();
    let term = InlineTerminal::new(
        sink.clone(),
        (145, 41),
        4,
        probe(false, false, Some((0, 29))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    let mut rig = Rig {
        term,
        sink,
        screen: alac,
        cols: 145,
        committed: 0,
    };
    rig.pump();
    rig.draw(0);
    rig.commit(2);

    for rows in [43, 41, 45, 41] {
        rig.resize(145, rows);
        let step = format!("{rows} rows");
        let top = assert_one_composer(&rig, &step);
        assert_history_ends_at_composer(&rig, top, &step);
        assert_tags_once_in_order(&rig.screen.everything(), 2);
    }
}

/// The issue's case on a terminal that cuts: a session started on the 200 by 120 screen of an
/// unattached terminal is first viewed at 145 by 41, in one step and in three.
#[test]
fn narrowing_on_a_cutting_terminal_keeps_history_and_one_composer() {
    for steps in [vec![(145, 41)], vec![(170, 80), (145, 60), (145, 41)]] {
        let mut rig = Rig::new(Cutting::new(200, 120), (200, 120), 5);
        rig.draw(0);
        rig.commit(6);
        rig.overlay_round_trip();
        rig.draft_and_clear(&["one", "two"]);
        rig.interrupt(8);
        rig.turn(8, 6);
        for (cols, rows) in steps {
            rig.resize(cols, rows);
            let step = format!("{cols}x{rows}");
            let top = assert_one_composer(&rig, &step);
            assert_history_ends_at_composer(&rig, top, &step);
            assert_tags_once_in_order(&rig.screen.everything(), 12);
        }
    }
}

/// The issue's case on a terminal that wraps, for a short session and for one longer than the
/// smaller screen, in one step and in three.
#[test]
fn narrowing_on_a_wrapping_terminal_keeps_history_and_one_composer() {
    for turns in [1, 4] {
        for steps in [vec![(145, 41)], vec![(170, 80), (145, 60), (145, 41)]] {
            let mut rig = Rig::new(Alac::new(200, 120), (200, 120), 5);
            rig.draw(0);
            rig.commit(6);
            rig.commit_wide(190);
            rig.commit_wide(160);
            rig.overlay_round_trip();
            rig.draft_and_clear(&["one", "two"]);
            rig.interrupt(8);
            for _ in 0..turns {
                rig.turn(8, 12);
            }
            for (cols, rows) in steps {
                rig.resize(cols, rows);
                let step = format!("{turns} turns, {cols}x{rows}");
                let top = assert_one_composer(&rig, &step);
                assert_history_ends_at_composer(&rig, top, &step);
                assert_tags_once_in_order(&rig.screen.everything(), rig.committed);
            }
        }
    }
}

/// A resize while an overlay covers the main screen: when the overlay closes there is still one
/// composer and every committed line. The composer may then sit lower than the last line.
#[test]
fn resizing_under_an_overlay_keeps_history_and_one_composer() {
    for (cols, rows) in [(145, 43), (120, 30)] {
        let mut rig = Rig::new(Cutting::new(145, 41), (145, 41), 25);
        rig.draw(0);
        rig.commit(10);
        rig.turn(8, 14);

        rig.term.open_overlay().expect("open overlay");
        rig.pump();
        rig.screen.resize(cols, rows);
        rig.term
            .resize(cols, rows)
            .expect("resize under the overlay");
        rig.cols = cols;
        rig.pump();
        rig.term.close_overlay().expect("close overlay");
        rig.pump();
        rig.draw(0);

        let step = format!("{cols}x{rows} under an overlay");
        assert_one_composer(&rig, &step);
        let visible = rig.screen.visible();
        let tags: Vec<&String> = visible
            .iter()
            .filter(|row| row.starts_with("line "))
            .collect();
        assert_eq!(
            tags.last().map(|row| row.as_str()),
            Some("line 0024"),
            "{step}: the newest committed line is gone: {visible:#?}"
        );
        for pair in tags.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{step}: lines out of order: {visible:#?}"
            );
        }
    }
}

/// A floating composer on the cutting terminal after one two row grow: the state in which the
/// row the program believes in and the row the terminal uses can differ.
fn resized_cutting_rig() -> Rig<Cutting> {
    let mut rig = Rig::new(Cutting::new(145, 41), (145, 41), 25);
    rig.draw(0);
    rig.commit(10);
    rig.turn(8, 14);
    rig.resize(145, 43);
    rig
}

/// Leaving after a resize clears the composer where it is and nothing above it.
#[test]
fn restore_after_a_resize_clears_the_composer_and_keeps_history() {
    let mut rig = resized_cutting_rig();
    rig.term.restore().expect("restore");
    rig.pump();
    let visible = rig.screen.visible();
    for piece in ["╭", PLACEHOLDER, "╰", " status"] {
        assert!(
            rows_with(&visible, piece).is_empty(),
            "`{piece}` survived the restore: {visible:#?}"
        );
    }
    let last = rows_with(&visible, "line 0024");
    assert_eq!(last.len(), 1, "{visible:#?}");
    assert_eq!(
        rig.screen.parser.screen().cursor_position(),
        (u16::try_from(last[0] + 1).expect("row"), 0),
        "the shell's prompt belongs on the row after the last line"
    );
}

/// Handing the terminal to a child program after a resize does the same.
#[test]
fn release_after_a_resize_clears_the_composer_and_keeps_history() {
    let mut rig = resized_cutting_rig();
    rig.term.release().expect("release");
    rig.pump();
    let visible = rig.screen.visible();
    for piece in ["╭", PLACEHOLDER, "╰", " status"] {
        assert!(
            rows_with(&visible, piece).is_empty(),
            "`{piece}` survived the release: {visible:#?}"
        );
    }
    let last = rows_with(&visible, "line 0024");
    assert_eq!(last.len(), 1, "{visible:#?}");
    assert_eq!(
        rig.screen.parser.screen().cursor_position(),
        (u16::try_from(last[0] + 1).expect("row"), 0),
        "the child starts on the row after the last line"
    );
}

/// A write error after a resize: the next write recovers by taking the bottom rows, the line
/// committed after the error appears once, and the composer is whole.
#[test]
fn write_error_after_a_resize_recovers_in_the_bottom_rows() {
    let (writer, sink) = FailingWriter::new();
    let mut screen = Cutting::new(145, 41);
    for n in 1..=25 {
        screen.feed(format!("shell {n:03}\r\n").as_bytes());
    }
    let mut term = InlineTerminal::new(
        writer.clone(),
        (145, 41),
        4,
        probe(false, false, Some((0, 25))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    screen.feed(&sink.take());
    let sections = composer(145);
    term.draw_live(&view_of(&sections)).expect("draw");
    term.commit_lines(tagged_from(1, 3)).expect("commit");
    screen.feed(&sink.take());
    screen.resize(145, 43);
    term.resize(145, 43).expect("resize");
    term.draw_live(&view_of(&sections)).expect("draw");
    screen.feed(&sink.take());

    writer.fail_after(12);
    let error = term
        .commit_lines(tagged_from(4, 2))
        .expect_err("the writer is broken, so the commit must fail");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    screen.feed(&sink.take());

    writer.heal();
    term.commit_lines(tagged_from(6, 1))
        .expect("the healed writer commits again");
    let recovery = sink.take();
    assert!(
        recovery.starts_with(b"\x1b[r"),
        "the first write after an error starts by recovering: {:?}",
        &recovery[..recovery.len().min(24)]
    );
    screen.feed(&recovery);
    term.draw_live(&view_of(&sections)).expect("draw");
    screen.feed(&sink.take());

    let visible = screen.visible();
    assert_eq!(visible.len(), 43);
    assert!(visible[39].starts_with('╭'), "{visible:#?}");
    assert!(visible[40].contains(PLACEHOLDER), "{visible:#?}");
    assert!(visible[41].starts_with('╰'), "{visible:#?}");
    assert_eq!(visible[42], " status", "{visible:#?}");
    assert_eq!(visible[38], "line 0006", "{visible:#?}");
    for number in 1..=6 {
        let wanted = format!("line {number:04}");
        let seen = rows_with(&visible, &wanted).len();
        assert!(seen <= 1, "`{wanted}` appears {seen} times: {visible:#?}");
    }
}
