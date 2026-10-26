//! The subagent hub: one tracker per attached session. Issue #242.
//!
//! The hub owns the wire: it sends the six allowed calls, feeds answers and
//! `subagent.*` notifications into the trees, and reports visible changes.
//! Drawing stays in `talaria-tui`.

pub mod hub;

pub use hub::{
    ControlOutcome, SubagentAction, SubagentHub, SubagentMsg, SubagentTailView, SubagentsConfig,
    outcome_text,
};
pub use talaria_core::subagents::{Spawning, SubagentCounts};

/// Where the hub sends its messages: the application event channel.
pub type AppSender = tokio::sync::mpsc::UnboundedSender<crate::app::AppMessage>;
