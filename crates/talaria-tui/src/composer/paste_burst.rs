//! Paste burst detector. The caller supplies the clock.

use std::time::Instant;

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
pub struct PasteBurst;

impl PasteBurst {
    /// No burst and no open window.
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    /// Feed one plain character.
    pub fn on_char(&mut self, _c: char, _now: Instant) -> CharStep {
        todo!()
    }

    /// Feed Enter.
    pub fn on_enter(&mut self, _now: Instant) -> EnterStep {
        todo!()
    }

    /// A key that is not a plain character or Enter. Returns buffered text to flush.
    pub fn on_other_key(&mut self) -> Option<String> {
        todo!()
    }

    /// Flush the buffer once it has been quiet for 8 ms.
    pub fn tick(&mut self, _now: Instant) -> Option<String> {
        todo!()
    }

    /// When [`Self::tick`] will next flush, if a burst is active.
    #[must_use]
    pub fn next_due(&self) -> Option<Instant> {
        todo!()
    }
}

impl Default for PasteBurst {
    fn default() -> Self {
        Self::new()
    }
}
