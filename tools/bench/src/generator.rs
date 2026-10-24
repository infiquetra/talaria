//! Writes the three replay files from recorded templates.

use std::path::Path;

use crate::budgets::Budgets;
use crate::error::BenchError;

/// Writes `empty.jsonl`, `reference-stream.jsonl` and `long-session.jsonl` into `out`.
pub fn generate(fixtures: &Path, out: &Path, budgets: &Budgets) -> Result<(), BenchError> {
    let _ = (fixtures, out, budgets);
    todo!("bench")
}
