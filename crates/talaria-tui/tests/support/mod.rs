//! Shared harness for the inline renderer tests: byte sinks, two terminal emulators and the
//! tagged-line helpers.
//!
//! Each integration test is its own crate, so a helper used by one binary is dead in the others.

#![allow(dead_code)]

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line as GridLine};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use ratatui::layout::Rect;
use ratatui::text::Line;
use talaria_core::session::{AssistantCell, Cell, CellId, UserCell};
use talaria_tui::live::PlainTextSection;
use talaria_tui::terminal::{ProbeReply, TerminalCaps};

pub mod frames;

/// Scrolling history kept by [`Alac`]. The tests commit well under this many lines.
const ALAC_HISTORY: usize = 20_000;

/// Bytes written so far, shared between clones. The terminal under test holds one clone and the
/// test keeps another to read and drain.
#[derive(Clone, Default)]
pub struct ByteSink(Arc<Mutex<Vec<u8>>>);

impl ByteSink {
    /// Drain the bytes written so far.
    pub fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().expect("sink lock"))
    }

    /// How many bytes are waiting to be taken.
    pub fn len(&self) -> usize {
        self.0.lock().expect("sink lock").len()
    }

    /// Whether nothing is waiting to be taken.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

/// Marks a writer with no byte limit.
const UNLIMITED: usize = usize::MAX;

/// A writer that can be told to stop working. Clones share the budget, so a test can limit or
/// heal the writer the terminal holds.
#[derive(Clone)]
pub struct FailingWriter {
    sink: ByteSink,
    budget: Arc<AtomicUsize>,
}

impl FailingWriter {
    /// A writer with no limit until [`Self::fail_after`], and the sink that sees its bytes.
    pub fn new() -> (FailingWriter, ByteSink) {
        let sink = ByteSink::default();
        let writer = FailingWriter {
            sink: sink.clone(),
            budget: Arc::new(AtomicUsize::new(UNLIMITED)),
        };
        (writer, sink)
    }

    /// Let `n` more bytes through, then fail every write with `BrokenPipe`.
    pub fn fail_after(&self, n: usize) {
        self.budget.store(n, Ordering::SeqCst);
    }

    /// Remove the limit again.
    pub fn heal(&self) {
        self.budget.store(UNLIMITED, Ordering::SeqCst);
    }
}

impl io::Write for FailingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let allowed = loop {
            let budget = self.budget.load(Ordering::SeqCst);
            if budget == UNLIMITED {
                break buf.len();
            }
            if budget == 0 {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            let allowed = budget.min(buf.len());
            if self
                .budget
                .compare_exchange(budget, budget - allowed, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break allowed;
            }
        };
        self.sink.write(&buf[..allowed])
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The `vt100` emulator: no scrollback, screen rows and the alternate screen flag.
pub struct Vt {
    parser: vt100::Parser,
    cols: u16,
}

impl Vt {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, 0),
            cols,
        }
    }

    pub fn feed(&mut self, b: &[u8]) {
        self.parser.process(b);
    }

    /// Take what the sink holds and feed it. Returns the bytes so the test can inspect them.
    pub fn absorb(&mut self, sink: &ByteSink) -> Vec<u8> {
        let bytes = sink.take();
        self.feed(&bytes);
        bytes
    }

    /// Every screen row, trailing blanks trimmed.
    pub fn rows(&self) -> Vec<String> {
        self.parser
            .screen()
            .rows(0, self.cols)
            .map(|row| row.trim_end().to_string())
            .collect()
    }

    /// Whether the alternate screen is showing.
    pub fn alternate(&self) -> bool {
        self.parser.screen().alternate_screen()
    }

    /// Cursor as (column, row), 0-based.
    pub fn cursor(&self) -> (u16, u16) {
        let (row, col) = self.parser.screen().cursor_position();
        (col, row)
    }

    /// Whether the cursor is hidden.
    pub fn cursor_hidden(&self) -> bool {
        self.parser.screen().hide_cursor()
    }
}

/// The `alacritty_terminal` emulator, which keeps real scrollback and reflows on resize.
pub struct Alac {
    term: Term<VoidListener>,
    processor: vte::ansi::Processor,
}

impl Alac {
    /// Empty screen, cursor at (0, 0).
    pub fn new(cols: u16, rows: u16) -> Self {
        let config = Config {
            scrolling_history: ALAC_HISTORY,
            ..Config::default()
        };
        let size = TermSize::new(usize::from(cols), usize::from(rows));
        Self {
            term: Term::new(config, &size, VoidListener),
            processor: vte::ansi::Processor::new(),
        }
    }

    /// 100 lines `shell 001` to `shell 100`, then the cursor on the last row at column 0.
    pub fn with_shell_history(cols: u16, rows: u16) -> Self {
        let mut alac = Self::new(cols, rows);
        for n in 1..=100 {
            alac.feed(format!("shell {n:03}\r\n").as_bytes());
        }
        alac
    }

    /// Three shell rows (`shell 1` to `shell 3`) directly above a prompt, with the cursor left at
    /// `(c, r)`: `$ ` padded with spaces to `c` characters. Requires `r >= 3` to keep all three
    /// shell rows. Above that, the rows are not drawn and the cursor is only moved.
    pub fn with_shell_rows(cols: u16, rows: u16, cursor: (u16, u16)) -> Self {
        let (c, r) = cursor;
        let mut alac = Self::new(cols, rows);
        if r >= 3 {
            for _ in 0..(r - 3) {
                alac.feed(b"\r\n");
            }
            alac.feed(b"shell 1\r\nshell 2\r\nshell 3\r\n");
        } else {
            alac.feed(format!("\x1b[{};1H", r + 1).as_bytes());
        }
        let mut prompt = String::new();
        if c >= 1 {
            prompt.push('$');
        }
        while prompt.chars().count() < usize::from(c) {
            prompt.push(' ');
        }
        alac.feed(prompt.as_bytes());
        alac
    }

    pub fn feed(&mut self, b: &[u8]) {
        self.processor.advance(&mut self.term, b);
    }

    /// Take what the sink holds and feed it. Returns the bytes so the test can inspect them.
    pub fn absorb(&mut self, sink: &ByteSink) -> Vec<u8> {
        let bytes = sink.take();
        self.feed(&bytes);
        bytes
    }

    /// Resize the emulator. It reflows long lines like a real terminal.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term
            .resize(TermSize::new(usize::from(cols), usize::from(rows)));
    }

    /// Every visible row, trailing blanks trimmed.
    pub fn screen(&self) -> Vec<String> {
        let grid = self.term.grid();
        (0..grid.screen_lines() as i32)
            .map(|line| self.row_text(GridLine(line)))
            .collect()
    }

    /// Every history row, oldest first, then every visible row, trailing blanks trimmed.
    pub fn history_and_screen(&self) -> Vec<String> {
        let grid = self.term.grid();
        let history = grid.history_size() as i32;
        let screen = grid.screen_lines() as i32;
        (-history..screen)
            .map(|line| self.row_text(GridLine(line)))
            .collect()
    }

    /// Cursor as (column, row), 0-based.
    pub fn cursor(&self) -> (u16, u16) {
        let point = self.term.grid().cursor.point;
        (point.column.0 as u16, point.line.0 as u16)
    }

    /// Whether the alternate screen is showing. `screen()` then reads that screen.
    pub fn alternate(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    fn row_text(&self, line: GridLine) -> String {
        let grid = self.term.grid();
        let mut text = String::new();
        for column in 0..grid.columns() {
            let cell = &grid[line][Column(column)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            text.push(cell.c);
        }
        text.trim_end().to_string()
    }
}

/// A probe reply with the given answers.
pub fn probe(sync: bool, keyboard: bool, cursor: Option<(u16, u16)>) -> ProbeReply {
    ProbeReply {
        caps: TerminalCaps {
            synchronized_output: sync,
            keyboard_enhancement: keyboard,
        },
        cursor,
        ..Default::default()
    }
}

/// Lines `line 0001` to `line <n>`.
pub fn tagged(n: usize) -> Vec<Line<'static>> {
    tagged_from(1, n)
}

/// `n` lines numbered from `first`, so batches continue the numbering.
pub fn tagged_from(first: usize, n: usize) -> Vec<Line<'static>> {
    (first..first + n)
        .map(|number| Line::from(tag(number)))
        .collect()
}

fn tag(number: usize) -> String {
    format!("line {number:04}")
}

/// Every tag from `line 0001` to `line <n>` appears on exactly one row, and the rows come in
/// order. Other rows are ignored.
pub fn assert_tags_once_in_order(rows: &[String], n: usize) {
    let mut previous: Option<usize> = None;
    for number in 1..=n {
        let wanted = tag(number);
        let found: Vec<usize> = rows
            .iter()
            .enumerate()
            .flat_map(|(index, row)| std::iter::repeat_n(index, row.matches(&wanted).count()))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "`{wanted}` should appear exactly once, found {} times in rows {found:?}",
            found.len()
        );
        let at = found[0];
        if let Some(before) = previous {
            assert!(
                at > before,
                "`{wanted}` at row {at} is not after the previous tag at row {before}"
            );
        }
        previous = Some(at);
    }
}

/// `height` one-row sections reading `LIVE# 1` to `LIVE# <height>`. The test builds the view from
/// references to these.
pub fn live_view_sections(height: u16) -> Vec<PlainTextSection> {
    (1..=height)
        .map(|n| PlainTextSection::new(&format!("LIVE# {n}")))
        .collect()
}

/// No row outside the viewport contains `LIVE#`.
pub fn assert_live_only_in_viewport(screen: &[String], viewport: Rect) {
    let first = usize::from(viewport.y);
    let end = first + usize::from(viewport.height);
    for (index, row) in screen.iter().enumerate() {
        if row.contains("LIVE#") {
            assert!(
                (first..end).contains(&index),
                "row {index} contains `LIVE#` but the viewport covers rows {first} to {end}: {row:?}"
            );
        }
    }
}

/// Transcript cells with distinct ids (1, 2, ...) and the text `cell 1`, `cell 2`, ... A `true`
/// entry is final (a user cell). A `false` entry is not final (an unsealed assistant cell).
pub fn cells(finality: &[bool]) -> Vec<Cell> {
    finality
        .iter()
        .enumerate()
        .map(|(index, &is_final)| {
            let number = index as u64 + 1;
            let id = CellId::new(number);
            let text = format!("cell {number}");
            if is_final {
                Cell::User(UserCell { id, text })
            } else {
                Cell::Assistant(AssistantCell {
                    id,
                    text,
                    sealed: false,
                    interim: false,
                    transformed: false,
                })
            }
        })
        .collect()
}
