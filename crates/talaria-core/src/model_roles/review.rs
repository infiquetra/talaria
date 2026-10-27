//! Masked diff lines for the section that is about to be saved.

use super::{DiffLine, ModelRolesState};

pub fn review_lines(state: &ModelRolesState) -> Vec<DiffLine> {
    state.diff_lines(state.section)
}
