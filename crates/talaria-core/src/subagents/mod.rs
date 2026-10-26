//! The subagent tree: rows learnt from `subagent.*` events and `subagent.list`.
//!
//! Pure domain state for issue #242. No I/O, no clock, no terminal library, no
//! `hermes-client` reach this module; times arrive as [`Clock`] and text leaves
//! unsanitized for the screen to clean.

pub mod batch;
pub mod command;
pub mod event;
pub mod row;
pub mod tree;

pub use batch::FinishedBatch;
pub use command::{AgentsArg, parse_agents_arg};
pub use event::{SubagentEvent, SubagentEventKind};
pub use row::{Activity, ActivityKind, FailKind, RowState, SubagentRow};
pub use tree::{Applied, Caps, Clock, Spawning, SubagentCounts, SubagentTree, TreeRow};

#[cfg(test)]
mod tests;
