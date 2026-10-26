//! How the live region at the bottom is redrawn: nothing is written when nothing changed, a frame
//! costs bytes in proportion to what changed and never to the history above, synchronized output
//! brackets every write only when the terminal supports it, and a region that shrinks, grows or
//! changes text leaves no stale rows behind.

mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use support::{Alac, ByteSink, Vt, live_view_sections, probe, tagged};
use talaria_tui::live::{FrameStats, LiveSection, LiveView, PlainTextSection};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// Opens a synchronized update.
const SYNC_OPEN: &[u8] = b"\x1b[?2026h";

/// Closes a synchronized update.
const SYNC_CLOSE: &[u8] = b"\x1b[?2026l";

#[test]
fn clearing_a_viewport_at_row_zero_never_erases_the_display_from_home() {
    for operation in ["draw", "resize", "commit", "close overlay", "restore"] {
        let sink = ByteSink::default();
        let mut term = InlineTerminal::new(
            sink.clone(),
            (COLS, ROWS),
            2,
            probe(false, false, Some((0, 0))),
            RestoreLink::detached(),
        )
        .expect("new terminal at row zero");
        let mut vt = vt100::Parser::new(ROWS, COLS, 0);
        let mut alac = Alac::new(COLS, ROWS);
        let setup = sink.take();
        vt.process(&setup);
        alac.feed(&setup);
        let sections = texts(&["LIVE# one", "LIVE# two"]);
        if operation != "draw" {
            term.draw_live(&view_of(&sections)).expect("initial frame");
            let bytes = sink.take();
            vt.process(&bytes);
            alac.feed(&bytes);
        }
        assert_eq!(term.viewport().y, 0);

        let rows = if operation == "resize" { 3 } else { ROWS };
        match operation {
            "draw" => {
                term.draw_live(&view_of(&sections)).expect("draw");
            }
            "resize" => {
                vt.screen_mut().set_size(rows, COLS);
                alac.resize(COLS, rows);
                term.resize(COLS, rows).expect("shrink to three rows");
                assert_eq!(term.viewport().y, 0);
            }
            "commit" => term.commit_lines(tagged(1)).expect("commit at row zero"),
            "close overlay" => {
                term.open_overlay().expect("open overlay");
                let bytes = sink.take();
                vt.process(&bytes);
                alac.feed(&bytes);
                term.close_overlay().expect("close overlay");
            }
            "restore" => term.restore().expect("restore"),
            _ => unreachable!(),
        }
        let bytes = sink.take();
        let mut cleared_first_row = false;
        let mut erased_from_second_row = false;
        for (at, byte) in bytes.iter().enumerate() {
            let (row, _) = vt.screen().cursor_position();
            if bytes[at..].starts_with(b"\x1b[K") && row == 0 {
                cleared_first_row = true;
            }
            if bytes[at..].starts_with(b"\x1b[J") {
                assert!(row > 0, "{operation} erased the display from row zero");
                erased_from_second_row |= row == 1;
            }
            vt.process(std::slice::from_ref(byte));
        }
        assert!(cleared_first_row, "{operation} did not clear the first row");
        assert!(
            erased_from_second_row,
            "{operation} did not erase from row one"
        );
        alac.feed(&bytes);
        let screen: Vec<String> = vt
            .screen()
            .rows(0, COLS)
            .map(|line| line.trim_end().to_owned())
            .collect();
        assert_eq!(alac.screen(), screen, "{operation} emulator disagreement");
        let mut expected = vec![String::new(); usize::from(rows)];
        match operation {
            "draw" => {
                expected[0] = "LIVE# one".into();
                expected[1] = "LIVE# two".into();
            }
            "commit" => {
                expected[0] = "line 0001".into();
                expected[1] = "LIVE# one".into();
                expected[2] = "LIVE# two".into();
            }
            _ => {}
        }
        assert_eq!(screen, expected, "{operation} changed the screen contents");
        let expected_cursor_row = match operation {
            "resize" | "restore" => 0,
            "commit" => 2,
            _ => 1,
        };
        assert_eq!(
            vt.screen().cursor_position(),
            (expected_cursor_row, 0),
            "{operation} left the cursor on the wrong row"
        );
    }
}

/// A terminal under test, the sink it writes into and the bytes its set-up wrote.
struct Rig {
    term: InlineTerminal<ByteSink>,
    sink: ByteSink,
    setup: Vec<u8>,
}

impl Rig {
    /// 80x24, the probe cursor at the bottom row, so the viewport is bottom-anchored.
    fn new(sync: bool, live_height: u16) -> Self {
        let sink = ByteSink::default();
        let term = InlineTerminal::new(
            sink.clone(),
            (COLS, ROWS),
            live_height,
            probe(sync, false, Some((0, ROWS - 1))),
            RestoreLink::detached(),
        )
        .expect("new terminal");
        let setup = sink.take();
        Self { term, sink, setup }
    }

    /// Draw the sections and return what the call reported and the bytes it wrote. The report
    /// must agree with the bytes.
    fn draw(&mut self, sections: &[PlainTextSection]) -> (FrameStats, Vec<u8>) {
        let stats = self.term.draw_live(&view_of(sections)).expect("draw_live");
        let bytes = self.sink.take();
        assert_eq!(
            stats.bytes,
            bytes.len(),
            "the frame must report every byte it wrote"
        );
        assert_eq!(
            stats.drew,
            !bytes.is_empty(),
            "`drew` is true exactly when a byte was written"
        );
        (stats, bytes)
    }

    /// Draw and let the emulator see the bytes.
    fn draw_into(&mut self, vt: &mut Vt, sections: &[PlainTextSection]) -> FrameStats {
        let (stats, bytes) = self.draw(sections);
        vt.feed(&bytes);
        stats
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

/// One section per text.
fn texts(lines: &[&str]) -> Vec<PlainTextSection> {
    lines
        .iter()
        .map(|line| PlainTextSection::new(line))
        .collect()
}

/// The 24 screen rows: `above` blank rows, then the given rows, then blank rows to the bottom.
fn screen_of(above: usize, rows: &[&str]) -> Vec<String> {
    let below = usize::from(ROWS) - above - rows.len();
    std::iter::repeat_n("", above)
        .chain(rows.iter().copied())
        .chain(std::iter::repeat_n("", below))
        .map(String::from)
        .collect()
}

/// Positions of every `needle` in `haystack`.
fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(at, _)| at)
        .collect()
}

/// Printable form of some bytes for failure messages.
fn shown(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace('\x1b', "ESC")
}

#[test]
fn idle_draw_writes_zero_bytes() {
    let mut rig = Rig::new(false, 2);
    let sections = live_view_sections(2);
    let nothing = FrameStats {
        drew: false,
        bytes: 0,
    };

    let (first, _) = rig.draw(&sections);
    assert!(first.drew && first.bytes > 0, "the first frame paints");

    let (second, bytes) = rig.draw(&sections);
    assert_eq!(second, nothing, "the same view again changes nothing");
    assert!(bytes.is_empty(), "wrote {}", shown(&bytes));

    rig.term.resize(COLS, ROWS).expect("same-size resize");
    assert!(
        rig.sink.is_empty(),
        "a same-size resize wrote {}",
        shown(&rig.sink.take())
    );
    let (third, bytes) = rig.draw(&sections);
    assert_eq!(third, nothing, "the draw after a same-size resize is idle");
    assert!(bytes.is_empty(), "wrote {}", shown(&bytes));

    let changed = texts(&["LIVE# 1", "LIVE# changed"]);
    let (fourth, _) = rig.draw(&changed);
    assert!(fourth.drew, "control: a real change does draw");
}

/// Bytes of one single-character change after `history` committed lines, from a fresh terminal.
fn one_character_change_bytes(history: usize) -> usize {
    let mut rig = Rig::new(false, 2);
    rig.term.commit_lines(tagged(history)).expect("commit");
    rig.sink.take();

    let (first, _) = rig.draw(&texts(&["LIVE# status ok", "LIVE# value a"]));
    assert!(first.drew && first.bytes > 0);
    let (changed, _) = rig.draw(&texts(&["LIVE# status ok", "LIVE# value b"]));
    assert!(changed.drew && changed.bytes > 0, "the change must draw");
    changed.bytes
}

#[test]
fn frame_bytes_do_not_grow_with_history() {
    let after_ten = one_character_change_bytes(10);
    let after_a_thousand = one_character_change_bytes(1_000);
    assert!(
        after_ten.abs_diff(after_a_thousand) <= 16,
        "a one-character frame wrote {after_ten} bytes after 10 lines and {after_a_thousand} \
         after 1,000"
    );
}

/// Six rows of 69 columns whose fourth row carries `marker`.
fn six_rows(marker: char) -> Vec<PlainTextSection> {
    (1..=6)
        .map(|number| {
            let mark = if number == 4 { marker } else { 'm' };
            let row = format!("LIVE# {number} {}{mark}{}", "a".repeat(30), "b".repeat(30));
            PlainTextSection::new(&row)
        })
        .collect()
}

#[test]
fn one_character_change_writes_less_than_full_repaint() {
    let mut rig = Rig::new(false, 6);
    assert_eq!(rig.term.viewport(), Rect::new(0, 18, COLS, 6));

    let (first, _) = rig.draw(&six_rows('m'));
    assert!(first.drew && first.bytes > 0);

    let (one, _) = rig.draw(&six_rows('n'));
    assert!(one.drew && one.bytes > 0, "the change must draw");

    rig.term.resize(COLS - 1, ROWS).expect("resize");
    rig.sink.take();
    let (full, _) = rig.draw(&six_rows('n'));
    assert!(full.drew);
    assert!(
        one.bytes * 2 < full.bytes,
        "one changed character wrote {} bytes, a full repaint of six rows wrote {}",
        one.bytes,
        full.bytes
    );
}

/// One write the sequence under test made, with text that must sit inside the update.
struct Step {
    name: &'static str,
    bytes: Vec<u8>,
    inside: Vec<&'static str>,
}

/// Every kind of write, in an order a real session produces them.
fn run_sequence(sync: bool) -> Vec<Step> {
    let mut rig = Rig::new(sync, 2);
    let mut steps = vec![Step {
        name: "set-up",
        bytes: std::mem::take(&mut rig.setup),
        inside: vec![],
    }];

    let first = texts(&["LIVE# one", "LIVE# PPPPP"]);
    let (_, bytes) = rig.draw(&first);
    steps.push(Step {
        name: "first live frame",
        bytes,
        inside: vec!["LIVE#"],
    });

    let (idle, bytes) = rig.draw(&first);
    assert_eq!(idle.bytes, 0);
    steps.push(Step {
        name: "idle live frame",
        bytes,
        inside: vec![],
    });

    let (_, bytes) = rig.draw(&texts(&["LIVE# one", "LIVE# QQQQQ"]));
    steps.push(Step {
        name: "changed live frame",
        bytes,
        inside: vec!["QQQQQ"],
    });

    rig.term.commit_lines(tagged(3)).expect("commit");
    steps.push(Step {
        name: "history batch",
        bytes: rig.sink.take(),
        inside: vec!["line 0001", "line 0002", "line 0003"],
    });

    rig.term.resize(COLS, 20).expect("resize");
    steps.push(Step {
        name: "resize clear",
        bytes: rig.sink.take(),
        inside: vec!["\x1b[J"],
    });

    let after_resize = texts(&["LIVE# one", "LIVE# QQQQQ"]);
    let (_, bytes) = rig.draw(&after_resize);
    steps.push(Step {
        name: "live frame after resize",
        bytes,
        inside: vec!["LIVE#"],
    });

    rig.term.open_overlay().expect("open overlay");
    steps.push(Step {
        name: "overlay open",
        bytes: rig.sink.take(),
        inside: vec![],
    });

    rig.term
        .draw_overlay(|_area: Rect, buf: &mut Buffer| {
            buf.set_string(0, 0, "OVERLAYTEXT", Style::default());
        })
        .expect("draw overlay");
    steps.push(Step {
        name: "overlay frame",
        bytes: rig.sink.take(),
        inside: vec!["OVERLAYTEXT"],
    });

    rig.term.close_overlay().expect("close overlay");
    steps.push(Step {
        name: "overlay close",
        bytes: rig.sink.take(),
        inside: vec![],
    });

    let (_, bytes) = rig.draw(&after_resize);
    steps.push(Step {
        name: "live frame after overlay",
        bytes,
        inside: vec!["LIVE#"],
    });

    steps
}

/// Every occurrence of `needle` has exactly one update open at that point.
fn assert_inside_update(step: &Step, needle: &str) {
    let at = find_all(&step.bytes, needle.as_bytes());
    assert!(
        !at.is_empty(),
        "step `{}` never wrote `{needle}`: {}",
        step.name,
        shown(&step.bytes)
    );
    for position in at {
        let opened = find_all(&step.bytes[..position], SYNC_OPEN).len();
        let closed = find_all(&step.bytes[..position], SYNC_CLOSE).len();
        assert_eq!(
            opened,
            closed + 1,
            "step `{}` wrote `{needle}` outside a synchronized update: {}",
            step.name,
            shown(&step.bytes)
        );
    }
}

/// Opens and closes alternate, starting with an open, and none is left open.
fn assert_paired(name: &str, bytes: &[u8]) {
    let mut marks: Vec<(usize, bool)> = find_all(bytes, SYNC_OPEN)
        .into_iter()
        .map(|at| (at, true))
        .chain(
            find_all(bytes, SYNC_CLOSE)
                .into_iter()
                .map(|at| (at, false)),
        )
        .collect();
    marks.sort_unstable();
    let mut open = false;
    for (_, is_open) in marks {
        assert_ne!(
            open,
            is_open,
            "unpaired update markers in `{name}`: {}",
            shown(bytes)
        );
        open = is_open;
    }
    assert!(
        !open,
        "`{name}` left a synchronized update open: {}",
        shown(bytes)
    );
}

#[test]
fn synchronized_output_only_when_supported() {
    let with_support = run_sequence(true);
    let mut everything = Vec::new();
    for step in &with_support {
        assert_paired(step.name, &step.bytes);
        for needle in &step.inside {
            assert_inside_update(step, needle);
        }
        everything.extend_from_slice(&step.bytes);
    }
    assert_paired("the whole sequence", &everything);
    let idle = with_support
        .iter()
        .find(|step| step.name == "idle live frame")
        .expect("idle step");
    assert!(idle.bytes.is_empty(), "an idle frame writes nothing at all");

    for step in run_sequence(false) {
        assert!(
            find_all(&step.bytes, b"\x1b[?2026").is_empty(),
            "step `{}` wrote synchronized output without support: {}",
            step.name,
            shown(&step.bytes)
        );
    }
}

#[test]
fn shrinking_live_region_leaves_no_stale_rows() {
    let mut rig = Rig::new(false, 8);
    let mut vt = Vt::new(COLS, ROWS);
    vt.feed(&rig.setup);
    assert_eq!(rig.term.viewport(), Rect::new(0, 16, COLS, 8));

    let tall = live_view_sections(8);
    rig.draw_into(&mut vt, &tall);
    assert_eq!(
        vt.rows(),
        screen_of(
            16,
            &[
                "LIVE# 1", "LIVE# 2", "LIVE# 3", "LIVE# 4", "LIVE# 5", "LIVE# 6", "LIVE# 7",
                "LIVE# 8",
            ]
        )
    );

    let short = live_view_sections(3);
    rig.draw_into(&mut vt, &short);
    assert_eq!(rig.term.viewport(), Rect::new(0, 16, COLS, 3));
    assert_eq!(
        vt.rows(),
        screen_of(16, &["LIVE# 1", "LIVE# 2", "LIVE# 3"]),
        "the top stays and the five freed rows are blank"
    );
}

#[test]
fn growing_live_region_leaves_no_stale_rows() {
    let mut rig = Rig::new(false, 2);
    let mut vt = Vt::new(COLS, ROWS);
    vt.feed(&rig.setup);

    rig.draw_into(&mut vt, &texts(&["PREVIOUS-1", "PREVIOUS-2"]));
    assert_eq!(rig.term.viewport(), Rect::new(0, 22, COLS, 2));
    assert_eq!(vt.rows(), screen_of(22, &["PREVIOUS-1", "PREVIOUS-2"]));

    rig.draw_into(&mut vt, &texts(&["\n\nNEW-3\nNEW-4"]));
    assert_eq!(rig.term.viewport(), Rect::new(0, 20, COLS, 4));
    let rows = vt.rows();
    assert_eq!(rows, screen_of(20, &["", "", "NEW-3", "NEW-4"]));
    assert!(
        rows.iter().all(|row| !row.contains("OLD")),
        "old live text survived the growth: {rows:?}"
    );
}

#[test]
fn changed_line_shows_its_new_text() {
    let mut rig = Rig::new(false, 2);
    let mut vt = Vt::new(COLS, ROWS);
    vt.feed(&rig.setup);

    let first = rig.draw_into(&mut vt, &texts(&["LIVE# fixed", "LIVE# alpha"]));
    assert_eq!(vt.rows(), screen_of(22, &["LIVE# fixed", "LIVE# alpha"]));

    let second = rig.draw_into(&mut vt, &texts(&["LIVE# fixed", "LIVE# alphb"]));
    assert!(
        second.bytes < first.bytes,
        "the one-character frame wrote {} bytes, the first frame {}",
        second.bytes,
        first.bytes
    );
    assert_eq!(vt.rows()[23], "LIVE# alphb");

    rig.draw_into(&mut vt, &texts(&["LIVE# fixed", "LIVE# a"]));
    assert_eq!(
        vt.rows(),
        screen_of(22, &["LIVE# fixed", "LIVE# a"]),
        "a shortened line keeps no tail of the old text"
    );
}
