//! The table and the JSON report.

use crate::budgets::Row;

#[derive(Debug, Default)]
pub struct Report {
    pub rows: Vec<Row>,
    /// Every child process the run spawned.
    pub child_pids: Vec<u32>,
}

impl Report {
    pub fn render(&self) -> String {
        todo!("bench")
    }

    pub fn to_json(&self) -> String {
        todo!("bench")
    }
}
