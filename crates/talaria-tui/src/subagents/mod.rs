//! The subagent dock and the `/agents` panel. Issue #242.

pub mod dock;
mod labels;
pub mod panel;
mod signal;

pub use dock::{SubagentDock, SubagentStyle};
pub(crate) use labels::compact_goal;
pub use panel::{AgentsPanel, PanelHeader, PanelMode, TailView, batch_block_lines, format_elapsed};
pub use signal::AgentClock;
