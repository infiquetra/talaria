//! The subagent tree: rows by id in insertion order. Issue #242.

use hermes_contract::schemas::SubagentListResult;
use std::collections::{HashMap, HashSet, VecDeque};

use super::batch::FinishedBatch;
use super::event::SubagentEvent;
use super::event::SubagentEventKind;
use super::row::{RowState, SubagentRow, listed_terminal, terminal};

/// The two clocks the tree reads: monotonic milliseconds for expiry and
/// windows, wall-clock seconds since 1970 for elapsed text and ordering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    pub mono_ms: u64,
    pub wall_s: f64,
}

/// One row of [`SubagentTree::rows_in_order`]: its index and walk depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRow {
    pub index: usize,
    pub depth: u8,
}

/// Row counts for the status line and the dock header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubagentCounts {
    pub live: u32,
    pub queued: u32,
    pub terminal: u32,
    pub failed: u32,
    pub stopped: u32,
}

/// What one `apply` did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub changed: bool,
    pub missing_id: bool,
}

/// The gateway's spawn limits, from `delegation.status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    pub max_spawn_depth: i64,
    pub max_concurrent_children: i64,
}

/// Whether new children may spawn. `Unknown` until `delegation.status`
/// answers, and after it answers -32601.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spawning {
    Unknown,
    On(Option<Caps>),
    Paused(Option<Caps>),
}

/// At most 24 activity lines per row; the oldest drops off.
pub const MAX_ACTIVITY: usize = 24;
/// At most 512 retired ids; the oldest is forgotten.
pub const RETIRED_CAP: usize = 512;
/// A queued row with no event for this long is removed, never retired.
pub const QUEUED_EXPIRY_MS: u64 = 600_000;

/// Rows by id, insertion order, the retired set, list-miss counters, version.
pub struct SubagentTree {
    rows: Vec<SubagentRow>,
    indices: HashMap<String, usize>,
    misses: HashMap<String, u8>,
    retired: HashSet<String>,
    retired_order: VecDeque<String>,
    returned: HashSet<String>,
    returned_order: VecDeque<String>,
    hydrated: bool,
    version: u64,
}

impl SubagentTree {
    #[must_use]
    pub fn new() -> SubagentTree {
        Self {
            rows: Vec::new(),
            indices: HashMap::new(),
            misses: HashMap::new(),
            retired: HashSet::new(),
            retired_order: VecDeque::new(),
            returned: HashSet::new(),
            returned_order: VecDeque::new(),
            hydrated: false,
            version: 0,
        }
    }

    pub fn apply(&mut self, ev: &SubagentEvent, now: Clock) -> Applied {
        let Some(id) = ev.payload.subagent_id.as_ref().filter(|s| !s.is_empty()) else {
            return Applied {
                changed: false,
                missing_id: true,
            };
        };
        if self.retired.contains(id) {
            return Applied::default();
        }
        let created = !self.indices.contains_key(id);
        if created {
            let state = match ev.kind {
                SubagentEventKind::SpawnRequested => RowState::Queued,
                SubagentEventKind::Start => RowState::Running,
                SubagentEventKind::Complete => terminal(ev.payload.status),
                _ => return Applied::default(),
            };
            self.indices.insert(id.clone(), self.rows.len());
            self.rows.push(SubagentRow::new(id.clone(), now, state));
        }
        let row = &mut self.rows[self.indices[id]];
        if row.state.is_terminal() && !created {
            return Applied::default();
        }
        row.last_event_ms = now.mono_ms;
        let before = row.clone();
        match ev.kind {
            SubagentEventKind::SpawnRequested => row.seen_live = true,
            SubagentEventKind::Start => {
                row.state = RowState::Running;
                row.seen_live = true;
            }
            SubagentEventKind::Complete => {
                row.state = terminal(ev.payload.status);
                if created {
                    row.seen_live = true;
                }
                row.stop_requested = false;
            }
            _ => row.state = RowState::Running,
        }
        row.merge_event(ev);
        let changed = created || *row != before;
        self.version += u64::from(changed);
        Applied {
            changed,
            missing_id: false,
        }
    }

    pub fn reconcile(&mut self, list: &SubagentListResult, now: Clock) -> Applied {
        let mut present = HashSet::new();
        let mut changed = false;
        for snapshot in &list.subagents {
            let id = &snapshot.subagent_id;
            if id.is_empty() || self.retired.contains(id) {
                continue;
            }
            present.insert(id.as_str());
            self.misses.remove(id);
            if listed_terminal(snapshot.status) {
                continue;
            }
            if !self.indices.contains_key(id) {
                self.indices.insert(id.clone(), self.rows.len());
                self.rows
                    .push(SubagentRow::new(id.clone(), now, RowState::Running));
                changed = true;
            }
            let row = &mut self.rows[self.indices[id]];
            if row.state.is_terminal() {
                continue;
            }
            let before = row.clone();
            row.state = RowState::Running;
            row.merge_snapshot(snapshot);
            changed |= *row != before;
        }
        for row in &mut self.rows {
            if row.state != RowState::Running || present.contains(row.id.as_str()) {
                continue;
            }
            let missed = self.misses.entry(row.id.clone()).or_default();
            *missed = missed.saturating_add(1);
            if *missed >= 2 {
                row.state = RowState::Gone;
                row.stop_requested = false;
                changed = true;
            }
        }
        self.hydrated = true;
        self.version += u64::from(changed);
        Applied {
            changed,
            missing_id: false,
        }
    }

    pub fn mark_stop_requested(&mut self, id: &str) -> bool {
        let Some(&index) = self.indices.get(id) else {
            return false;
        };
        let row = &mut self.rows[index];
        if row.state != RowState::Running || row.stop_requested {
            return false;
        }
        row.stop_requested = true;
        self.version += 1;
        true
    }

    pub fn take_finished_batches(&mut self) -> Vec<FinishedBatch> {
        let groups = self.batches();
        if groups.iter().any(|(_, rows)| {
            rows.iter().any(|r| {
                r.depth == 0
                    && self.rows[r.index].state.is_live()
                    && self.rows[r.index].delegation_id.is_none()
            })
        }) {
            return Vec::new();
        }
        let mut finished = Vec::new();
        for (key, rows) in groups {
            if self.returned.contains(&key)
                || !rows.iter().all(|r| self.rows[r.index].state.is_terminal())
                || !rows.iter().any(|r| self.rows[r.index].seen_live)
            {
                continue;
            }
            remember(&mut self.returned, &mut self.returned_order, key.clone());
            finished.push(FinishedBatch {
                key,
                rows: rows
                    .iter()
                    .map(|r| (self.rows[r.index].clone(), r.depth))
                    .collect(),
            });
        }
        finished
    }

    pub fn prune_finished(&mut self) -> bool {
        let mut remove = HashSet::new();
        for (key, rows) in self.batches() {
            if rows.iter().all(|r| self.rows[r.index].state.is_terminal())
                && (self.returned.contains(&key)
                    || !rows.iter().any(|r| self.rows[r.index].seen_live))
            {
                for r in rows {
                    remove.insert(self.rows[r.index].id.clone());
                }
            }
        }
        for row in &self.rows {
            if remove.contains(&row.id) {
                remember(&mut self.retired, &mut self.retired_order, row.id.clone());
            }
        }
        self.remove_ids(&remove)
    }

    /// Remove queued rows with no event for 600,000 ms. Never retires.
    pub fn expire(&mut self, now: Clock) -> bool {
        let remove = self
            .rows
            .iter()
            .filter(|r| {
                r.state == RowState::Queued
                    && now.mono_ms.saturating_sub(r.last_event_ms) >= QUEUED_EXPIRY_MS
            })
            .map(|r| r.id.clone())
            .collect();
        self.remove_ids(&remove)
    }

    #[must_use]
    pub fn rows_in_order(&self) -> Vec<TreeRow> {
        let mut roots = Vec::new();
        let mut children = vec![Vec::new(); self.rows.len()];
        for (index, row) in self.rows.iter().enumerate() {
            if let Some(parent) = row.parent_id.as_ref().and_then(|p| self.indices.get(p)) {
                children[*parent].push(index);
            } else {
                roots.push(index);
            }
        }
        let compare = |a: &usize, b: &usize| {
            let a = &self.rows[*a];
            let b = &self.rows[*b];
            a.started_at
                .unwrap_or(a.first_seen_s)
                .total_cmp(&b.started_at.unwrap_or(b.first_seen_s))
                .then(a.task_index.cmp(&b.task_index))
                .then(a.goal.cmp(&b.goal))
        };
        roots.sort_by(compare);
        for siblings in &mut children {
            siblings.sort_by(compare);
        }
        let mut seen = vec![false; self.rows.len()];
        let mut order = Vec::with_capacity(self.rows.len());
        let mut stack: Vec<_> = roots.into_iter().rev().map(|i| (i, 0u8)).collect();
        while let Some((index, depth)) = stack.pop() {
            if seen[index] {
                continue;
            }
            seen[index] = true;
            order.push(TreeRow { index, depth });
            stack.extend(
                children[index]
                    .iter()
                    .rev()
                    .map(|&i| (i, depth.saturating_add(1).min(8))),
            );
        }
        for (index, visited) in seen.iter().enumerate() {
            if !visited {
                order.push(TreeRow { index, depth: 0 });
            }
        }
        order
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SubagentRow> {
        self.indices.get(id).map(|&index| &self.rows[index])
    }

    #[must_use]
    pub fn row(&self, index: usize) -> &SubagentRow {
        &self.rows[index]
    }

    #[must_use]
    pub fn live_ids(&self) -> Vec<String> {
        self.rows_in_order()
            .iter()
            .filter_map(|r| {
                let row = &self.rows[r.index];
                row.state.is_live().then(|| row.id.clone())
            })
            .collect()
    }

    #[must_use]
    pub fn counts(&self) -> SubagentCounts {
        let mut counts = SubagentCounts::default();
        for row in &self.rows {
            match row.state {
                RowState::Queued => {
                    counts.live += 1;
                    counts.queued += 1;
                }
                RowState::Running => counts.live += 1,
                RowState::Completed => counts.terminal += 1,
                RowState::Failed(_) | RowState::Gone => {
                    counts.terminal += 1;
                    counts.failed += 1;
                }
                RowState::Interrupted => {
                    counts.terminal += 1;
                    counts.stopped += 1;
                }
            }
        }
        counts
    }

    /// True once a list has reconciled successfully.
    #[must_use]
    pub fn hydrated(&self) -> bool {
        self.hydrated
    }

    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    fn remove_ids(&mut self, ids: &HashSet<String>) -> bool {
        if ids.is_empty() {
            return false;
        }
        self.rows.retain(|r| !ids.contains(&r.id));
        self.indices.clear();
        for (index, row) in self.rows.iter().enumerate() {
            self.indices.insert(row.id.clone(), index);
        }
        self.misses.retain(|id, _| !ids.contains(id));
        self.version += 1;
        true
    }

    fn batches(&self) -> Vec<(String, Vec<TreeRow>)> {
        let mut groups: Vec<(String, Vec<TreeRow>)> = Vec::new();
        let mut root_keys = Vec::new();
        for row in self.rows_in_order() {
            let key = if row.depth == 0 {
                root_keys.clear();
                self.rows[row.index]
                    .delegation_id
                    .clone()
                    .unwrap_or_else(|| format!("row:{}", self.rows[row.index].id))
            } else {
                root_keys.last().cloned().unwrap_or_default()
            };
            if row.depth == 0 {
                root_keys.push(key.clone());
            }
            if let Some((_, members)) = groups.iter_mut().find(|(existing, _)| *existing == key) {
                members.push(row);
            } else {
                groups.push((key, vec![row]));
            }
        }
        groups
    }
}

fn remember(set: &mut HashSet<String>, order: &mut VecDeque<String>, key: String) {
    if !set.insert(key.clone()) {
        return;
    }
    order.push_back(key);
    if order.len() > RETIRED_CAP
        && let Some(old) = order.pop_front()
    {
        set.remove(&old);
    }
}

impl Default for SubagentTree {
    fn default() -> Self {
        Self::new()
    }
}
