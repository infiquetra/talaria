//! A finished batch: one `delegate_task` call's children, all terminal.
//! Issue #242.

use super::row::SubagentRow;

/// One batch, root first in depth-first order, with each row's walk depth.
#[derive(Clone, Debug, PartialEq)]
pub struct FinishedBatch {
    pub key: String,
    pub rows: Vec<(SubagentRow, u8)>,
}
