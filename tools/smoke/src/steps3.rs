//! The phase 3 smoke suite of issue #248.
//!
//! Twelve steps, in order. The body panics until the steps are written. The limit table
//! is the one the deadline test reads. Key names are the ones a later live check sends.

use std::sync::atomic::AtomicBool;

use crate::{Clock, SmokeConfig, SmokeReport, Terminal};

/// The steps, in the order the suite runs them.
pub const STEP_ORDER: [&str; 12] = [
    "fleet-quiet",
    "bg-launch",
    "worktree-launch",
    "fleet-list",
    "attach",
    "startup",
    "plain-answer",
    "local-commands",
    "fleet-view",
    "open-in-pane",
    "quit",
    "needs-you",
];

/// Each step's own limit, in seconds. The sum is 1095.
pub const STEP_LIMITS: &[(&str, u64)] = &[
    ("fleet-quiet", 30),
    ("bg-launch", 60),
    ("worktree-launch", 90),
    ("fleet-list", 180),
    ("attach", 60),
    ("startup", 30),
    ("plain-answer", 90),
    ("local-commands", 30),
    ("fleet-view", 30),
    ("open-in-pane", 45),
    ("quit", 30),
    ("needs-you", 420),
];

/// Jump to the session that needs an answer.
pub const KEY_JUMP: &str = "alt+a";
/// Open the subagent panel. The suite records the name; it does not send it.
pub const KEY_SUBAGENTS: &str = "alt+s";
/// Push. The suite records the name; it does not send it.
pub const KEY_PUSH: &str = "shift+p";
/// Stop. The suite records the name; it does not send it.
pub const KEY_STOP: &str = "shift+x";
/// The submit fallback a Herdr build may refuse. The suite records the name; it does not send it.
pub const KEY_SUBMIT_FALLBACK: &str = "ctrl+s";

/// Runs the twelve checks, stopping at the first failure.
pub fn run_smoke_3(
    term: &dyn Terminal,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    stop: &AtomicBool,
) -> SmokeReport {
    let _ = (term, clock, cfg, stop);
    panic!("not implemented");
}
