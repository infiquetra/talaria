//! The headless entry point: print one answer and return its exit code.

use std::time::Instant;

use crate::args::Cli;

/// Run one headless turn, write its output, and return the exit code.
pub fn run_print(cli: &Cli, started: Instant) -> u8 {
    let _ = (cli, started);
    todo!("#235")
}
