//! The `@` query at the caret and the list that follows it.

use std::time::Instant;

use hermes_contract::schemas::CompletionItem;

/// The `@` word under the caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionQuery {
    /// Byte offset of `@`.
    pub start: usize,
    /// The word, including `@`.
    pub word: String,
}

/// Tab completes; Enter inserts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accept {
    /// Tab.
    Complete,
    /// Enter.
    Insert,
}

/// The draft after one acceptance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    /// The draft with the query replaced.
    pub draft: String,
    /// Caret byte, at the end of the insert.
    pub caret: usize,
    /// Whether the list stays open.
    pub keep_open: bool,
}

/// One completion the list is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionRequest {
    /// The word sent as `word`.
    pub word: String,
    /// Generation of this edit.
    pub generation: u64,
}

/// Where the list is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MentionPhase {
    /// No query.
    Closed,
    /// Waiting out the debounce.
    Waiting { since: Instant },
    /// A request has been fired.
    Requested { since: Instant },
    /// Rows are showing.
    Items,
    /// The gateway returned nothing.
    Empty,
    /// The gateway failed. The string is the plain reason.
    Unavailable(String),
    /// Esc dismissed this word.
    Dismissed { word: String },
}

/// List state. Time is only the `Instant` arguments.
pub struct MentionState {
    phase: MentionPhase,
    query: Option<MentionQuery>,
    generation: u64,
    items: Vec<CompletionItem>,
    selected: usize,
    offset: usize,
}

impl Default for MentionState {
    fn default() -> Self {
        Self::new()
    }
}

impl MentionState {
    /// Closed, generation 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: MentionPhase::Closed,
            query: None,
            generation: 0,
            items: Vec::new(),
            selected: 0,
            offset: 0,
        }
    }

    fn note(&self) {
        let _ = (
            &self.phase,
            &self.query,
            self.generation,
            self.items.len(),
            self.selected,
            self.offset,
        );
    }

    /// Start or restart the wait after an edit.
    pub fn on_edit(&mut self, draft: &str, caret: usize, now: Instant) {
        self.note();
        let _ = (draft, caret, now);
        unimplemented!()
    }

    /// Fire the request when the wait is over.
    pub fn tick(&mut self, now: Instant) -> Option<CompletionRequest> {
        self.note();
        let _ = now;
        unimplemented!()
    }

    /// When the loop should tick next.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.note();
        unimplemented!()
    }

    /// Store a reply for `generation`. A stale generation is ignored.
    pub fn on_items(&mut self, generation: u64, items: Vec<CompletionItem>) {
        self.note();
        let _ = (generation, items);
        unimplemented!()
    }

    /// Record a failure for `generation`.
    pub fn on_failed(&mut self, generation: u64, reason: String) {
        self.note();
        let _ = (generation, reason);
        unimplemented!()
    }

    /// A query is present and this word was not dismissed.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.note();
        unimplemented!()
    }

    /// The list is showing rows.
    #[must_use]
    pub fn has_items(&self) -> bool {
        self.note();
        unimplemented!()
    }

    /// Move the selection down, wrapping, and keep it in the window.
    pub fn next(&mut self) {
        self.note();
        unimplemented!()
    }

    /// Move the selection up, wrapping, and keep it in the window.
    pub fn prev(&mut self) {
        self.note();
        unimplemented!()
    }

    /// Dismiss the list for the current word.
    pub fn dismiss(&mut self) {
        self.note();
        unimplemented!()
    }

    /// The selected row, when the list has rows.
    #[must_use]
    pub fn selected(&self) -> Option<&CompletionItem> {
        self.note();
        unimplemented!()
    }

    /// Rows, status and footer for one draw.
    #[must_use]
    pub fn view(&self, now: Instant) -> MentionView {
        self.note();
        let _ = now;
        unimplemented!()
    }
}

/// What the list section draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionView {
    /// Display and meta of each visible row.
    pub rows: Vec<(String, String)>,
    /// Selected index inside `rows`.
    pub selected: Option<usize>,
    /// Status line, when one is showing.
    pub status: Option<String>,
    /// The fixed footer.
    pub footer: &'static str,
}

/// The `@` query at `caret`, when the caret ends the word.
#[must_use]
pub fn mention_at(draft: &str, caret: usize) -> Option<MentionQuery> {
    let _ = (draft, caret);
    unimplemented!()
}

/// Drop profile rows. Keep the gateway's order.
#[must_use]
pub fn filter_items(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    let _ = items;
    unimplemented!()
}

/// Replace the query with `item`, quoting the value.
#[must_use]
pub fn accept(draft: &str, query: &MentionQuery, item: &CompletionItem, how: Accept) -> Accepted {
    let _ = (draft, query, item, how);
    unimplemented!()
}

/// Quote a reference value the way the gateway expects.
#[must_use]
pub fn format_ref_value(value: &str) -> String {
    let _ = value;
    unimplemented!()
}

/// Built-in `@` references in `text`.
#[must_use]
pub fn count_references(text: &str) -> usize {
    let _ = text;
    unimplemented!()
}

/// The notice when a message has more references than the gateway expands.
#[must_use]
pub fn reference_notice(count: usize) -> Option<&'static str> {
    let _ = count;
    unimplemented!()
}
