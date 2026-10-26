//! The subagent dock and the `/agents` panel. Issue #242.

pub mod dock;
pub mod panel;

pub use dock::{SubagentDock, SubagentStyle};
pub use panel::{AgentsPanel, PanelHeader, PanelMode, TailView, batch_block_lines, format_elapsed};
