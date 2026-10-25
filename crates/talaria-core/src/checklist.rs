//! The agent's task checklist: typed items, tree order, and the live tracker.

use std::collections::BTreeMap;

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
        match text {
            "pending" => Some(TodoStatus::Pending),
            "in_progress" => Some(TodoStatus::InProgress),
            "completed" => Some(TodoStatus::Completed),
            "cancelled" => Some(TodoStatus::Cancelled),
            _ => None,
        }
    }

    /// Pending or in progress.
    pub fn is_active(self) -> bool {
        matches!(self, TodoStatus::Pending | TodoStatus::InProgress)
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
        match u64::try_from(state.revision) {
            Ok(revision) => Checklist::from_values(revision, &state.todos),
            // A revision that cannot be a count is no plan at all.
            Err(_) => Checklist {
                revision: 0,
                items: Vec::new(),
                skipped: 0,
            },
        }
    }

    /// Parse raw item values at this revision.
    pub fn from_values(revision: u64, items: &[Value]) -> Checklist {
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut skipped = 0;
        for value in items {
            match parse_item(value) {
                Some(candidate) => candidates.push(candidate),
                None => skipped += 1,
            }
        }
        // The first item with an id is the parent of every item that names it. A parent
        // that would point at the item itself is dropped.
        let mut first: BTreeMap<String, usize> = BTreeMap::new();
        for (index, candidate) in candidates.iter().enumerate() {
            first.entry(candidate.id.clone()).or_insert(index);
        }
        let mut parsed = Vec::with_capacity(candidates.len());
        for (index, candidate) in candidates.into_iter().enumerate() {
            let parent = candidate
                .parent
                .filter(|parent| first.get(parent.as_str()) != Some(&index));
            parsed.push(TodoItem {
                id: candidate.id,
                content: candidate.content,
                status: candidate.status,
                parent,
            });
        }
        Checklist {
            revision,
            items: parsed,
            skipped,
        }
    }

    /// No item parsed, whatever `skipped` is.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Count the parsed items by state.
    pub fn counts(&self) -> ChecklistCounts {
        let mut counts = ChecklistCounts {
            total: self.items.len(),
            ..ChecklistCounts::default()
        };
        for item in &self.items {
            match item.status {
                TodoStatus::Pending => counts.pending += 1,
                TodoStatus::InProgress => counts.in_progress += 1,
                TodoStatus::Completed => counts.completed += 1,
                TodoStatus::Cancelled => counts.cancelled += 1,
            }
        }
        counts
    }

    /// Any pending or in-progress item.
    pub fn is_active(&self) -> bool {
        self.items.iter().any(|item| item.status.is_active())
    }

    /// Every item once, parents before children, depth first.
    pub fn tree(&self) -> Vec<TreeRow> {
        let count = self.items.len();
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut roots: Vec<usize> = Vec::new();
        for index in 0..count {
            match self.parent_index(index) {
                Some(parent) => children[parent].push(index),
                None => roots.push(index),
            }
        }
        let mut rows = Vec::with_capacity(count);
        let mut visited = vec![false; count];
        for root in roots {
            self.walk(root, 0, &mut children, &mut visited, &mut rows);
        }
        // Items no root reaches: a cycle, and whatever hangs under it.
        for (index, seen) in visited.iter_mut().enumerate() {
            if !*seen {
                *seen = true;
                rows.push(TreeRow { index, depth: 0 });
            }
        }
        rows
    }

    /// The tree position of the step to show, if any.
    pub fn focus(&self) -> Option<usize> {
        let tree = self.tree();
        let in_progress = tree
            .iter()
            .position(|row| self.items[row.index].status == TodoStatus::InProgress);
        in_progress.or_else(|| {
            tree.iter()
                .position(|row| self.items[row.index].status == TodoStatus::Pending)
        })
    }

    /// The index of the item this one hangs under, when that item is in the list.
    fn parent_index(&self, index: usize) -> Option<usize> {
        let item = &self.items[index];
        let parent = item.parent.as_ref()?;
        // The first item with that id is the parent; an item that names itself is a root.
        let found = self.items.iter().position(|other| &other.id == parent)?;
        (found != index).then_some(found)
    }

    /// Emit one item and everything under it, once.
    fn walk(
        &self,
        index: usize,
        depth: u8,
        children: &mut [Vec<usize>],
        visited: &mut [bool],
        rows: &mut Vec<TreeRow>,
    ) {
        if visited[index] {
            return;
        }
        visited[index] = true;
        rows.push(TreeRow { index, depth });
        let under = std::mem::take(&mut children[index]);
        for child in under {
            self.walk(child, depth.saturating_add(1), children, visited, rows);
        }
    }
}

/// One item's id or parent: a trimmed string, or a number in decimal.
fn identifier(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// One item before the list decides who its parent is.
struct Candidate {
    id: String,
    content: String,
    status: TodoStatus,
    parent: Option<String>,
}

/// One item, when it carries everything a row needs.
fn parse_item(value: &Value) -> Option<Candidate> {
    let object = value.as_object()?;
    let status = TodoStatus::parse(object.get("status")?.as_str()?)?;
    let id = identifier(object.get("id")?)?;
    if id.is_empty() {
        return None;
    }
    let content = object.get("content")?.as_str()?.trim().to_string();
    if content.is_empty() {
        return None;
    }
    let parent = object
        .get("parent")
        .and_then(identifier)
        .filter(|parent| !parent.is_empty());
    Some(Candidate {
        id,
        content,
        status,
        parent,
    })
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
        let list = todo
            .map(Checklist::from_state)
            .filter(|list| !list.is_empty());
        self.was_running = running;
        match list {
            None => None,
            Some(list) if running => {
                self.live = Some(list);
                None
            }
            Some(list) => {
                self.committed_revision = Some(list.revision);
                self.live = None;
                Some(ChecklistCommit {
                    checklist: list,
                    kind: CommitKind::Resumed,
                })
            }
        }
    }

    /// Take the list after one reducer update.
    pub fn observe(&mut self, todo: Option<&TodoState>, turn: &TurnState) -> Observed {
        let running = matches!(turn, TurnState::Running);
        let list = todo
            .map(Checklist::from_state)
            .filter(|list| !list.is_empty());
        let before = self.live.clone();
        let mut commit = None;

        if running && let Some(list) = list.as_ref() {
            let newer = self
                .committed_revision
                .is_none_or(|committed| list.revision > committed);
            if self.live.is_some() || newer {
                self.live = Some(list.clone());
            }
        }
        // An empty list is a real clear: the panel goes, and nothing is written.
        if list.is_none() && self.live.is_some() {
            self.live = None;
        }
        if self.was_running
            && !running
            && let Some(live) = self.live.take()
        {
            let checklist = match list.as_ref() {
                Some(current) if current.revision >= live.revision => current.clone(),
                _ => live,
            };
            self.committed_revision = Some(checklist.revision);
            commit = Some(ChecklistCommit {
                checklist,
                kind: CommitKind::TurnEnded,
            });
        }
        self.was_running = running;
        Observed {
            commit,
            live_changed: self.live != before,
        }
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
        self.expanded = !self.expanded;
        self.live.is_some()
    }

    /// Forget the live list and committed revision, keeping the form.
    pub fn reset(&mut self) {
        self.live = None;
        self.committed_revision = None;
        self.was_running = false;
    }
}

impl Default for ChecklistTracker {
    fn default() -> Self {
        Self::new()
    }
}
