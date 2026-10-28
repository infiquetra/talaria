//! Which suite runs: the first suite, the phase 2a suite of issue #229, the phase 2b
//! suite of issue #238, or the phase 3 suite of issue #248.

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
    /// The phase 3 suite of issue #248: background sessions, the fleet and attach.
    Phase3,
    /// The configuration screens suite.
    Phase4a,
    /// The capabilities and operations screens suite.
    Phase4b,
    /// The workflow smoke suite.
    Phase5,
}

impl FromStr for Suite {
    type Err = String;

    /// `phase1`, `phase2a`, `phase2b` or `phase3`, else
    /// `unknown suite '{s}'; use phase1 or phase2a`.
    /// The parse text stays as it is: the phase 2a suite pins it exactly.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "phase1" => Ok(Suite::Phase1),
            "phase2a" => Ok(Suite::Phase2a),
            "phase2b" => Ok(Suite::Phase2b),
            "phase3" => Ok(Suite::Phase3),
            "phase4a" => Ok(Suite::Phase4a),
            "phase4b" => Ok(Suite::Phase4b),
            _ => Err(format!("unknown suite '{s}'; use phase1 or phase2a")),
        }
    }
}

impl Suite {
    /// The default `--deadline-secs`: 300 for the first suite, 1800 for phase 2a, 1500 for
    /// phase 2b and 1200 for phase 3, above the sum of its step limits.
    pub fn default_deadline_secs(self) -> u64 {
        match self {
            Suite::Phase1 => 300,
            Suite::Phase2a => 1800,
            Suite::Phase2b => 1500,
            Suite::Phase3 => 1200,
            Suite::Phase4a => 900,
            Suite::Phase4b => 900,
            Suite::Phase5 => panic!("not implemented"),
        }
    }
}
