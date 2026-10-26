//! One-line row text. Not sanitized and not cut; the screen does both.

use crate::fleet::{FleetRow, NeedItem, RowState};

/// The one-line detail for a row: approval, question, sudo, secret, waiting, working,
/// starting, failed, completed, interrupted, finished or idle.
#[must_use]
pub fn row_detail(row: &FleetRow, items_of_row: &[&NeedItem]) -> String {
    let _ = (row, items_of_row);
    todo!("fleet view stub")
}

/// The mark in front of a row.
#[must_use]
pub fn glyph(state: RowState) -> char {
    let _ = state;
    todo!("fleet view stub")
}

/// The title, or `(untitled)` when the title is empty.
#[must_use]
pub fn title_or_untitled(row: &FleetRow) -> String {
    let _ = row;
    todo!("fleet view stub")
}

/// Whether a lower-cased query is a substring of the title, profile, short model, session id
/// or detail. An empty query matches every row.
#[must_use]
pub fn matches(row: &FleetRow, detail: &str, query: &str) -> bool {
    let _ = (row, detail, query);
    todo!("fleet view stub")
}
