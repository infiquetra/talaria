//! The card stack: which card is drawn, who takes keys, per card view state.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::session::PromptCell;

use super::{KeyOutcome, MaskedView};
use crate::keys::Key;
use crate::live::LiveSection;

/// View state only: highlight, note, ticks, typed text, masked buffer, focus time.
pub struct PromptStack {
    _private: (),
}

impl Default for PromptStack {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptStack {
    pub fn new() -> Self {
        todo!("prompts")
    }

    pub fn sync(
        &mut self,
        foreground: Option<&str>,
        prompts: &[&PromptCell],
        masked: &[MaskedView],
        draft_empty: bool,
        now: Instant,
    ) {
        let _ = (foreground, prompts, masked, draft_empty, now);
        todo!("prompts")
    }

    pub fn handle_key(&mut self, key: Key, now: Instant) -> KeyOutcome {
        let _ = (key, now);
        todo!("prompts")
    }

    pub fn handle_paste(&mut self, text: &str, now: Instant) -> KeyOutcome {
        let _ = (text, now);
        todo!("prompts")
    }

    /// No interrupt was sent, or the call returned.
    pub fn stop_cleared(&mut self) {
        todo!("prompts")
    }

    /// The drawn card, if any.
    pub fn section(&self) -> Option<PromptSection<'_>> {
        todo!("prompts")
    }
}

/// The drawn card as a live region section.
pub struct PromptSection<'a> {
    _stack: &'a PromptStack,
}

impl LiveSection for PromptSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = (width, self._stack);
        todo!("prompts")
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (area, buf);
        todo!("prompts")
    }
}
