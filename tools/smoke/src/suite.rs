//! Which suite runs: the first suite, or the phase 2a suite of issue #229.

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
        match s {
            "phase1" => Ok(Suite::Phase1),
            "phase2a" => Ok(Suite::Phase2a),
            _ => Err(format!("unknown suite '{s}'; use phase1 or phase2a")),
        }
    }
}

impl Suite {
    /// The default `--deadline-secs`: 300 for the first suite, 1800 for phase 2a.
    pub fn default_deadline_secs(self) -> u64 {
        match self {
            Suite::Phase1 => 300,
            Suite::Phase2a => 1800,
        }
    }
}
