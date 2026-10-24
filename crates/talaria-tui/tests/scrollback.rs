//! Finished lines reach the terminal's own scrollback exactly once, in order: through plain
//! commits, a viewport at the very top, a viewport that slides down, a live region at its height
//! cap, resizes, overlays, hostile text and a screen that is too small to hold the live region.
//!
//! The terminal under test writes into a byte sink that an `alacritty_terminal` emulator reads, so
//! the assertions look at real scrollback and real reflow.

mod support;

use std::io;

use proptest::prelude::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use support::{
    Alac, ByteSink, FailingWriter, assert_live_only_in_viewport, assert_tags_once_in_order, cells,
    live_view_sections, probe, tagged, tagged_from,
};
use talaria_tui::history::{CommitLedger, PlainCellLines};
use talaria_tui::live::{FrameStats, LiveSection, LiveView, PlainTextSection};
use talaria_tui::terminal::{InlineTerminal, ProbeReply, RestoreLink};

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// The restore sequence's closing bytes: the cursor is shown again.
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";

/// A terminal under test, the sink it writes into and the emulator that reads the sink.
struct Rig {
    term: InlineTerminal<ByteSink>,
    sink: ByteSink,
    alac: Alac,
}

impl Rig {
    fn new(alac: Alac, size: (u16, u16), live_height: u16, reply: ProbeReply) -> Self {
        let sink = ByteSink::default();
        let term = InlineTerminal::new(
            sink.clone(),
            size,
            live_height,
            reply,
            RestoreLink::detached(),
        )
        .expect("new terminal");
        let mut rig = Self { term, sink, alac };
        rig.alac.absorb(&rig.sink);
        rig
    }

    /// 80x24 with a live height of 2, no capabilities and the probe cursor on the bottom row, so
    /// the viewport starts bottom-anchored at row 22.
    fn anchored(alac: Alac) -> Self {
        Self::new(
            alac,
            (COLS, ROWS),
            2,
            probe(false, false, Some((0, ROWS - 1))),
        )
    }

    /// Commit the lines and let the emulator see the bytes. Returns the bytes.
    fn commit_lines(&mut self, lines: Vec<Line<'static>>) -> Vec<u8> {
        self.term.commit_lines(lines).expect("commit_lines");
        self.alac.absorb(&self.sink)
    }

    /// Draw the sections, let the emulator see the bytes and check the report agrees with them.
    fn draw(&mut self, sections: &[PlainTextSection]) -> FrameStats {
        let stats = self.term.draw_live(&view_of(sections)).expect("draw_live");
        let bytes = self.alac.absorb(&self.sink);
        assert_eq!(
            stats.bytes,
            bytes.len(),
            "the frame must report every byte it wrote"
        );
        stats
    }

    /// Resize the emulator and the terminal together. Returns the bytes the terminal wrote.
    fn resize(&mut self, cols: u16, rows: u16) -> Vec<u8> {
        self.alac.resize(cols, rows);
        self.term.resize(cols, rows).expect("resize");
        self.alac.absorb(&self.sink)
    }

    fn open_overlay(&mut self) {
        self.term.open_overlay().expect("open_overlay");
        self.alac.absorb(&self.sink);
    }

    fn draw_overlay(&mut self) {
        self.term
            .draw_overlay(|area: Rect, buf: &mut Buffer| {
                buf.set_string(area.x, area.y, "OVERLAY", Style::default());
            })
            .expect("draw_overlay");
        self.alac.absorb(&self.sink);
    }

    fn close_overlay(&mut self) {
        self.term.close_overlay().expect("close_overlay");
        self.alac.absorb(&self.sink);
    }
}

/// A live view that borrows the given sections.
fn view_of(sections: &[PlainTextSection]) -> LiveView<'_> {
    LiveView {
        sections: sections
            .iter()
            .map(|section| section as &dyn LiveSection)
            .collect(),
    }
}

fn tag_label(number: usize) -> String {
    format!("line {number:04}")
}

/// A screen of `rows` blank rows with the given rows filled in.
fn screen_with(rows: usize, filled: &[(usize, String)]) -> Vec<String> {
    let mut screen = vec![String::new(); rows];
    for (row, text) in filled {
        screen[*row] = text.clone();
    }
    screen
}

/// The row of the one row that holds the tag, which must be unique.
fn tag_row(rows: &[String], number: usize) -> usize {
    let wanted = tag_label(number);
    let found: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.contains(&wanted))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(found.len(), 1, "`{wanted}` should be on one row: {found:?}");
    found[0]
}

/// Tags `line 0001` to `line <n>` sit on consecutive rows: no blank row and no other text is
/// wedged between two of them.
fn assert_tags_adjacent(rows: &[String], n: usize) {
    let mut previous = tag_row(rows, 1);
    for number in 2..=n {
        let at = tag_row(rows, number);
        assert_eq!(
            at,
            previous + 1,
            "`{}` should sit directly below `{}`",
            tag_label(number),
            tag_label(number - 1)
        );
        previous = at;
    }
}

/// How many tags appear in the rows, counting repeats.
fn count_tags(rows: &[String]) -> usize {
    rows.iter().map(|row| row.matches("line 0").count()).sum()
}

/// A line exactly as wide as the screen that carries the tag.
fn wide_tag_line(number: usize) -> Line<'static> {
    let tag = tag_label(number);
    let fill = usize::from(COLS) - tag.chars().count();
    Line::from(format!("{tag}{}", "w".repeat(fill)))
}

/// The bytes contain each needle in this order, each after the end of the one before.
fn contains_in_order(bytes: &[u8], needles: &[&[u8]]) -> bool {
    let mut from = 0;
    for needle in needles {
        let Some(found) = bytes[from..]
            .windows(needle.len())
            .position(|window| window == *needle)
        else {
            return false;
        };
        from += found + needle.len();
    }
    true
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

#[test]
fn committed_lines_reach_scrollback_once() {
    let mut rig = Rig::anchored(Alac::new(COLS, ROWS));
    let live = live_view_sections(2);
    rig.draw(&live);
    let mut next = 1;

    let bytes = rig.commit_lines(Vec::new());
    assert!(bytes.is_empty(), "no lines must write no bytes: {bytes:?}");

    for batch in [1, 7, 50, 242] {
        rig.commit_lines(tagged_from(next, batch));
        next += batch;
        assert_bottom_anchored_state(&rig, next - 1);
    }

    // A row exactly as wide as the screen, last in its batch and then first in the next one.
    let mut lines = tagged_from(next, 1);
    lines.push(wide_tag_line(next + 1));
    rig.commit_lines(lines);
    next += 2;
    assert_bottom_anchored_state(&rig, next - 1);

    let mut lines = vec![wide_tag_line(next)];
    lines.extend(tagged_from(next + 1, 2));
    rig.commit_lines(lines);
    next += 3;
    assert_bottom_anchored_state(&rig, next - 1);

    let all = rig.alac.history_and_screen();
    for wide in [next - 4, next - 3] {
        let row = &all[tag_row(&all, wide)];
        assert_eq!(
            row,
            &format!(
                "{}{}",
                tag_label(wide),
                "w".repeat(usize::from(COLS) - tag_label(wide).chars().count())
            ),
            "a full width row must keep all of its cells"
        );
    }

    assert_write_error_recovers(false);
    assert_write_error_recovers(true);
}

/// Every tag up to `total` sits once, in order, on consecutive rows; the live region is still
/// where the start-up put it and nothing of it leaked into the history.
fn assert_bottom_anchored_state(rig: &Rig, total: usize) {
    let all = rig.alac.history_and_screen();
    assert_tags_once_in_order(&all, total);
    assert_tags_adjacent(&all, total);
    assert_eq!(count_tags(&all), total, "no tag may appear twice");
    let viewport = rig.term.viewport();
    assert_eq!(viewport, Rect::new(0, ROWS - 2, COLS, 2));
    let screen = rig.alac.screen();
    assert_live_only_in_viewport(&screen, viewport);
    let history = &all[..all.len() - usize::from(ROWS)];
    assert!(
        history.iter().all(|row| !row.contains("LIVE#")),
        "no live row may reach the history"
    );
    assert_eq!(screen[22], "LIVE# 1");
    assert_eq!(screen[23], "LIVE# 2");
}

/// A write error leaves the ledger alone, drops the batch, and the next write starts with the
/// recovery bytes; restore still completes once.
fn assert_write_error_recovers(synchronized: bool) {
    let (writer, sink) = FailingWriter::new();
    let mut alac = Alac::new(COLS, ROWS);
    let mut term = InlineTerminal::new(
        writer.clone(),
        (COLS, ROWS),
        2,
        probe(synchronized, false, Some((0, ROWS - 1))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    alac.absorb(&sink);
    let live = live_view_sections(2);
    term.draw_live(&view_of(&live)).expect("first draw");
    alac.absorb(&sink);

    writer.fail_after(30);
    let final_cells = cells(&[true; 5]);
    let mut ledger = CommitLedger::new();
    let ready = ledger.take_ready(&final_cells);
    assert_eq!(ready.len(), 5, "all five cells are final");
    let error = term
        .commit(ready, &PlainCellLines)
        .expect_err("the writer is broken, so the commit must fail");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    alac.absorb(&sink);
    assert!(
        ledger.take_ready(&final_cells).is_empty(),
        "a failed write must not rewind the ledger"
    );

    writer.heal();
    term.commit_lines(tagged_from(6, 1))
        .expect("the healed writer commits again");
    let recovery = sink.take();
    let wanted: &[u8] = if synchronized {
        b"\x1b[?2026l\x1b[r"
    } else {
        b"\x1b[r"
    };
    assert!(
        recovery.starts_with(wanted),
        "the first write after an error must start with {wanted:?}, got {:?}",
        &recovery[..recovery.len().min(24)]
    );
    alac.feed(&recovery);

    let all = alac.history_and_screen();
    assert_eq!(
        all.iter().filter(|row| row.contains(&tag_label(6))).count(),
        1,
        "the line committed after the error appears once: {all:?}"
    );
    for number in 1..=5 {
        let wanted = format!("cell {number}");
        let seen = all.iter().filter(|row| row.contains(&wanted)).count();
        assert!(
            seen <= 1,
            "a line of the failed batch must never appear twice (`{wanted}` seen {seen} times)"
        );
    }

    term.restore().expect("restore");
    let restore = sink.take();
    assert!(
        contains_in_order(
            &restore,
            &[
                b"\x1b[r",
                b"\x1b[J",
                b"\x1b[?2004l",
                b"\x1b[0m",
                SHOW_CURSOR
            ]
        ),
        "restore must write its whole sequence after a write error: {restore:?}"
    );
    assert!(restore.ends_with(SHOW_CURSOR));
    term.restore().expect("second restore");
    assert!(
        sink.take().is_empty(),
        "a second restore must write nothing"
    );
}

#[test]
fn first_line_kept_when_viewport_starts_at_row_zero() {
    // One line leaves the slid viewport on row 1, two put it on row 2 where the region starts;
    // five is the usual case. The first line must be on row 0 each time.
    for count in [1usize, 2, 5] {
        let mut rig = Rig::new(
            Alac::new(COLS, ROWS),
            (COLS, ROWS),
            2,
            probe(false, false, Some((0, 0))),
        );
        assert_eq!(rig.term.viewport(), Rect::new(0, 0, COLS, 2));
        let live = live_view_sections(2);
        rig.draw(&live);

        rig.commit_lines(tagged(count));

        let all = rig.alac.history_and_screen();
        assert_tags_once_in_order(&all, count);
        assert_eq!(count_tags(&all), count);
        assert_eq!(rig.term.viewport(), Rect::new(0, count as u16, COLS, 2));
        let mut filled: Vec<(usize, String)> = (1..=count).map(|n| (n - 1, tag_label(n))).collect();
        filled.push((count, "LIVE# 1".to_string()));
        filled.push((count + 1, "LIVE# 2".to_string()));
        assert_eq!(
            rig.alac.screen(),
            screen_with(usize::from(ROWS), &filled),
            "commit of {count}: line 0001 sits on the very first row and the live region moved \
             down with the slide"
        );
    }
}

#[test]
fn viewport_slides_down_while_room_below() {
    let mut rig = Rig::new(
        Alac::with_shell_rows(COLS, ROWS, (0, 2)),
        (COLS, ROWS),
        2,
        probe(false, false, Some((0, 2))),
    );
    assert_eq!(rig.term.viewport(), Rect::new(0, 2, COLS, 2));
    let live = live_view_sections(2);
    rig.draw(&live);
    assert_eq!(rig.alac.screen()[2], "LIVE# 1");

    rig.commit_lines(tagged(5));

    // No redraw: the slide itself must have carried the live rows down.
    assert_eq!(rig.term.viewport(), Rect::new(0, 7, COLS, 2));
    let mut filled: Vec<(usize, String)> = (1..=5).map(|n| (n + 1, tag_label(n))).collect();
    filled.push((7, "LIVE# 1".to_string()));
    filled.push((8, "LIVE# 2".to_string()));
    assert_eq!(
        rig.alac.screen(),
        screen_with(usize::from(ROWS), &filled),
        "tags at rows 2 to 6, live rows at 7 and 8, everything else blank"
    );
}

#[test]
fn commit_at_live_height_cap_keeps_every_line() {
    let mut rig = Rig::new(
        Alac::new(COLS, ROWS),
        (COLS, ROWS),
        22,
        probe(false, false, Some((0, ROWS - 1))),
    );
    assert_eq!(rig.term.viewport(), Rect::new(0, 2, COLS, 22));
    let live = live_view_sections(22);
    rig.draw(&live);

    rig.commit_lines(tagged_from(1, 1));
    rig.commit_lines(tagged_from(2, 5));

    let all = rig.alac.history_and_screen();
    assert_tags_once_in_order(&all, 6);
    assert_eq!(count_tags(&all), 6);
    let screen = rig.alac.screen();
    assert_eq!(screen[0], tag_label(5));
    assert_eq!(screen[1], tag_label(6));
    for (index, row) in screen[2..].iter().enumerate() {
        assert_eq!(
            row,
            &format!("LIVE# {}", index + 1),
            "live row {index} must be untouched"
        );
    }
    assert_eq!(rig.term.viewport(), Rect::new(0, 2, COLS, 22));
}

/// Every shell line, then every tag, appears once and in order; no live row sits anywhere
/// outside the viewport, history included.
fn assert_history_exact(rig: &Rig, tags: usize, size: (u16, u16)) {
    let all = rig.alac.history_and_screen();
    let shell: Vec<String> = (1..=100).map(|n| format!("shell {n:03}")).collect();
    let mut previous = None;
    for label in &shell {
        let found: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|(_, row)| row.as_str() == label)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(found.len(), 1, "`{label}` should appear once: {found:?}");
        if let Some(before) = previous {
            assert!(found[0] > before, "`{label}` is out of order");
        }
        previous = Some(found[0]);
    }
    assert_tags_once_in_order(&all, tags);
    assert_eq!(count_tags(&all), tags, "no tag may appear twice");
    assert!(
        previous.expect("shell lines") < tag_row(&all, 1),
        "the tags come after the shell lines"
    );

    let (cols, rows) = size;
    let viewport = rig.term.viewport();
    assert_eq!(viewport, Rect::new(0, rows - 2, cols, 2));
    let screen_start = all.len() - usize::from(rows);
    for (index, row) in all.iter().enumerate() {
        let in_viewport = index >= screen_start + usize::from(viewport.y)
            && index < screen_start + usize::from(viewport.y + viewport.height);
        if row.contains("LIVE#") {
            assert!(in_viewport, "stray live row {index}: {row:?}");
        }
    }
    let screen = rig.alac.screen();
    assert_eq!(screen[usize::from(viewport.y)], "LIVE# 1");
    assert_eq!(screen[usize::from(viewport.y) + 1], "LIVE# 2");
}

#[test]
fn shrink_and_larger_grow_from_fresh_screen_clears_old_live_tail() {
    let mut rig = Rig::new(
        Alac::with_shell_rows(100, 30, (0, 1)),
        (100, 30),
        13,
        probe(false, false, Some((0, 1))),
    );
    rig.commit_lines(tagged(15));
    assert_eq!(rig.term.viewport().y, 16);
    let live = live_view_sections(13);
    rig.draw(&live);

    for (cols, rows) in [(80, 20), (100, 40)] {
        rig.resize(cols, rows);
        rig.draw(&live);
        let viewport = rig.term.viewport();
        let screen = rig.alac.screen();
        assert_live_only_in_viewport(&screen, viewport);
        assert_tags_once_in_order(&rig.alac.history_and_screen(), 15);
        for row in 0..viewport.height {
            assert_eq!(
                screen[usize::from(viewport.y + row)],
                format!("LIVE# {}", row + 1)
            );
        }
    }
    assert_eq!(rig.term.viewport().y, 16);
}

#[test]
fn narrowing_live_rows_tracks_wraps_separately_from_history() {
    // Alacritty and tmux pull history back differently after width reflow. Check the writer's
    // geometry here rather than treating the emulator as an oracle for tmux regrowth.
    for (cols, overlay, expected_top) in [
        (80, false, 7),
        (100, false, 10),
        (120, false, 10),
        (80, true, 10),
    ] {
        let sink = ByteSink::default();
        let mut term = InlineTerminal::new(
            sink.clone(),
            (100, 30),
            10,
            probe(false, false, Some((0, 1))),
            RestoreLink::detached(),
        )
        .expect("new terminal");
        term.commit_lines(tagged(15)).expect("commit history");
        let mut live = live_view_sections(6);
        for n in 1..=3 {
            let prefix = format!("LIVE# border {n} ");
            live.push(PlainTextSection::new(&format!(
                "{prefix}{}",
                "x".repeat(100 - prefix.len())
            )));
        }
        live.push(PlainTextSection::new("status"));
        term.draw_live(&view_of(&live)).expect("draw wide rows");
        assert_eq!(term.viewport(), Rect::new(0, 16, 100, 10));
        if overlay {
            term.open_overlay().expect("open overlay");
        }
        sink.take();
        term.resize(cols, 20).expect("shrink");
        assert_eq!(term.viewport(), Rect::new(0, expected_top, cols, 10));
        if !overlay {
            let written = String::from_utf8(sink.take()).expect("ASCII cursor moves");
            assert!(written.contains(&format!("\x1b[{};1H", expected_top + 1)));
            let mut vt = vt100::Parser::new(20, cols, 0);
            vt.process(written.as_bytes());
            assert_eq!(vt.screen().cursor_position(), (expected_top + 9, 0));
        }
        term.resize(100, 40).expect("grow");
        assert_eq!(
            term.viewport().y,
            expected_top + 6,
            "only the six rows pushed by the height change are pulled back"
        );
    }
}

#[test]
fn resize_neither_duplicates_nor_loses_lines() {
    let mut rig = Rig::anchored(Alac::with_shell_history(COLS, ROWS));
    let live = live_view_sections(2);
    rig.draw(&live);
    let mut next = 1;
    for batch in [30, 30, 40] {
        rig.commit_lines(tagged_from(next, batch));
        next += batch;
    }
    assert_history_exact(&rig, next - 1, (COLS, ROWS));

    let steps = [
        (80, 12),
        (80, 40),
        (60, 40),
        (80, 40),
        (60, 12),
        (60, 40),
        (80, 24),
    ];
    for (cols, rows) in steps {
        rig.resize(cols, rows);
        rig.draw(&live);
        assert_history_exact(&rig, next - 1, (cols, rows));

        rig.commit_lines(tagged_from(next, 3));
        next += 3;
        assert_history_exact(&rig, next - 1, (cols, rows));
    }
}

/// What a generated sequence can do. Heights are requested live heights; resizes name the new
/// row count (the width stays 80).
#[derive(Clone, Debug)]
enum Op {
    Commit(usize),
    SetLiveHeight(u16),
    Resize(u16),
    OpenOverlay,
    CloseOverlay,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0usize..20).prop_map(Op::Commit),
        2 => (1u16..=12).prop_map(Op::SetLiveHeight),
        3 => (5u16..=60).prop_map(Op::Resize),
        1 => Just(Op::OpenOverlay),
        1 => Just(Op::CloseOverlay),
    ]
}

/// What the test believes about the terminal it drives.
struct Model {
    /// Requested live height; the view holds this many one row sections.
    view_height: u16,
    rows: u16,
    /// Height of the last drawn live region.
    drawn_height: u16,
    overlay_open: bool,
    /// Tags handed to the terminal so far, including those queued behind an overlay.
    committed: usize,
    /// Tags handed over while the overlay was open.
    queued: usize,
}

fn live_cap(rows: u16) -> u16 {
    rows.saturating_sub(2)
}

impl Model {
    fn visible(&self) -> usize {
        self.committed - self.queued
    }

    fn redrawn(&mut self) {
        self.drawn_height = self.view_height.min(live_cap(self.rows));
    }
}

/// The checks that must hold whenever the main screen is showing.
fn assert_main_screen_exact(rig: &Rig, model: &Model, shell_history: bool, step: &str) {
    let all = rig.alac.history_and_screen();
    assert_tags_once_in_order(&all, model.visible());
    assert_eq!(
        count_tags(&all),
        model.visible(),
        "{step}: a tag is duplicated or invented"
    );
    if shell_history {
        let mut previous = None;
        for n in 1..=100 {
            let label = format!("shell {n:03}");
            let found: Vec<usize> = all
                .iter()
                .enumerate()
                .filter(|(_, row)| row.as_str() == label)
                .map(|(index, _)| index)
                .collect();
            assert_eq!(found.len(), 1, "{step}: `{label}` should appear once");
            if let Some(before) = previous {
                assert!(found[0] > before, "{step}: `{label}` is out of order");
            }
            previous = Some(found[0]);
        }
    }

    let viewport = rig.term.viewport();
    assert_eq!(
        viewport.height, model.drawn_height,
        "{step}: viewport height"
    );
    assert_eq!(viewport.width, COLS, "{step}: viewport width");
    assert!(
        viewport.y + viewport.height <= model.rows,
        "{step}: viewport {viewport:?} leaves the {} row screen",
        model.rows
    );
    let screen = rig.alac.screen();
    assert_eq!(screen.len(), usize::from(model.rows));
    let first_visible = model.view_height - viewport.height + 1;
    for index in 0..viewport.height {
        assert_eq!(
            screen[usize::from(viewport.y + index)],
            format!("LIVE# {}", first_visible + index),
            "{step}: live row {index}"
        );
    }

    // With enough scrollback behind it a terminal returns every row the model assumes, so no
    // stale live row may survive anywhere. With none, a grow returns fewer rows than assumed
    // and stale live rows above the viewport are a documented limit; tags still must not break.
    if shell_history {
        let screen_start = all.len() - usize::from(model.rows);
        for (index, row) in all.iter().enumerate() {
            let inside = index >= screen_start + usize::from(viewport.y)
                && index < screen_start + usize::from(viewport.y + viewport.height);
            assert!(
                inside || !row.contains("LIVE#"),
                "{step}: stray live row {index}: {row:?}"
            );
        }
    }
}

fn run_sequence(shell_history: bool, ops: &[Op]) {
    let alac = if shell_history {
        Alac::with_shell_history(COLS, ROWS)
    } else {
        Alac::new(COLS, ROWS)
    };
    let mut rig = Rig::anchored(alac);
    let mut model = Model {
        view_height: 2,
        rows: ROWS,
        drawn_height: 2,
        overlay_open: false,
        committed: 0,
        queued: 0,
    };
    let mut sections = live_view_sections(model.view_height);
    rig.draw(&sections);
    assert_main_screen_exact(&rig, &model, shell_history, "start");

    for (index, op) in ops.iter().enumerate() {
        let step = format!("step {index} {op:?} (shell history: {shell_history})");
        match *op {
            Op::Commit(n) => {
                let bytes = rig.commit_lines(tagged_from(model.committed + 1, n));
                model.committed += n;
                if model.overlay_open {
                    model.queued += n;
                    assert!(
                        bytes.is_empty(),
                        "{step}: an open overlay queues, not writes"
                    );
                } else if n == 0 {
                    assert!(bytes.is_empty(), "{step}: no lines, no bytes");
                }
            }
            Op::SetLiveHeight(height) => {
                model.view_height = height;
                sections = live_view_sections(height);
                let stats = rig.draw(&sections);
                if model.overlay_open {
                    assert_eq!(
                        stats,
                        FrameStats::default(),
                        "{step}: overlay hides the live region"
                    );
                } else {
                    model.redrawn();
                }
            }
            Op::Resize(rows) => {
                // A shrink below the last drawn height pushes live rows into scrollback before
                // the terminal can be told. While an overlay is open the terminal also cannot
                // re-park its cursor, so a shrink that caps the live height further leaves
                // later resizes working from a cursor row that was never written. Both are
                // documented limits, not cases to check.
                if rows < model.drawn_height
                    || (model.overlay_open && live_cap(rows) < model.drawn_height)
                {
                    continue;
                }
                let bytes = rig.resize(COLS, rows);
                if rows == model.rows {
                    assert!(
                        bytes.is_empty(),
                        "{step}: a same size resize writes nothing"
                    );
                }
                model.rows = rows;
                if model.overlay_open {
                    rig.draw_overlay();
                } else {
                    rig.draw(&sections);
                    model.redrawn();
                }
            }
            Op::OpenOverlay => {
                if model.overlay_open {
                    continue;
                }
                rig.open_overlay();
                rig.draw_overlay();
                model.overlay_open = true;
            }
            Op::CloseOverlay => {
                if !model.overlay_open {
                    continue;
                }
                rig.close_overlay();
                model.overlay_open = false;
                model.queued = 0;
                rig.draw(&sections);
                model.redrawn();
            }
        }
        if !model.overlay_open {
            assert_main_screen_exact(&rig, &model, shell_history, &step);
        }
    }

    if model.overlay_open {
        rig.close_overlay();
        model.overlay_open = false;
        model.queued = 0;
        rig.draw(&sections);
        model.redrawn();
    }
    assert_main_screen_exact(&rig, &model, shell_history, "end");
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_commit_resize_overlay_sequence_is_exact(
        ops in proptest::collection::vec(op_strategy(), 1..25)
    ) {
        run_sequence(true, &ops);
        run_sequence(false, &ops);
    }
}

#[test]
fn hostile_text_cannot_clear_or_move_the_screen() {
    let mut rig = Rig::anchored(Alac::new(COLS, ROWS));
    let live = live_view_sections(2);
    rig.draw(&live);
    let mut written = Vec::new();

    let hostile = Line::from("HOSTILE \u{1b}[2J\u{1b}[H\u{7}\u{202e} END");
    let mut batch = tagged_from(1, 3);
    batch.push(hostile);
    batch.extend(tagged_from(4, 2));
    written.extend(rig.commit_lines(batch));

    // Several spans, each sanitized on its own: a C1 control and a delete character.
    let spans = Line::from(vec![
        Span::raw("SPANS "),
        Span::raw("\u{9b}2J"),
        Span::raw(" \u{7f} END"),
    ]);
    let mut batch = vec![spans];
    batch.extend(tagged_from(6, 3));
    written.extend(rig.commit_lines(batch));

    let all = rig.alac.history_and_screen();
    assert_tags_once_in_order(&all, 8);
    assert_eq!(count_tags(&all), 8);
    let hostile_rows: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|(_, row)| row.as_str() == "HOSTILE ␛[2J␛[H␇‹U+202E› END")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(hostile_rows.len(), 1, "stand-ins expected once: {all:?}");
    assert!(tag_row(&all, 3) < hostile_rows[0] && hostile_rows[0] < tag_row(&all, 4));
    let span_rows: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|(_, row)| row.as_str() == "SPANS ‹U+009B›2J ␡ END")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(span_rows.len(), 1, "stand-ins expected once: {all:?}");
    assert!(tag_row(&all, 5) < span_rows[0] && span_rows[0] < tag_row(&all, 6));

    assert!(
        !contains(&written, b"\x1b[2J"),
        "hostile text must never reach the terminal as a clear"
    );
    assert!(!contains(&written, b"\x07"), "no bell byte");
    assert!(
        !contains(&written, "\u{202e}".as_bytes()),
        "no direction mark"
    );
    assert!(!contains(&written, "\u{9b}".as_bytes()), "no C1 control");

    // Nothing was cleared or moved: the live rows are where they were, untouched.
    let screen = rig.alac.screen();
    assert_eq!(screen[22], "LIVE# 1");
    assert_eq!(screen[23], "LIVE# 2");
    assert_eq!(rig.term.viewport(), Rect::new(0, 22, COLS, 2));

    assert_newline_in_span_stays_on_one_row();
}

/// A newline inside one span shows as a visible stand-in on a single row.
fn assert_newline_in_span_stays_on_one_row() {
    let mut rig = Rig::anchored(Alac::new(COLS, ROWS));
    let mut lines = tagged_from(1, 1);
    lines.push(Line::from(vec![Span::raw("a\nb")]));
    lines.extend(tagged_from(2, 1));
    rig.commit_lines(lines);

    let all = rig.alac.history_and_screen();
    assert_tags_once_in_order(&all, 2);
    let split: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|(_, row)| row.as_str() == "a␊b")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(split.len(), 1, "one row reading `a␊b` expected: {all:?}");
    assert_eq!(
        tag_row(&all, 2) - tag_row(&all, 1),
        2,
        "exactly one row sits between the two tags"
    );
    assert!(
        all.iter().all(|row| row != "a" && row != "b"),
        "the span must not be split: {all:?}"
    );
}

#[test]
fn too_small_screen_queues_until_it_grows() {
    let mut rig = Rig::new(
        Alac::new(COLS, 2),
        (COLS, 2),
        2,
        probe(false, false, Some((0, 1))),
    );
    assert_eq!(rig.term.viewport().height, 0, "no viewport on two rows");
    let live = live_view_sections(2);

    let bytes = rig.commit_lines(tagged(3));
    assert!(bytes.is_empty(), "a too small screen queues: {bytes:?}");
    let stats = rig.draw(&live);
    assert_eq!(
        stats,
        FrameStats {
            drew: false,
            bytes: 0
        }
    );
    assert!(
        rig.alac
            .history_and_screen()
            .iter()
            .all(|row| !row.contains("line ")),
        "nothing is written before the screen grows"
    );

    rig.resize(COLS, 10);
    rig.draw(&live);

    let all = rig.alac.history_and_screen();
    assert_tags_once_in_order(&all, 3);
    assert_eq!(count_tags(&all), 3);
    assert_eq!(rig.term.viewport(), Rect::new(0, 8, COLS, 2));
    let filled = [
        (5, tag_label(1)),
        (6, tag_label(2)),
        (7, tag_label(3)),
        (8, "LIVE# 1".to_string()),
        (9, "LIVE# 2".to_string()),
    ];
    assert_eq!(
        rig.alac.screen(),
        screen_with(10, &filled),
        "queued lines sit directly above the viewport at the bottom"
    );
}
