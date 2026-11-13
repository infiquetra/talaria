//! Failed and done counters on the status line.
//!
//! The checks are the visible line and the order of the pieces on it. They do not
//! read a segment count or a numeric rank.

use std::io;
use std::sync::{Arc, Mutex};

use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::status::{
    Connection, Segment, StatusInputs, StatusPalette, StatusSection, Tone, plan,
};
use talaria_tui::terminal::{InlineTerminal, ProbeReply, RestoreLink, TerminalCaps};
use unicode_width::UnicodeWidthStr;

const WIDE: &str = " opus-5.5 high │ ~/proj (main) │ ctx 34% │ $0.12 │ ⚑ 2 need you │ ✗ 1 failed │ 3 working │ ✓ 1 done";
const DROP_COST: &str =
    " opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you │ ✗ 1 failed │ 3 working │ ✓ 1 done";
const DROP_DONE: &str =
    " opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you │ ✗ 1 failed │ 3 working";
const DROP_WORKING: &str = " opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you │ ✗ 1 failed";
const DROP_FOLDER: &str = " opus-5.5 high │ ctx 34% │ ⚑ 2 need you │ ✗ 1 failed";
const DROP_MODEL: &str = " ⚑ 2 need you │ ✗ 1 failed";
const NEEDS: &str = " ⚑ 2 need you";

const GUARD_75: &str =
    " opus-5.5 high │ ~/proj (main) │ ctx 34% │ $0.12 │ ⚑ 2 need you │ 3 working";
const GUARD_67: &str = " opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you │ 3 working";
const GUARD_55: &str = " opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you";
const GUARD_39: &str = " opus-5.5 high │ ctx 34% │ ⚑ 2 need you";
const GUARD_29: &str = " opus-5.5 high │ ⚑ 2 need you";
const GUARD_13: &str = " ⚑ 2 need you";

fn path_of(parts: &[&str]) -> String {
    parts.join("/")
}

fn connected() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        ..StatusInputs::default()
    }
}

fn full() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        model: Some("opus-5.5".into()),
        reasoning_effort: Some("high".into()),
        reasoning_effort_wire: Some("high".into()),
        working_dir: Some(path_of(&["", "h", "dev", "proj"])),
        home_dir: Some(path_of(&["", "h", "dev"])),
        branch: Some("main".into()),
        context_percent: Some(34),
        context_estimated: false,
        cost_usd: Some(0.12),
        cost_status: Some("actual".into()),
        needs_you: 2,
        working: 3,
        ..StatusInputs::default()
    }
}

fn fleet_full() -> StatusInputs {
    StatusInputs {
        failed: 1,
        done: 1,
        ..full()
    }
}

fn piece(inputs: &StatusInputs, segment: Segment) -> Option<(String, Tone)> {
    plan(inputs, None, 400)
        .pieces
        .into_iter()
        .find(|(found, _, _)| *found == segment)
        .map(|(_, text, tone)| (text, tone))
}

fn line_for(cols: u16) -> &'static str {
    match cols {
        99..=120 => WIDE,
        91..=98 => DROP_COST,
        80..=90 => DROP_DONE,
        68..=79 => DROP_WORKING,
        52..=67 => DROP_FOLDER,
        42..=51 => " opus-5.5 high │ ⚑ 2 need you │ ✗ 1 failed",
        26..=41 => DROP_MODEL,
        13..=25 => NEEDS,
        12 => " ⚑ 2 need y…",
        11 => " ⚑ 2 need …",
        10 => " ⚑ 2 need…",
        _ => " ⚑ 2 need…",
    }
}

#[derive(Clone, Default)]
struct ByteSink(Arc<Mutex<Vec<u8>>>);

impl ByteSink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().expect("sink lock"))
    }
}

impl io::Write for ByteSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("sink lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn bottom_row(inputs: &StatusInputs, cols: u16, rows: u16) -> String {
    let sink = ByteSink::default();
    let probe = ProbeReply {
        caps: TerminalCaps::default(),
        cursor: Some((0, rows - 1)),
        ..ProbeReply::default()
    };
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        1,
        probe,
        RestoreLink::detached(),
    )
    .expect("inline terminal");
    let palette = StatusPalette::default();
    let status = StatusSection::new(inputs, None, &palette);
    let sections: Vec<&dyn LiveSection> = vec![&status];
    terminal
        .draw_live(&LiveView { sections })
        .expect("draw the status row");
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(&sink.take());
    parser
        .screen()
        .contents()
        .lines()
        .next_back()
        .unwrap_or("")
        .trim_end()
        .to_string()
}

#[test]
fn failed_and_done_segment_text_and_tone() {
    let shown = StatusInputs {
        failed: 1,
        done: 2,
        needs_you: 2,
        working: 3,
        ..connected()
    };
    assert_eq!(
        piece(&shown, Segment::Failed),
        Some(("✗ 1 failed".to_string(), Tone::Error)),
        "missing failed segment"
    );
    assert_eq!(
        piece(&shown, Segment::Done),
        Some(("✓ 2 done".to_string(), Tone::Success)),
        "missing done segment"
    );
    assert_eq!(UnicodeWidthStr::width("✗"), 1);
    assert_eq!(UnicodeWidthStr::width("✓"), 1);

    let hidden = plan(&connected(), None, 200);
    assert!(
        hidden
            .pieces
            .iter()
            .all(|(segment, _, _)| *segment != Segment::Failed && *segment != Segment::Done),
        "zero counts stay hidden"
    );

    let names: Vec<Segment> = plan(&shown, None, 400)
        .pieces
        .iter()
        .map(|(segment, _, _)| *segment)
        .collect();
    let at = |segment: Segment| {
        names
            .iter()
            .position(|found| *found == segment)
            .expect("segment")
    };
    assert!(at(Segment::NeedsYou) < at(Segment::Failed));
    assert!(at(Segment::Failed) < at(Segment::Working));
    assert!(at(Segment::Working) < at(Segment::Done));
}

#[test]
fn priority_ladder_with_fleet_segments() {
    let inputs = fleet_full();
    for cols in (10u16..=120).rev() {
        assert_eq!(
            plan(&inputs, None, cols).text(),
            line_for(cols),
            "line at {cols} columns"
        );
    }
    assert_eq!(bottom_row(&inputs, 80, 24), line_for(80));
}

#[test]
fn zero_counts_draw_nothing_new() {
    let inputs = full();
    assert_eq!(inputs.failed, 0);
    assert_eq!(inputs.done, 0);
    let cases = [
        (200u16, GUARD_75),
        (75, GUARD_75),
        (74, GUARD_67),
        (67, GUARD_67),
        (66, GUARD_55),
        (55, GUARD_55),
        (54, GUARD_39),
        (39, GUARD_39),
        (38, GUARD_29),
        (29, GUARD_29),
        (28, GUARD_13),
        (13, GUARD_13),
        (10, " ⚑ 2 need…"),
        (3, " ⚑…"),
        (2, " …"),
    ];
    for (cols, expected) in cases {
        let line = plan(&inputs, None, cols);
        assert_eq!(line.text(), expected, "guard line at {cols} columns");
        assert!(
            line.pieces
                .iter()
                .all(|(segment, _, _)| *segment != Segment::Failed && *segment != Segment::Done),
            "zero counts add no segment at {cols} columns"
        );
    }
}
