//! The subagent tree: rows by id in insertion order. Issue #242.

use hermes_contract::schemas::SubagentListResult;

use super::batch::FinishedBatch;
use super::event::SubagentEvent;
use super::row::SubagentRow;

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
    /// Reserved for the rows. The stub holds nothing yet.
    _reserved: (),
}

impl SubagentTree {
    #[must_use]
    pub fn new() -> SubagentTree {
        todo!("#242")
    }

    pub fn apply(&mut self, _ev: &SubagentEvent, _now: Clock) -> Applied {
        todo!("#242")
    }

    pub fn reconcile(&mut self, _list: &SubagentListResult, _now: Clock) -> Applied {
        todo!("#242")
    }

    pub fn mark_stop_requested(&mut self, _id: &str) -> bool {
        todo!("#242")
    }

    pub fn take_finished_batches(&mut self) -> Vec<FinishedBatch> {
        todo!("#242")
    }

    pub fn prune_finished(&mut self) -> bool {
        todo!("#242")
    }

    /// Remove queued rows with no event for 600,000 ms. Never retires.
    pub fn expire(&mut self, _now: Clock) -> bool {
        todo!("#242")
    }

    #[must_use]
    pub fn rows_in_order(&self) -> Vec<TreeRow> {
        todo!("#242")
    }

    #[must_use]
    pub fn get(&self, _id: &str) -> Option<&SubagentRow> {
        todo!("#242")
    }

    #[must_use]
    pub fn row(&self, _index: usize) -> &SubagentRow {
        todo!("#242")
    }

    #[must_use]
    pub fn live_ids(&self) -> Vec<String> {
        todo!("#242")
    }

    #[must_use]
    pub fn counts(&self) -> SubagentCounts {
        todo!("#242")
    }

    /// True once a list has reconciled successfully.
    #[must_use]
    pub fn hydrated(&self) -> bool {
        todo!("#242")
    }

    #[must_use]
    pub fn version(&self) -> u64 {
        todo!("#242")
    }
}

impl Default for SubagentTree {
    fn default() -> Self {
        Self::new()
    }
}
