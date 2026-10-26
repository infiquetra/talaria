//! The full-screen `/agents` panel and the scrollback batch block. Issue #242.

use ratatui::text::Line;
use talaria_app::subagents::SubagentAction;
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::{Clock, FinishedBatch, Spawning, SubagentTree};

use crate::keys::Action;

/// What the panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelMode {
    List,
    Detail,
    Tail,
    Steer,
    ConfirmPause,
}

/// The panel's first line: the session title, spawning, and the key label
/// when the panel shows a background session.
pub struct PanelHeader {
    pub title: String,
    pub spawning: Spawning,
    pub key_label: Option<String>,
}

/// The tail view's input: the latest text, whether the gateway cut it, and
/// whether a call has ever failed.
pub struct TailView {
    pub text: Option<String>,
    pub truncated: bool,
    pub ever_failed: bool,
}

/// The panel: key, mode, cursor, tail scroll, steer draft, flash, follow flag.
pub struct AgentsPanel {
    /// Reserved for the panel state. The stub holds nothing yet.
    _reserved: (),
}

impl AgentsPanel {
    #[must_use]
    pub fn new(_key: RowKey) -> AgentsPanel {
        todo!("#242")
    }

    /// Feed one key-map action. Returns the hub control to run, if any.
    pub fn handle(&mut self, _action: Action, _tree: &SubagentTree) -> Option<SubagentAction> {
        todo!("#242")
    }

    /// Type into the steer box, or answer the pause confirmation.
    pub fn type_char(&mut self, _c: char) {
        todo!("#242")
    }

    /// Show this outcome text on the flash line until the next key.
    pub fn set_flash(&mut self, _text: String) {
        todo!("#242")
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        _tree: &SubagentTree,
        _header: &PanelHeader,
        _tail: Option<&TailView>,
        _width: u16,
        _height: u16,
        _now: Clock,
    ) -> Vec<Line<'static>> {
        todo!("#242")
    }
}

/// The scrollback block for one finished batch.
#[must_use]
pub fn batch_block_lines(
    _batch: &FinishedBatch,
    _width: u16,
    _style: &super::SubagentStyle,
) -> Vec<Line<'static>> {
    todo!("#242")
}

/// `0s` to `59s`, `1m 05s` to `59m 59s`, `1h 02m` and above.
#[must_use]
pub fn format_elapsed(_seconds: f64) -> String {
    todo!("#242")
}
