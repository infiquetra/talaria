//! History browse state: the saved draft and edits made while recalling.

use std::collections::BTreeMap;

/// Most prompts kept. The oldest is dropped first.
pub const MAX_HISTORY: usize = 1_000;

/// Submitted prompts and the state of one walk through them.
#[derive(Debug, Default)]
pub struct Recall {
    entries: Vec<String>,
    /// The entry shown, while browsing.
    browsing: Option<usize>,
    /// The draft as it was before the first step back.
    scratch: Option<String>,
    /// Edited text of recalled entries, by index, for the rest of the browse.
    edits: BTreeMap<usize, String>,
}

impl Recall {
    /// A history seeded oldest first, by the same rule as a submit.
    #[must_use]
    pub fn with_entries(entries: Vec<String>) -> Self {
        let mut recall = Self::default();
        for entry in entries {
            recall.add(entry);
        }
        recall
    }

    /// Submitted prompts, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Add a prompt: never blank, never equal to the one before; the oldest goes past the cap.
    pub fn add(&mut self, text: String) {
        if text.trim().is_empty() || self.entries.last() == Some(&text) {
            return;
        }
        self.entries.push(text);
        if self.entries.len() > MAX_HISTORY {
            let extra = self.entries.len() - MAX_HISTORY;
            self.entries.drain(..extra);
        }
    }

    /// Whether a walk is in progress.
    #[must_use]
    pub fn is_browsing(&self) -> bool {
        self.browsing.is_some()
    }

    /// One step back. `draft` is the text shown now. Returns the text to show, or `None` at the
    /// oldest entry or with no history.
    pub fn older(&mut self, draft: &str) -> Option<String> {
        let target = match self.browsing {
            None if self.entries.is_empty() => return None,
            None => {
                self.scratch = Some(draft.to_string());
                self.entries.len() - 1
            }
            Some(0) => return None,
            Some(index) => {
                self.keep_edit(index, draft);
                index - 1
            }
        };
        self.browsing = Some(target);
        Some(self.shown(target))
    }

    /// One step forward. Returns the text to show, the saved draft past the newest entry, or
    /// `None` when not browsing.
    pub fn newer(&mut self, draft: &str) -> Option<String> {
        let index = self.browsing?;
        self.keep_edit(index, draft);
        if index + 1 < self.entries.len() {
            self.browsing = Some(index + 1);
            return Some(self.shown(index + 1));
        }
        self.browsing = None;
        Some(self.scratch.take().unwrap_or_default())
    }

    /// End the walk and forget its saved draft and edits.
    pub fn end(&mut self) {
        self.browsing = None;
        self.scratch = None;
        self.edits.clear();
    }

    fn shown(&self, index: usize) -> String {
        self.edits
            .get(&index)
            .unwrap_or(&self.entries[index])
            .clone()
    }

    fn keep_edit(&mut self, index: usize, draft: &str) {
        if self.shown(index) != draft {
            if self.entries[index] == draft {
                self.edits.remove(&index);
            } else {
                self.edits.insert(index, draft.to_string());
            }
        }
    }
}
