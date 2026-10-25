//! History search box: view state over submitted prompts.
//!
//! Pure view state; drawing is a live section. Behavior contract: issue 228.

use std::sync::Arc;
use std::time::SystemTime;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::prompt_history::HistoryEntry;

use crate::keys::{Key, KeyChord};
use crate::live::LiveSection;

/// What a search key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchOutcome {
    /// Nothing changed.
    Unchanged,
    /// The query, scope, or selection changed.
    Changed,
    /// Load this text into the composer, unsent.
    Accept(String),
    /// Close the box and restore the saved draft.
    Cancel,
}

/// The open search box.
#[derive(Debug)]
pub struct HistorySearch {
    /// Placeholder until the build adds the view state.
    _private: (),
}

impl HistorySearch {
    /// Open the box over these entries, saving the draft.
    pub fn open(entries: Arc<[HistoryEntry]>, draft: String, search_chords: Vec<KeyChord>) -> Self {
        let _ = (entries, draft, search_chords);
        todo!("#228")
    }

    /// Apply one key.
    pub fn handle_key(&mut self, key: Key, now: SystemTime) -> SearchOutcome {
        let _ = (key, now);
        todo!("#228")
    }

    /// Append the first pasted line to the query.
    pub fn handle_paste(&mut self, text: &str) {
        let _ = text;
        todo!("#228")
    }

    /// The draft saved when the box opened.
    pub fn saved_draft(&self) -> &str {
        todo!("#228")
    }

    /// The box as a live section replacing the composer.
    pub fn section(&self, now: SystemTime) -> impl LiveSection + '_ {
        let _ = now;
        HistorySection { _private: () }
    }
}

/// The search box as a live section. Stub.
#[derive(Debug)]
pub struct HistorySection {
    /// Placeholder until the build adds the rendered rows.
    _private: (),
}

impl LiveSection for HistorySection {
    fn height(&self, _width: u16) -> u16 {
        todo!("#228")
    }

    fn render(&self, _area: Rect, _buf: &mut Buffer) {
        todo!("#228")
    }
}
