//! `talaria --bg`. Step 0 exits 0 and writes nothing.

use std::time::Instant;

use crate::args::Cli;

/// Run a background launch. Step 0 does not start one.
pub fn run_bg(cli: &Cli, started: Instant) -> u8 {
    let _ = (cli, started);
    0
}
