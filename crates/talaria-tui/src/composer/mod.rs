//! Multi-line draft editor. It writes no escape sequence and calls no gateway.

mod buffer;
pub mod paste_burst;
pub mod recall;
mod render;
pub mod wrap;

pub use render::ComposerSection;

use std::time::Instant;

use crate::keys::{Key, KeyMap};

/// What the composer should do with one key or one paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerOutcome {
    /// The key did nothing visible.
    Unchanged,
    /// The draft or the hint changed and should be drawn again.
    Changed,
    /// Send this text. Trailing whitespace has already been removed.
    Submit(String),
    /// Stop the running turn. The draft is kept.
    Interrupt,
    /// The person confirmed they want to leave.
    ExitRequested,
}

/// Facts from outside the draft that change what a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposerContext {
    /// A turn is in progress.
    pub turn_running: bool,
}

/// The draft, its history, and the paste and exit timers.
#[derive(Debug)]
pub struct Composer;

impl Composer {
    /// An empty draft bound to `keymap`.
    #[must_use]
    pub fn new(_keymap: KeyMap) -> Self {
        todo!()
    }

    /// An empty draft whose history is seeded, oldest first.
    #[must_use]
    pub fn with_history(_keymap: KeyMap, _entries: Vec<String>) -> Self {
        todo!()
    }

    /// Apply one key.
    pub fn handle_key(
        &mut self,
        _key: Key,
        _ctx: ComposerContext,
        _now: Instant,
    ) -> ComposerOutcome {
        todo!()
    }

    /// Insert a bracketed paste. Never submits.
    pub fn handle_paste(&mut self, _text: &str) -> ComposerOutcome {
        todo!()
    }

    /// Flush a paste burst or retire an exit hint when `now` has reached it.
    pub fn tick(&mut self, _now: Instant) -> ComposerOutcome {
        todo!()
    }

    /// When the caller should wake with no key, if something is pending.
    #[must_use]
    pub fn next_tick(&self) -> Option<Instant> {
        todo!()
    }

    /// The draft text.
    #[must_use]
    pub fn text(&self) -> &str {
        todo!()
    }

    /// Byte offset of the caret, on a grapheme boundary.
    #[must_use]
    pub fn caret(&self) -> usize {
        todo!()
    }

    /// The draft has no characters. Spaces are not empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// The caret is on the first visual row.
    #[must_use]
    pub fn cursor_on_first_row(&self) -> bool {
        todo!()
    }

    /// Submitted prompts, oldest first.
    #[must_use]
    pub fn history(&self) -> &[String] {
        todo!()
    }

    /// Replace the draft. The caret lands at the end. History is unchanged.
    pub fn load(&mut self, _text: String) {
        todo!()
    }

    /// Remove the draft, end a history browse, and disarm exit.
    pub fn clear(&mut self) {
        todo!()
    }

    /// Title drawn on the top border. `None` clears it.
    pub fn set_title(&mut self, _title: Option<String>) {
        todo!()
    }

    /// Whether Shift+Enter can be sent. The default is true.
    pub fn set_keyboard_enhancement(&mut self, _on: bool) {
        todo!()
    }

    /// Prepare wrapping for this width and return the section to draw.
    pub fn section(&mut self, _width: u16, _max_rows: u16) -> ComposerSection<'_> {
        let _view = ComposerSection::new(self);
        todo!()
    }
}
