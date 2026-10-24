//! Paste burst detector. The caller supplies the clock.
//!
//! Without bracketed paste, a paste arrives as plain characters a few milliseconds apart. The
//! third character in a row at most 8 ms after the one before starts a burst: the two typed
//! before it are taken back out of the draft and buffered with it, and the buffer is inserted
//! as one paste after 8 ms of quiet. An Enter less than 120 ms after the last burst event is a
//! line break, not a submit.

use std::time::{Duration, Instant};

/// Characters in a row that start a burst.
pub const BURST_MIN_CHARS: usize = 3;
/// The largest gap between two characters of one burst.
pub const BURST_CHAR_INTERVAL: Duration = Duration::from_millis(8);
/// Quiet time after which a burst is flushed.
pub const BURST_IDLE_TIMEOUT: Duration = Duration::from_millis(8);
/// How long after the last burst event an Enter still adds a line.
pub const ENTER_SUPPRESS_WINDOW: Duration = Duration::from_millis(120);

/// What to do with one plain character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharStep {
    /// Type this character now.
    Insert,
    /// Delete this many characters before the caret and start a buffer.
    TakeBack(usize),
    /// Add this character to the burst buffer.
    Append,
}

/// What Enter should do given the burst and its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterStep {
    /// Submit the draft.
    Submit,
    /// Insert a line break in the draft.
    Newline,
    /// Add a line break to the burst buffer.
    Append,
}

/// Consecutive fast characters and the short window after they flush.
#[derive(Debug)]
pub struct PasteBurst {
    /// Characters in the current run, each at most the interval after the one before.
    consecutive: usize,
    /// When the last plain character arrived.
    last_char: Option<Instant>,
    /// The characters of the current run that were typed into the draft.
    typed: Vec<char>,
    /// The active burst, when there is one.
    buffer: Option<String>,
    /// When the last burst event happened: the start of the Enter window.
    last_event: Option<Instant>,
}

impl PasteBurst {
    /// No burst and no open window.
    #[must_use]
    pub fn new() -> Self {
        Self {
            consecutive: 0,
            last_char: None,
            typed: Vec::new(),
            buffer: None,
            last_event: None,
        }
    }

    /// Whether a burst buffer is collecting characters.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.buffer.is_some()
    }

    /// Characters of the current run typed into the draft.
    pub(crate) fn run_len(&self) -> usize {
        self.typed.len()
    }

    /// Feed one plain character.
    pub fn on_char(&mut self, c: char, now: Instant) -> CharStep {
        if let Some(buffer) = &mut self.buffer {
            buffer.push(c);
            self.last_char = Some(now);
            self.last_event = Some(now);
            return CharStep::Append;
        }
        let close = self
            .last_char
            .is_some_and(|last| now.saturating_duration_since(last) <= BURST_CHAR_INTERVAL);
        if close && self.consecutive > 0 {
            self.consecutive += 1;
        } else {
            self.consecutive = 1;
            self.typed.clear();
        }
        self.last_char = Some(now);
        if self.consecutive >= BURST_MIN_CHARS {
            let taken = self.typed.len();
            let mut buffer: String = self.typed.drain(..).collect();
            buffer.push(c);
            self.buffer = Some(buffer);
            self.consecutive = 0;
            self.last_event = Some(now);
            return CharStep::TakeBack(taken);
        }
        self.typed.push(c);
        CharStep::Insert
    }

    /// Feed Enter.
    pub fn on_enter(&mut self, now: Instant) -> EnterStep {
        if let Some(buffer) = &mut self.buffer {
            buffer.push('\n');
            self.last_event = Some(now);
            return EnterStep::Append;
        }
        self.reset_run();
        let open = self
            .last_event
            .is_some_and(|last| now.saturating_duration_since(last) < ENTER_SUPPRESS_WINDOW);
        if open {
            EnterStep::Newline
        } else {
            EnterStep::Submit
        }
    }

    /// A key that is not a plain character or Enter. Returns buffered text to flush, and
    /// closes the Enter window.
    pub fn on_other_key(&mut self) -> Option<String> {
        self.reset_run();
        self.last_event = None;
        self.buffer.take()
    }

    /// Flush the buffer once it has been quiet for 8 ms. The Enter window stays open.
    pub fn tick(&mut self, now: Instant) -> Option<String> {
        let due = self.next_due()?;
        if now < due {
            return None;
        }
        self.reset_run();
        self.buffer.take()
    }

    /// When [`Self::tick`] will next flush, if a burst is active.
    #[must_use]
    pub fn next_due(&self) -> Option<Instant> {
        self.buffer.as_ref()?;
        self.last_event.map(|last| last + BURST_IDLE_TIMEOUT)
    }

    /// Forget the run, the buffer and the window: the draft was replaced, or a bracketed
    /// paste arrived after its buffer was taken.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn reset_run(&mut self) {
        self.consecutive = 0;
        self.last_char = None;
        self.typed.clear();
    }
}

impl Default for PasteBurst {
    fn default() -> Self {
        Self::new()
    }
}
