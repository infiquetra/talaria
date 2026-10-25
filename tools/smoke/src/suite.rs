//! Which suite runs: the first suite, or the phase 2a suite of issue #229.
//!
//! Stage 0 stub: every public item panics `not implemented`.

use std::str::FromStr;

/// The suite the `run` subcommand executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suite {
    /// The first suite (the default).
    Phase1,
    /// The phase 2a suite: the eight units of issues #221 to #228.
    Phase2a,
}

impl FromStr for Suite {
    type Err = String;

    /// `phase1` or `phase2a`, else `unknown suite '{s}'; use phase1 or phase2a`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let _ = s;
        unimplemented!("not implemented")
    }
}

impl Suite {
    /// The default `--deadline-secs`: 300 for the first suite, 1800 for phase 2a.
    /// Stage 0 stub.
    pub fn default_deadline_secs(self) -> u64 {
        unimplemented!("not implemented")
    }
}
