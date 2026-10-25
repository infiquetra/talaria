//! Presenting Hermes's own command replies. Stubs.
//!
//! Behavior contract: issue 236.

use crate::commands::{CommandCatalog, CommandOutcome};

/// The commands whose replies render as cells, by canonical name.
pub const PRESENTED_COMMANDS: &[&str] = &[
    "plan",
    "goal",
    "subgoal",
    "loop",
    "heartbeat",
    "yolo",
    "fast",
    "reasoning",
    "busy",
    "verbose",
    "footer",
    "approvals",
    "personality",
    "codex-runtime",
];

/// The note after the first submitted `/plan` of a session.
pub const PLAN_MODE_NOTE: &str = "Plan mode is an instruction to the agent, not a sandbox: it \
    should only write a plan under .hermes/plans/ in the session folder, but nothing stops a \
    tool call, and approvals still apply.";

/// A presented command reply: the typed line and its cleaned body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HermesCommandCell {
    /// The canonical command name.
    pub name: String,
    /// The line exactly as typed.
    pub typed: String,
    /// The cleaned reply body.
    pub body: String,
    /// True when the body opens with a warning glyph.
    pub warning: bool,
}

/// What a command reply becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Presentation {
    /// A ran command: one cell.
    Output(HermesCommandCell),
    /// A queued turn: an optional cell plus the submission.
    Submit {
        /// The cell, when the notice is non-blank.
        cell: Option<HermesCommandCell>,
        /// The turn text.
        text: String,
        /// The display line.
        display: String,
    },
}

/// The canonical command name of a typed line, if it is one.
pub fn canonical_name(typed: &str, catalog: Option<&CommandCatalog>) -> Option<String> {
    let _ = (typed, catalog);
    todo!("#236")
}

/// What a command reply becomes, if this unit presents it.
pub fn present_command(
    canonical: &str,
    typed: &str,
    outcome: &CommandOutcome,
) -> Option<Presentation> {
    let _ = (canonical, typed, outcome);
    todo!("#236")
}

/// Clean a command reply for Markdown rendering.
pub fn clean_command_text(text: &str) -> String {
    let _ = text;
    todo!("#236")
}
