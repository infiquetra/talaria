//! Which suite runs: the first suite, the phase 2a suite of issue #229, or the phase 2b
//! suite of issue #238.
//!
//! Stage 0 stub: the phase 2b additions panic `not implemented`.

use std::str::FromStr;

/// The suite the `run` subcommand executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suite {
    /// The first suite (the default).
    Phase1,
    /// The phase 2a suite: the eight units of issues #221 to #228.
    Phase2a,
    /// The phase 2b suite: the eight units of issues #230 to #237.
    Phase2b,
}

impl FromStr for Suite {
    type Err = String;

    /// `phase1`, `phase2a` or `phase2b`, else `unknown suite '{s}'; use phase1 or phase2a`.
    /// The parse text stays as it is: the phase 2a suite pins it exactly.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "phase1" => Ok(Suite::Phase1),
            "phase2a" => Ok(Suite::Phase2a),
            _ => Err(format!("unknown suite '{s}'; use phase1 or phase2a")),
        }
    }
}

impl Suite {
    /// The default `--deadline-secs`: 300 for the first suite, 1800 for phase 2a, 1500 for
    /// phase 2b. Stage 0 stub.
    pub fn default_deadline_secs(self) -> u64 {
        match self {
            Suite::Phase1 => 300,
            Suite::Phase2a => 1800,
            Suite::Phase2b => unimplemented!("not implemented"),
        }
    }
}
