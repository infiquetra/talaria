//! Labels in a draft and the payloads that belong to them.

use std::ops::Range;

use crate::session::SessionId;

use super::expand::{ExpandError, Expanded};

/// When a paste becomes a label. Zero turns that rule off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollapseRule {
    /// Minimum line count that collapses.
    pub lines: usize,
    /// Minimum character count that collapses.
    pub chars: usize,
}

impl CollapseRule {
    /// Five lines or 2,000 characters.
    pub const HERMES_DEFAULT: Self = Self {
        lines: 5,
        chars: 2000,
    };

    /// Whether `text` meets either threshold.
    #[must_use]
    pub fn should_collapse(&self, text: &str) -> bool {
        let _ = (self.lines, self.chars, text);
        unimplemented!()
    }
}

/// Identity of one chip. Paste numbers are not chip ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChipId(pub u64);

impl ChipId {
    /// The raw counter value.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Where an image label is in its life.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageState {
    /// Bytes are being staged.
    Pending { session: SessionId },
    /// The gateway accepted the bytes.
    Ready {
        session: SessionId,
        path: String,
        mime: String,
    },
    /// A receipt already covered this image.
    Sent,
}

/// What a chip holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipKind {
    /// Collapsed paste text.
    Paste { text: String, lines: usize },
    /// An image label.
    Image { n: u32, state: ImageState },
    /// A file label.
    File {
        n: u32,
        name: String,
        ref_text: String,
        session: SessionId,
    },
}

/// One label and its payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    /// Stable id for this payload.
    pub id: ChipId,
    /// The exact label text in the draft.
    pub label: String,
    /// Payload behind that label.
    pub kind: ChipKind,
}

/// A chip could not be added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipError {
    /// The draft already has ten live images.
    TooManyImages,
}

/// Paste texts and the labels that point at them.
pub struct ChipStore {
    budget: usize,
    next_paste: u64,
    chips: Vec<Chip>,
    held: usize,
}

impl ChipStore {
    /// A store that keeps at most `paste_budget_bytes` of paste text.
    #[must_use]
    pub fn new(paste_budget_bytes: usize) -> Self {
        Self {
            budget: paste_budget_bytes,
            next_paste: 1,
            chips: Vec::new(),
            held: 0,
        }
    }

    fn note(&self) {
        let _ = (self.budget, self.next_paste, self.chips.len(), self.held);
    }

    /// Collapse `text` into a label and evict older pastes while over budget.
    pub fn collapse_paste(&mut self, text: &str) -> String {
        self.note();
        let _ = text;
        unimplemented!()
    }

    /// Start an image label. The number is one past the highest in `draft`.
    pub fn begin_image(
        &mut self,
        draft: &str,
        session: &SessionId,
    ) -> Result<(ChipId, String), ChipError> {
        self.note();
        let _ = (draft, session);
        unimplemented!()
    }

    /// Mark an image ready with the gateway path and media type.
    pub fn image_ready(&mut self, id: ChipId, path: String, mime: String) {
        self.note();
        let _ = (id, path, mime);
        unimplemented!()
    }

    /// Drop a failed image and return the label to remove.
    pub fn image_failed(&mut self, id: ChipId) -> Option<String> {
        self.note();
        let _ = id;
        unimplemented!()
    }

    /// Add a file label. `name` is sanitized before it enters the label.
    pub fn add_file(
        &mut self,
        draft: &str,
        name: &str,
        ref_text: String,
        session: &SessionId,
    ) -> String {
        self.note();
        let _ = (draft, name, ref_text, session);
        unimplemented!()
    }

    /// Drop chips whose label is no longer intact in `draft`.
    pub fn reconcile(&mut self, draft: &str) {
        self.note();
        let _ = draft;
        unimplemented!()
    }

    /// Live chips and the first intact range of each label.
    #[must_use]
    pub fn live<'a>(&'a self, draft: &str) -> Vec<(&'a Chip, Range<usize>)> {
        self.note();
        let _ = draft;
        unimplemented!()
    }

    /// The live label that ends at `byte`, if the caret is just after it.
    #[must_use]
    pub fn label_ending_at(&self, draft: &str, byte: usize) -> Option<Range<usize>> {
        self.note();
        let _ = (draft, byte);
        unimplemented!()
    }

    /// The live label that starts at `byte`, if the caret is just before it.
    #[must_use]
    pub fn label_starting_at(&self, draft: &str, byte: usize) -> Option<Range<usize>> {
        self.note();
        let _ = (draft, byte);
        unimplemented!()
    }

    /// Expand live labels. Image labels stay in the text.
    #[must_use]
    pub fn expand(&self, draft: &str) -> Result<Expanded, ExpandError> {
        self.note();
        let _ = draft;
        unimplemented!()
    }

    /// Mark the images in `expanded` sent. Call this only after a receipt.
    pub fn mark_sent(&mut self, expanded: &Expanded) {
        self.note();
        let _ = expanded;
        unimplemented!()
    }

    /// Drop image and file chips for `session`. True when any were dropped.
    pub fn drop_session_payloads(&mut self, session: &SessionId) -> bool {
        self.note();
        let _ = session;
        unimplemented!()
    }
}
