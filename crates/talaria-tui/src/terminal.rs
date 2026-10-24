//! The only module that writes escape sequences: start-up probe, inline viewport,
//! scrollback commits, overlays and terminal restore.
//!
//! The scroll-region history insertion technique follows OpenAI Codex's
//! `codex-rs/tui/src/insert_history.rs` (`openai/codex`, Apache-2.0); this module is an
//! independent implementation and copies no Codex source code.
//!
//! Every move after start-up is written relative to the cursor, which stays parked on the
//! viewport's last row. A multiplexer can resize the screen before the program hears of it;
//! the terminal keeps the cursor on its content line, so a relative write made in that window
//! still lands next to the live region, where an absolute one would land on the wrong row.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossterm::queue;
use crossterm::style::{PrintStyledContent, SetStyle, StyledContent};
use ratatui::backend::{Backend, CrosstermBackend, IntoCrossterm};
use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use talaria_core::session::Cell;

use crate::history::{CellLines, sanitize_line, wrap_lines};
use crate::live::{FrameStats, LiveView, max_live_height};

const SYNC_BEGIN: &[u8] = b"\x1b[?2026h";
const SYNC_END: &[u8] = b"\x1b[?2026l";
const PASTE_ON: &[u8] = b"\x1b[?2004h";
const PASTE_OFF: &[u8] = b"\x1b[?2004l";
const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
/// Kitty keyboard protocol: push the disambiguate-escape-codes flag, and pop it.
const FLAGS_PUSH: &[u8] = b"\x1b[>1u";
const FLAGS_POP: &[u8] = b"\x1b[<1u";
const ALTERNATE_ON: &[u8] = b"\x1b[?1049h";
const ALTERNATE_OFF: &[u8] = b"\x1b[?1049l";
const RESET_REGION: &[u8] = b"\x1b[r";
const RESET_STYLE: &[u8] = b"\x1b[0m";
const CLEAR_BELOW: &[u8] = b"\x1b[J";
const CLEAR_LINE: &[u8] = b"\x1b[K";
const CLEAR_SCREEN: &[u8] = b"\x1b[2J";

/// After a probe deadline passes with no end marker, input is drained until it has been quiet
/// this long, for at most [`DRAIN_LIMIT`].
const DRAIN_QUIET: Duration = Duration::from_millis(50);
const DRAIN_LIMIT: Duration = Duration::from_millis(250);

/// How long the key reader waits for an event before it checks its stop flag.
const INPUT_POLL: Duration = Duration::from_millis(25);

/// What the terminal said it can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalCaps {
    pub synchronized_output: bool,
    pub keyboard_enhancement: bool,
}

/// The start-up probe's answers. `cursor` is (column, row), 0-based.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeReply {
    pub caps: TerminalCaps,
    pub cursor: Option<(u16, u16)>,
}

/// The four queries, written once at start-up: cursor position, keyboard flags, the
/// synchronized output mode report, and device attributes, which every terminal answers and so
/// marks the end of the reply.
pub const PROBE_QUERY: &[u8] = b"\x1b[6n\x1b[?u\x1b[?2026$p\x1b[c";

/// One control sequence (`ESC [` …) read from a probe reply.
struct Sequence {
    /// Bytes after `ESC [`, final byte included.
    len: usize,
    private: bool,
    dollar: bool,
    params: Vec<u32>,
    final_byte: u8,
}

enum Scan {
    Complete(Sequence),
    /// The bytes ran out inside the sequence.
    Incomplete,
    /// Not a sequence this parser reads.
    Invalid,
}

/// Read one sequence from the bytes after `ESC [`.
fn scan_sequence(rest: &[u8]) -> Scan {
    let private = rest.first() == Some(&b'?');
    let mut at = usize::from(private);
    let mut params = Vec::new();
    let mut current: Option<u32> = None;
    let mut dollar = false;
    loop {
        let Some(&byte) = rest.get(at) else {
            return Scan::Incomplete;
        };
        match byte {
            b'0'..=b'9' if !dollar => {
                let digit = u32::from(byte - b'0');
                current = Some(
                    current
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(digit),
                );
            }
            b';' if !dollar => params.push(current.take().unwrap_or(0)),
            b'$' if !dollar => dollar = true,
            0x40..=0x7e => {
                params.extend(current);
                return Scan::Complete(Sequence {
                    len: at + 1,
                    private,
                    dollar,
                    params,
                    final_byte: byte,
                });
            }
            _ => return Scan::Invalid,
        }
        at += 1;
    }
}

fn clamp_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// Parse a probe reply. The flag is true when the end marker was seen; without it every answer
/// counts as unsupported and the cursor as unknown.
pub fn parse_probe_reply(bytes: &[u8]) -> (ProbeReply, bool) {
    let mut found = ProbeReply::default();
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] != 0x1b || bytes[at + 1] != b'[' {
            at += 1;
            continue;
        }
        let sequence = match scan_sequence(&bytes[at + 2..]) {
            Scan::Complete(sequence) => sequence,
            Scan::Incomplete => break,
            Scan::Invalid => {
                at += 1;
                continue;
            }
        };
        at += 2 + sequence.len;
        match (sequence.private, sequence.dollar, sequence.final_byte) {
            (false, false, b'R') if sequence.params.len() == 2 => {
                let row = clamp_u16(sequence.params[0].saturating_sub(1));
                let column = clamp_u16(sequence.params[1].saturating_sub(1));
                found.cursor = Some((column, row));
            }
            (true, false, b'u') => found.caps.keyboard_enhancement = true,
            (true, true, b'y') if sequence.params.first() == Some(&2026) => {
                found.caps.synchronized_output = matches!(sequence.params.get(1), Some(1..=3));
            }
            (_, false, b'c') => return (found, true),
            _ => {}
        }
    }
    (ProbeReply::default(), false)
}

fn timespec(duration: Duration) -> Timespec {
    Timespec {
        tv_sec: duration.as_secs().try_into().unwrap_or(i64::MAX),
        tv_nsec: duration.subsec_nanos().into(),
    }
}

/// Wait until `fd` can be read or `timeout` passes. True when it can be read (end of file
/// counts).
fn wait_readable(fd: BorrowedFd<'_>, timeout: Duration) -> io::Result<bool> {
    let timeout = timespec(timeout);
    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
    loop {
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Ok(false),
            Ok(_) => return Ok(!fds[0].revents().is_empty()),
            Err(Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

/// One read. `None` when nothing was there after all.
fn read_some(fd: BorrowedFd<'_>, chunk: &mut [u8]) -> io::Result<Option<usize>> {
    loop {
        match rustix::io::read(fd, &mut *chunk) {
            Ok(count) => return Ok(Some(count)),
            Err(Errno::INTR) => continue,
            Err(Errno::AGAIN) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
}

/// The time left until `end`, or `None` once it has passed.
fn left_until(end: Instant) -> Option<Duration> {
    end.checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
}

/// Read a probe reply from `fd` until the marker or `deadline`. Never writes.
///
/// The descriptor is read directly, never through the buffered standard input, so no byte typed
/// after the reply is held back from the key reader. Without a marker by the deadline, input is
/// discarded until it has been quiet for 50 ms (at most 250 ms more), which takes most late
/// replies. End of file stops at once.
pub fn read_probe_reply(fd: impl AsFd, deadline: Duration) -> io::Result<ProbeReply> {
    let fd = fd.as_fd();
    let end = Instant::now() + deadline;
    let mut reply = Vec::new();
    let mut chunk = [0u8; 256];
    while let Some(left) = left_until(end) {
        if !wait_readable(fd, left)? {
            continue;
        }
        match read_some(fd, &mut chunk)? {
            Some(0) => return Ok(ProbeReply::default()),
            Some(count) => reply.extend_from_slice(&chunk[..count]),
            None => continue,
        }
        let (parsed, complete) = parse_probe_reply(&reply);
        if complete {
            return Ok(parsed);
        }
    }

    let drain_end = Instant::now() + DRAIN_LIMIT;
    while let Some(left) = left_until(drain_end) {
        if !wait_readable(fd, left.min(DRAIN_QUIET))? {
            break;
        }
        if read_some(fd, &mut chunk)? == Some(0) {
            break;
        }
    }
    Ok(ProbeReply::default())
}

/// Write the probe query and read the reply.
///
/// Call order, always: [`TerminalGuard::acquire`] (raw mode must already be on, or the replies
/// echo), then this, then [`terminal_size`], [`InlineTerminal::new`] and [`spawn_input`]. Nothing
/// queries the terminal after this.
pub fn probe_startup(deadline: Duration) -> Result<ProbeReply, TerminalError> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err(TerminalError::NotATerminal);
    }
    stdout
        .write_all(PROBE_QUERY)
        .and_then(|()| stdout.flush())
        .map_err(TerminalError::Io)?;
    read_probe_reply(&stdin, deadline).map_err(TerminalError::Io)
}

/// The terminal size as (columns, rows).
pub fn terminal_size() -> io::Result<(u16, u16)> {
    crossterm::terminal::size()
}

#[derive(Debug)]
pub enum TerminalError {
    NotATerminal,
    Io(io::Error),
}

impl fmt::Display for TerminalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotATerminal => f.write_str("standard input or output is not a terminal"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for TerminalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotATerminal => None,
            Self::Io(error) => Some(error),
        }
    }
}

/// The link the panic hook restores through. The hook is installed once per process and reads
/// the link of the most recent guard.
static PANIC_LINK: Mutex<Option<RestoreLink>> = Mutex::new(None);
static PANIC_HOOK: Once = Once::new();

fn install_panic_hook(link: &RestoreLink) {
    *PANIC_LINK.lock().unwrap_or_else(PoisonError::into_inner) = Some(link.clone());
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let link = match PANIC_LINK.try_lock() {
                Ok(slot) => slot.clone(),
                Err(TryLockError::Poisoned(slot)) => slot.into_inner().clone(),
                Err(TryLockError::WouldBlock) => None,
            };
            if let Some(link) = link
                && link.restore_on_panic(&mut io::stdout())
            {
                let _ = crossterm::terminal::disable_raw_mode();
            }
            previous(info);
        }));
    });
}

/// Raw mode and the panic hook, restored on drop.
pub struct TerminalGuard {
    link: RestoreLink,
}

impl TerminalGuard {
    pub fn acquire() -> Result<Self, TerminalError> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(TerminalError::NotATerminal);
        }
        crossterm::terminal::enable_raw_mode().map_err(TerminalError::Io)?;
        let link = RestoreLink::detached();
        install_panic_hook(&link);
        Ok(Self { link })
    }

    pub fn link(&self) -> RestoreLink {
        self.link.clone()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.link.restore_into(&mut io::stdout());
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// What the restore sequence needs to know, kept current by the terminal.
#[derive(Clone, Copy, Debug, Default)]
struct RestoreState {
    /// An inline terminal exists.
    active: bool,
    /// A viewport was placed (false while the screen is too small).
    placed: bool,
    top: u16,
    overlay_open: bool,
    flags_pushed: bool,
    in_sync: bool,
    restored: bool,
}

/// The restore sequence for this state.
fn restore_sequence(state: &RestoreState) -> Vec<u8> {
    let mut bytes = Vec::new();
    if state.in_sync {
        bytes.extend_from_slice(SYNC_END);
    }
    if state.overlay_open {
        bytes.extend_from_slice(ALTERNATE_OFF);
    }
    bytes.extend_from_slice(RESET_REGION);
    if state.placed {
        push_move(&mut bytes, 0, state.top);
        if state.top == 0 {
            bytes.extend_from_slice(CLEAR_LINE);
            push_move(&mut bytes, 0, 1);
        }
        bytes.extend_from_slice(CLEAR_BELOW);
        if state.top == 0 {
            push_move(&mut bytes, 0, 0);
        }
    }
    bytes.extend_from_slice(PASTE_OFF);
    if state.flags_pushed {
        bytes.extend_from_slice(FLAGS_POP);
    }
    bytes.extend_from_slice(RESET_STYLE);
    bytes.extend_from_slice(SHOW_CURSOR);
    bytes
}

/// What the panic hook writes when the state is locked by the panicking code.
fn minimal_restore() -> Vec<u8> {
    [RESET_REGION, PASTE_OFF, RESET_STYLE, SHOW_CURSOR].concat()
}

/// The shared state the restore routine runs from.
#[derive(Clone)]
pub struct RestoreLink {
    state: Arc<Mutex<RestoreState>>,
}

impl RestoreLink {
    pub fn detached() -> Self {
        Self {
            state: Arc::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, RestoreState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Mark the restore as done and return the state to write it from, unless it was already
    /// done or there is nothing to restore.
    fn claim(state: &mut RestoreState) -> Option<RestoreState> {
        if state.restored || !state.active {
            return None;
        }
        state.restored = true;
        Some(*state)
    }

    /// Write the restore sequence once. `Ok(false)` means nothing was written.
    pub fn restore_into(&self, w: &mut dyn io::Write) -> io::Result<bool> {
        let claimed = Self::claim(&mut self.lock());
        let Some(state) = claimed else {
            return Ok(false);
        };
        w.write_all(&restore_sequence(&state))?;
        w.flush()?;
        Ok(true)
    }

    /// The panic hook's restore. True when it wrote something.
    fn restore_on_panic(&self, w: &mut dyn io::Write) -> bool {
        let bytes = match self.state.try_lock() {
            Ok(mut state) => Self::claim(&mut state).map(|state| restore_sequence(&state)),
            Err(TryLockError::Poisoned(state)) => {
                Self::claim(&mut state.into_inner()).map(|state| restore_sequence(&state))
            }
            Err(TryLockError::WouldBlock) => Some(minimal_restore()),
        };
        let Some(bytes) = bytes else {
            return false;
        };
        let _ = w.write_all(&bytes).and_then(|()| w.flush());
        true
    }
}

fn push_move(bytes: &mut Vec<u8>, column: u16, row: u16) {
    bytes.extend_from_slice(
        format!("\x1b[{};{}H", u32::from(row) + 1, u32::from(column) + 1).as_bytes(),
    );
}

/// A state change that takes effect once the bytes before it were written.
#[derive(Clone, Copy)]
enum Mark {
    SyncOn,
    SyncOff,
    OverlayOn,
    OverlayOff,
    Recovered,
}

/// The bytes of one operation, written in one go.
#[derive(Default)]
struct Batch {
    bytes: Vec<u8>,
    marks: Vec<(usize, Mark)>,
}

impl Batch {
    fn put(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn mark(&mut self, mark: Mark) {
        self.marks.push((self.bytes.len(), mark));
    }

    fn move_to(&mut self, column: u16, row: u16) {
        push_move(&mut self.bytes, column, row);
    }

    /// The row's spans with their styles.
    fn styled_row(&mut self, row: &Line<'_>) -> io::Result<()> {
        for span in &row.spans {
            let style = row.style.patch(span.style);
            if style == Style::default() {
                self.put(span.content.as_bytes());
            } else {
                let content = StyledContent::new(style.into_crossterm(), span.content.as_ref());
                queue!(self.bytes, PrintStyledContent(content))?;
            }
        }
        Ok(())
    }
}

/// The style a buffer cell is written with.
fn cell_style(cell: &ratatui::buffer::Cell) -> Style {
    let mut style = Style::default().add_modifier(cell.modifier);
    if cell.fg != Color::Reset {
        style = style.fg(cell.fg);
    }
    if cell.bg != Color::Reset {
        style = style.bg(cell.bg);
    }
    style
}

/// The overlay's previous frame, and whether the next frame must clear the screen first.
struct Overlay {
    previous: Buffer,
    clear: bool,
}

/// A fixed viewport at the bottom of the terminal with native scrollback above it.
///
/// Finished lines are written once into the terminal's own scrollback; only the live region,
/// the viewport, is redrawn. The cursor stays hidden and is parked at column 0 of the
/// viewport's last row after every operation, because the resize model depends on where it is.
pub struct InlineTerminal<W: io::Write> {
    out: W,
    link: RestoreLink,
    caps: TerminalCaps,
    cols: u16,
    rows: u16,
    /// The viewport's first row, 0-based.
    top: u16,
    /// The row the cursor is on. Moves are written relative to it, so a write made before
    /// Talaria hears of a resize still lands next to the live region.
    cursor: u16,
    /// The live region's height: the last drawn one, or the one asked for at start.
    height: u16,
    /// Rows known to be in history, or unknown when startup may have inherited history.
    scrolled: Option<u32>,
    /// Live rows pushed above row zero by a shrink, awaiting recovery on a grow.
    spilled: u16,
    /// False while the screen is too small to hold a viewport.
    placed: bool,
    /// The last live frame drawn.
    previous: Buffer,
    /// The next live frame paints every cell.
    repaint: bool,
    /// The next live frame clears from the viewport top down before it paints.
    clear: bool,
    overlay: Option<Overlay>,
    /// Lines held while an overlay is open or the screen is too small, sanitized, not wrapped.
    queued: Vec<Line<'static>>,
    /// A write failed; the next write starts by undoing whatever it left open.
    dirty: bool,
    /// `restore` ran; nothing more is written.
    closed: bool,
}

impl<W: io::Write> InlineTerminal<W> {
    /// Switch on bracketed paste, hide the cursor, push keyboard flags when the terminal has the
    /// protocol, and place the viewport from the probe's cursor. No mouse mode is ever written.
    pub fn new(
        w: W,
        size: (u16, u16),
        live_height: u16,
        probe: ProbeReply,
        link: RestoreLink,
    ) -> io::Result<Self> {
        let (cols, rows) = size;
        let mut term = Self {
            out: w,
            link,
            caps: probe.caps,
            cols,
            rows,
            top: 0,
            cursor: probe
                .cursor
                .map_or(0, |(_, row)| row.min(rows.saturating_sub(1))),
            height: live_height.max(1),
            scrolled: match probe.cursor {
                Some((_, row)) if row < rows.saturating_sub(1) => Some(0),
                _ => None,
            },
            spilled: 0,
            placed: false,
            previous: Buffer::empty(Rect::new(0, 0, cols, 0)),
            repaint: true,
            clear: true,
            overlay: None,
            queued: Vec::new(),
            dirty: false,
            closed: false,
        };
        let pushed = term.caps.keyboard_enhancement;
        term.link.lock().active = true;
        term.link.lock().flags_pushed = pushed;

        let mut batch = Batch::default();
        batch.put(PASTE_ON);
        batch.put(HIDE_CURSOR);
        if pushed {
            batch.put(FLAGS_PUSH);
        }
        if rows >= 3 {
            term.height = term.height.clamp(1, max_live_height(rows));
            term.top = match probe.cursor {
                Some((column, row)) if column != 0 => {
                    term.newline(&mut batch);
                    row.saturating_add(1).min(rows - 1)
                }
                Some((_, row)) => row.min(rows - 1),
                None => {
                    term.jump(&mut batch, 0, rows - 1);
                    rows - 1
                }
            };
            term.placed = true;
            // Start-up knows the geometry exactly: place by absolute address.
            if term.top + term.height > rows {
                term.jump(&mut batch, 0, rows - 1);
                term.make_room(&mut batch);
            }
            term.jump(&mut batch, 0, term.top + term.height - 1);
        }
        term.send(batch)?;
        Ok(term)
    }

    /// Render the cells and commit their lines.
    pub fn commit(&mut self, cells: &[Cell], render: &dyn CellLines) -> io::Result<()> {
        if cells.is_empty() {
            return Ok(());
        }
        let lines = cells
            .iter()
            .flat_map(|cell| render.lines(cell, self.cols))
            .collect();
        self.commit_lines(lines)
    }

    /// Write the lines into the scrollback above the viewport, once. Every span is sanitized
    /// and never split on a newline. While an overlay is open or the screen is too small, the
    /// lines wait and are written when that ends.
    pub fn commit_lines(&mut self, lines: Vec<Line<'static>>) -> io::Result<()> {
        if self.closed || lines.is_empty() {
            return Ok(());
        }
        let lines: Vec<Line<'static>> = lines.into_iter().map(sanitize_line).collect();
        if self.overlay.is_some() || !self.placed {
            self.queued.extend(lines);
            return Ok(());
        }
        let rows = wrap_lines(lines, self.cols);
        let mut batch = self.begin();
        self.sync_on(&mut batch);
        self.insert_rows(&mut batch, &rows)?;
        self.park(&mut batch);
        self.sync_off(&mut batch);
        self.send(batch).map(drop)
    }

    /// Redraw the live region, writing only the cells that changed. Nothing changed, nothing
    /// written.
    pub fn draw_live(&mut self, view: &LiveView<'_>) -> io::Result<FrameStats> {
        if self.closed || self.overlay.is_some() || !self.placed {
            return Ok(FrameStats::default());
        }
        let rects = view.layout(self.cols, max_live_height(self.rows));
        let wanted = rects
            .iter()
            .fold(0u16, |sum, rect| sum.saturating_add(rect.height))
            .max(1);
        let area = Rect::new(0, 0, self.cols, wanted);
        let mut frame = Buffer::empty(area);
        for (section, rect) in view.sections.iter().zip(&rects) {
            if rect.height > 0 {
                section.render(*rect, &mut frame);
            }
        }
        let resized = wanted != self.height || self.previous.area != area;
        if !resized && !self.repaint && frame == self.previous {
            return Ok(FrameStats::default());
        }

        let mut batch = self.begin();
        self.sync_on(&mut batch);
        if resized {
            let growing = wanted > self.height;
            self.height = wanted;
            if growing {
                self.make_room(&mut batch);
            }
            self.repaint = true;
            self.clear = true;
        }
        if self.clear {
            self.clear_from_top_relative(&mut batch);
        }
        let blank = Buffer::empty(area);
        let previous = std::mem::replace(&mut self.previous, blank);
        let base = if self.repaint {
            &self.previous
        } else {
            &previous
        };
        let changed: Vec<(u16, u16, ratatui::buffer::Cell)> = base
            .diff(&frame)
            .into_iter()
            .map(|(x, y, cell)| (x, y, cell.clone()))
            .collect();
        self.write_cells(
            &mut batch,
            changed.iter().map(|(x, y, cell)| (*x, *y, cell)),
        )?;
        self.park(&mut batch);
        self.sync_off(&mut batch);
        self.previous = frame;
        self.repaint = false;
        self.clear = false;
        let bytes = self.send(batch)?;
        Ok(FrameStats {
            drew: bytes > 0,
            bytes,
        })
    }

    /// Follow a size change without asking the terminal where the cursor went.
    ///
    /// The model: a shrink drops rows below the cursor first, then pushes rows from the top into
    /// scrollback; a grow pulls rows back from scrollback. The cursor is parked on the
    /// viewport's last row, so the viewport moves up by the rows pushed out, or down by the rows
    /// pulled back. Clears from the new top down; the caller's next `draw_live` paints. Rows
    /// above the top are history and are never cleared.
    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        if self.closed || (cols, rows) == (self.cols, self.rows) {
            return Ok(());
        }
        if !self.placed {
            self.cols = cols;
            self.rows = rows;
            self.forget_frames();
            if rows >= 3 && self.overlay.is_none() {
                let mut batch = self.begin();
                self.sync_on(&mut batch);
                self.place_at_bottom(&mut batch);
                self.write_queue(&mut batch)?;
                self.park(&mut batch);
                self.sync_off(&mut batch);
                return self.send(batch).map(drop);
            }
            return Ok(());
        }

        let below = self.rows - (self.top + self.height);
        let wrapped = self.wrapped_extra(cols);
        if rows < self.rows {
            let height_pushed = (self.rows - rows).saturating_sub(below);
            let pushed = height_pushed.saturating_add(wrapped);
            if let Some(scrolled) = &mut self.scrolled {
                *scrolled = scrolled.saturating_add(u32::from(height_pushed));
            }
            self.spilled = self.spilled.saturating_add(pushed.saturating_sub(self.top));
            self.top -= self.top.min(pushed);
            self.cursor -= self.cursor.min(height_pushed);
        } else {
            let grown = rows - self.rows;
            let pulled = match &mut self.scrolled {
                Some(scrolled) => {
                    let pulled = grown.min(u16::try_from(*scrolled).unwrap_or(u16::MAX));
                    *scrolled -= u32::from(pulled);
                    pulled
                }
                None => grown,
            };
            let recovered = self.spilled.min(pulled);
            self.spilled -= recovered;
            self.top += pulled - recovered;
            self.cursor += pulled;
        }
        self.cursor = self.cursor.min(rows.saturating_sub(1));
        self.cols = cols;
        self.rows = rows;
        self.forget_frames();
        if rows < 3 {
            self.placed = false;
            self.update_link();
            return Ok(());
        }
        self.height = self.height.clamp(1, max_live_height(rows));
        if self.top + self.height > rows {
            self.top = rows - self.height;
        }
        self.repaint = true;
        self.clear = true;
        if self.overlay.is_some() {
            self.update_link();
            return Ok(());
        }

        // The new size is known now: address the top absolutely, so the cursor row is exact again.
        let mut batch = self.begin();
        self.sync_on(&mut batch);
        self.clear_from_top_absolute(&mut batch);
        self.park(&mut batch);
        self.sync_off(&mut batch);
        self.clear = false;
        self.send(batch).map(drop)
    }

    /// Enter and clear the alternate screen. Until it closes, commits wait and the live region
    /// is not drawn.
    pub fn open_overlay(&mut self) -> io::Result<()> {
        if self.closed || self.overlay.is_some() {
            return Ok(());
        }
        let mut batch = self.begin();
        self.sync_on(&mut batch);
        batch.put(ALTERNATE_ON);
        batch.mark(Mark::OverlayOn);
        batch.put(CLEAR_SCREEN);
        self.sync_off(&mut batch);
        self.overlay = Some(Overlay {
            previous: Buffer::empty(self.screen_area()),
            clear: false,
        });
        self.send(batch).map(drop)
    }

    /// Render a full-screen frame and write the cells that changed.
    pub fn draw_overlay(
        &mut self,
        render: impl FnOnce(Rect, &mut Buffer),
    ) -> io::Result<FrameStats> {
        if self.closed {
            return Ok(FrameStats::default());
        }
        let Some(mut overlay) = self.overlay.take() else {
            return Ok(FrameStats::default());
        };
        let area = self.screen_area();
        let mut frame = Buffer::empty(area);
        render(area, &mut frame);
        if overlay.previous.area != area {
            overlay.previous = Buffer::empty(area);
            overlay.clear = true;
        }
        if !overlay.clear && frame == overlay.previous {
            self.overlay = Some(overlay);
            return Ok(FrameStats::default());
        }

        let mut batch = self.begin();
        self.sync_on(&mut batch);
        if overlay.clear {
            batch.put(CLEAR_SCREEN);
        }
        let blank;
        let base = if overlay.clear {
            blank = Buffer::empty(area);
            &blank
        } else {
            &overlay.previous
        };
        let drawn = CrosstermBackend::new(&mut batch.bytes).draw(base.diff(&frame).into_iter());
        self.sync_off(&mut batch);
        overlay.previous = frame;
        overlay.clear = false;
        self.overlay = Some(overlay);
        drawn?;
        let bytes = self.send(batch)?;
        Ok(FrameStats {
            drew: bytes > 0,
            bytes,
        })
    }

    /// Leave the alternate screen, clear the live region, write the lines that waited, and
    /// leave the live region for the caller's next `draw_live` to paint.
    pub fn close_overlay(&mut self) -> io::Result<()> {
        if self.closed || self.overlay.take().is_none() {
            return Ok(());
        }
        let mut batch = self.begin();
        self.sync_on(&mut batch);
        batch.put(ALTERNATE_OFF);
        batch.mark(Mark::OverlayOff);
        if self.placed {
            self.clear_from_top_absolute(&mut batch);
            self.write_queue(&mut batch)?;
        } else if self.rows >= 3 {
            self.place_at_bottom(&mut batch);
            self.write_queue(&mut batch)?;
        }
        self.park(&mut batch);
        self.sync_off(&mut batch);
        self.forget_frames();
        self.repaint = true;
        self.clear = false;
        self.send(batch).map(drop)
    }

    /// Undo every mode this terminal set, once. Later calls, and any call after the link has
    /// restored, write nothing.
    pub fn restore(&mut self) -> io::Result<()> {
        self.closed = true;
        let claimed = RestoreLink::claim(&mut self.link.lock());
        let Some(state) = claimed else {
            return Ok(());
        };
        self.out.write_all(&restore_sequence(&state))?;
        self.out.flush()
    }

    /// Height 0 while the screen is too small.
    pub fn viewport(&self) -> Rect {
        if self.placed {
            Rect::new(0, self.top, self.cols, self.height)
        } else {
            Rect::new(0, 0, self.cols, 0)
        }
    }

    fn screen_area(&self) -> Rect {
        Rect::new(0, 0, self.cols, self.rows)
    }

    /// After a size change no earlier frame can be diffed against.
    fn forget_frames(&mut self) {
        self.previous = Buffer::empty(Rect::new(0, 0, self.cols, 0));
        let screen = self.screen_area();
        if let Some(overlay) = &mut self.overlay {
            overlay.previous = Buffer::empty(screen);
            overlay.clear = true;
        }
    }

    /// A new batch, starting with the recovery bytes when the last write failed.
    fn begin(&self) -> Batch {
        let mut batch = Batch::default();
        if self.dirty {
            if self.link.lock().in_sync {
                batch.put(SYNC_END);
                batch.mark(Mark::SyncOff);
            }
            batch.put(RESET_REGION);
            batch.put(RESET_STYLE);
            batch.move_to(0, self.cursor);
            batch.mark(Mark::Recovered);
        }
        batch
    }

    fn sync_on(&self, batch: &mut Batch) {
        if self.caps.synchronized_output {
            batch.put(SYNC_BEGIN);
            batch.mark(Mark::SyncOn);
        }
    }

    fn sync_off(&self, batch: &mut Batch) {
        if self.caps.synchronized_output {
            batch.put(SYNC_END);
            batch.mark(Mark::SyncOff);
        }
    }

    /// Put the hidden cursor on column 0 of the viewport's last row.
    fn park(&mut self, batch: &mut Batch) {
        if self.placed {
            self.go(batch, 0, self.top + self.height - 1);
        }
    }

    /// Move to the row and column relative to the cursor's row.
    fn go(&mut self, batch: &mut Batch, column: u16, row: u16) {
        if row < self.cursor {
            batch.put(format!("\x1b[{}A", self.cursor - row).as_bytes());
        } else if row > self.cursor {
            batch.put(format!("\x1b[{}B", row - self.cursor).as_bytes());
        }
        if column == 0 {
            batch.put(b"\r");
        } else {
            batch.put(format!("\x1b[{}G", u32::from(column) + 1).as_bytes());
        }
        self.cursor = row;
    }

    /// Move to the row and column by absolute address.
    fn jump(&mut self, batch: &mut Batch, column: u16, row: u16) {
        batch.move_to(column, row);
        self.cursor = row;
    }

    /// Extra rows the last drawn live frame wraps into when the screen narrows to `cols`.
    fn wrapped_extra(&self, cols: u16) -> u16 {
        if cols == 0 || cols >= self.cols || self.overlay.is_some() {
            return 0;
        }
        let area = self.previous.area;
        let height = area.height.min(self.height);
        let mut extra: u16 = 0;
        for y in 0..height {
            let last = (0..area.width)
                .rev()
                .find(|&x| self.previous[(x, y)] != ratatui::buffer::Cell::EMPTY)
                .map_or(0, |x| x + 1);
            if last > cols {
                extra = extra.saturating_add((last - 1) / cols);
            }
        }
        extra
    }

    // tmux saves the visible screen to history when erase-below starts at the origin.
    fn clear_from_top_relative(&mut self, batch: &mut Batch) {
        self.go(batch, 0, self.top);
        if self.top == 0 {
            batch.put(CLEAR_LINE);
            if self.rows > 1 {
                self.go(batch, 0, 1);
                batch.put(CLEAR_BELOW);
                self.go(batch, 0, 0);
            }
        } else {
            batch.put(CLEAR_BELOW);
        }
    }

    fn clear_from_top_absolute(&mut self, batch: &mut Batch) {
        self.jump(batch, 0, self.top);
        if self.top == 0 {
            batch.put(CLEAR_LINE);
            if self.rows > 1 {
                self.jump(batch, 0, 1);
                batch.put(CLEAR_BELOW);
                self.jump(batch, 0, 0);
            }
        } else {
            batch.put(CLEAR_BELOW);
        }
    }

    /// A newline at column 0; at the last row it scrolls the screen.
    fn newline(&mut self, batch: &mut Batch) {
        batch.put(b"\r\n");
        if self.rows > 0
            && self.cursor >= self.rows - 1
            && let Some(scrolled) = &mut self.scrolled
        {
            *scrolled = scrolled.saturating_add(1);
        }
        self.cursor = (self.cursor + 1).min(self.rows.saturating_sub(1));
    }

    /// Write cells at their viewport positions, moving relative to the cursor.
    fn write_cells<'a>(
        &mut self,
        batch: &mut Batch,
        cells: impl Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    ) -> io::Result<()> {
        let mut at: Option<(u16, u16)> = None;
        let mut current: Option<Style> = None;
        for (x, y, cell) in cells {
            if at != Some((x, y)) {
                self.go(batch, x, self.top + y);
            }
            let style = cell_style(cell);
            if current != Some(style) {
                batch.put(RESET_STYLE);
                if style != Style::default() {
                    queue!(batch.bytes, SetStyle(style.into_crossterm()))?;
                }
                current = Some(style);
            }
            batch.put(cell.symbol().as_bytes());
            at = Some((x + cell.symbol().cell_width().max(1), y));
        }
        if current.is_some() {
            batch.put(RESET_STYLE);
        }
        Ok(())
    }

    /// Write the last live frame again, row by row below the cursor, with newlines.
    fn repaint_live_rows(&mut self, batch: &mut Batch) -> io::Result<()> {
        let frame = std::mem::replace(&mut self.previous, Buffer::empty(Rect::default()));
        let area = frame.area;
        for y in 0..self.height {
            self.newline(batch);
            if y >= area.height {
                continue;
            }
            let last = (0..area.width)
                .rev()
                .find(|&x| frame[(x, y)] != ratatui::buffer::Cell::EMPTY)
                .map_or(0, |x| x + 1);
            let mut style: Option<Style> = None;
            let mut x = 0;
            while x < last {
                let cell = &frame[(x, y)];
                let wanted = cell_style(cell);
                if style != Some(wanted) {
                    batch.put(RESET_STYLE);
                    if wanted != Style::default() {
                        queue!(batch.bytes, SetStyle(wanted.into_crossterm()))?;
                    }
                    style = Some(wanted);
                }
                batch.put(cell.symbol().as_bytes());
                x += cell.symbol().cell_width().max(1);
            }
            batch.put(RESET_STYLE);
        }
        self.previous = frame;
        Ok(())
    }

    /// Make the viewport fit below its top by scrolling the whole screen up with newlines at
    /// the last row. Content above moves into scrollback; nothing is erased.
    fn make_room(&mut self, batch: &mut Batch) {
        if self.top + self.height > self.rows {
            let room = self.top + self.height - self.rows;
            if let Some(scrolled) = &mut self.scrolled {
                *scrolled = scrolled.saturating_add(u32::from(room));
            }
            self.go(batch, 0, self.rows - 1);
            for _ in 0..room {
                batch.put(b"\n");
            }
            self.top = self.rows - self.height;
        }
    }

    /// Place the viewport at the bottom of a screen that has just become big enough.
    fn place_at_bottom(&mut self, batch: &mut Batch) {
        self.height = self.height.clamp(1, max_live_height(self.rows));
        if let Some(scrolled) = &mut self.scrolled {
            *scrolled = scrolled.saturating_add(u32::from(self.height));
        }
        self.jump(batch, 0, self.rows - 1);
        for _ in 0..self.height {
            batch.put(b"\n");
        }
        self.top = self.rows - self.height;
        self.placed = true;
        self.repaint = true;
        self.clear = true;
    }

    /// Write the lines that waited, once, in order.
    fn write_queue(&mut self, batch: &mut Batch) -> io::Result<()> {
        let lines = std::mem::take(&mut self.queued);
        if lines.is_empty() {
            return Ok(());
        }
        let rows = wrap_lines(lines, self.cols);
        self.insert_rows(batch, &rows)
    }

    /// The insertion itself, relative to the cursor: from the viewport's top row, clear the
    /// live region, write each row (a newline at the last row scrolls the oldest row into
    /// scrollback), then write the live rows again below them.
    fn insert_rows(&mut self, batch: &mut Batch, rows: &[Line<'static>]) -> io::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let count = u16::try_from(rows.len()).unwrap_or(u16::MAX);
        self.spilled = 0;
        self.clear_from_top_relative(batch);
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                self.newline(batch);
            }
            batch.put(RESET_STYLE);
            batch.put(CLEAR_LINE);
            batch.styled_row(row)?;
        }
        batch.put(RESET_STYLE);
        self.top = self.top.saturating_add(count).min(self.rows - self.height);
        self.repaint_live_rows(batch)
    }

    /// Copy the viewport position into the restore link.
    fn update_link(&self) {
        let mut state = self.link.lock();
        state.top = self.top;
        state.placed = self.placed;
    }

    /// Write the batch. The state it planned is kept whatever happens; on an error the batch is
    /// dropped, not retried, and the next write starts by recovering.
    fn send(&mut self, batch: Batch) -> io::Result<usize> {
        let total = batch.bytes.len();
        let mut written = 0;
        let mut result = Ok(());
        while written < total {
            match self.out.write(&batch.bytes[written..]) {
                Ok(0) => {
                    result = Err(io::Error::from(io::ErrorKind::WriteZero));
                    break;
                }
                Ok(count) => written += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        if result.is_ok() && total > 0 {
            result = self.out.flush();
        }

        let mut recovered = false;
        {
            let mut state = self.link.lock();
            for &(at, mark) in &batch.marks {
                if at > written {
                    break;
                }
                match mark {
                    Mark::SyncOn => state.in_sync = true,
                    Mark::SyncOff => state.in_sync = false,
                    Mark::OverlayOn => state.overlay_open = true,
                    Mark::OverlayOff => state.overlay_open = false,
                    Mark::Recovered => recovered = true,
                }
            }
            state.top = self.top;
            state.placed = self.placed;
        }
        if recovered {
            self.dirty = false;
        }
        match result {
            Ok(()) => Ok(written),
            Err(error) => {
                self.dirty = true;
                self.repaint = true;
                self.clear = true;
                Err(error)
            }
        }
    }
}

/// The reader thread; dropping it stops and joins it.
pub struct InputThread {
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl InputThread {
    pub fn stop(self) {
        drop(self);
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

impl Drop for InputThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Start the key reader. `None` when standard input is not a terminal. The closure
/// returns false to stop the thread.
pub fn spawn_input<F>(mut forward: F) -> Option<InputThread>
where
    F: FnMut(crossterm::event::Event) -> bool + Send + 'static,
{
    if !io::stdin().is_terminal() {
        return None;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let handle = {
        let stop = Arc::clone(&stop);
        let finished = Arc::clone(&finished);
        thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match crossterm::event::poll(INPUT_POLL) {
                    Ok(true) => match crossterm::event::read() {
                        Ok(event) => {
                            if !forward(event) {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                    Ok(false) => {}
                    Err(_) => break,
                }
            }
            finished.store(true, Ordering::SeqCst);
        })
    };
    Some(InputThread {
        stop,
        finished,
        handle: Some(handle),
    })
}
