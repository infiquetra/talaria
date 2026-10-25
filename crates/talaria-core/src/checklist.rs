//! The agent's task checklist: typed items, tree order, and the live tracker.

use serde_json::Value;

use crate::session::{TodoState, TurnState};

/// One step's state, exactly Hermes's four words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    /// Parse one of the four exact lower-case words.
    pub fn parse(text: &str) -> Option<TodoStatus> {
        let _ = text;
        unimplemented!("#226")
    }

    /// Pending or in progress.
    pub fn is_active(self) -> bool {
        unimplemented!("#226")
    }
}

/// One parsed step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub parent: Option<String>,
}

/// One tree row: the item index and its depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRow {
    pub index: usize,
    pub depth: u8,
}

/// How many items of each state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChecklistCounts {
    pub total: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub pending: usize,
    pub cancelled: usize,
}

/// One parsed snapshot with its revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checklist {
    pub revision: u64,
    pub items: Vec<TodoItem>,
    pub skipped: usize,
}

impl Checklist {
    /// Parse the reducer's stored snapshot.
    pub fn from_state(state: &TodoState) -> Checklist {
        let _ = state;
        unimplemented!("#226")
    }

    /// Parse raw item values at this revision.
    pub fn from_values(revision: u64, items: &[Value]) -> Checklist {
        let _ = (revision, items);
        unimplemented!("#226")
    }

    /// No item parsed, whatever `skipped` is.
    pub fn is_empty(&self) -> bool {
        unimplemented!("#226")
    }

    /// Count the parsed items by state.
    pub fn counts(&self) -> ChecklistCounts {
        unimplemented!("#226")
    }

    /// Any pending or in-progress item.
    pub fn is_active(&self) -> bool {
        unimplemented!("#226")
    }

    /// Every item once, parents before children, depth first.
    pub fn tree(&self) -> Vec<TreeRow> {
        unimplemented!("#226")
    }

    /// The tree position of the step to show, if any.
    pub fn focus(&self) -> Option<usize> {
        unimplemented!("#226")
    }
}

/// Why a list was written to scrollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitKind {
    TurnEnded,
    Resumed,
}

/// One list written to scrollback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecklistCommit {
    pub checklist: Checklist,
    pub kind: CommitKind,
}

/// What one tracker update changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Observed {
    pub commit: Option<ChecklistCommit>,
    pub live_changed: bool,
}

/// When the checklist is visible and when it is committed.
#[allow(dead_code)]
pub struct ChecklistTracker {
    live: Option<Checklist>,
    committed_revision: Option<u64>,
    was_running: bool,
    expanded: bool,
}

impl ChecklistTracker {
    /// A tracker with no list, expanded.
    pub fn new() -> ChecklistTracker {
        ChecklistTracker {
            live: None,
            committed_revision: None,
            was_running: false,
            expanded: true,
        }
    }

    /// Take the list from a fresh snapshot, once.
    pub fn attach(&mut self, todo: Option<&TodoState>, running: bool) -> Option<ChecklistCommit> {
        let _ = (todo, running);
        unimplemented!("#226")
    }

    /// Take the list after one reducer update.
    pub fn observe(&mut self, todo: Option<&TodoState>, turn: &TurnState) -> Observed {
        let _ = (todo, turn);
        unimplemented!("#226")
    }

    /// The list in the live panel, if any.
    pub fn live(&self) -> Option<&Checklist> {
        self.live.as_ref()
    }

    /// Whether the panel shows every row.
    pub fn expanded(&self) -> bool {
        self.expanded
    }

    /// Flip the panel form. True when something visible changed.
    pub fn toggle(&mut self) -> bool {
        unimplemented!("#226")
    }

    /// Forget the live list and committed revision, keeping the form.
    pub fn reset(&mut self) {
        unimplemented!("#226")
    }
}

impl Default for ChecklistTracker {
    fn default() -> Self {
        Self::new()
    }
}
