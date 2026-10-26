//! Draw the fleet overlay. The stub leaves the buffer untouched.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use talaria_core::fleet::RowState;
use talaria_core::fleet_view::FleetViewState;
use talaria_core::session::SessionState;

use crate::prompts::PromptStack;

/// The peeked session, when the panel has one to draw.
pub struct PeekPanel<'a> {
    pub session: &'a SessionState,
    pub stack: Option<&'a PromptStack>,
    pub now: f64,
}

/// Colours for one fleet frame. Stale rows use the dim modifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetTheme {
    pub needs_you: Style,
    pub working: Style,
    pub failed: Style,
    pub done: Style,
    pub idle: Style,
    pub dim: Style,
    pub header: Style,
    pub footer: Style,
}

impl Default for FleetTheme {
    fn default() -> Self {
        Self {
            needs_you: Style::default(),
            working: Style::default(),
            failed: Style::default(),
            done: Style::default(),
            idle: Style::default(),
            dim: Style::default().add_modifier(Modifier::DIM),
            header: Style::default(),
            footer: Style::default(),
        }
    }
}

impl FleetTheme {
    /// The style for a row in `state`. Stale rows are dim as well as coloured.
    #[must_use]
    pub fn row_style(&self, state: RowState, stale: bool) -> Style {
        let base = match state {
            RowState::NeedsYou => self.needs_you,
            RowState::Working => self.working,
            RowState::Failed => self.failed,
            RowState::Done => self.done,
            RowState::Idle => self.idle,
        };
        if stale { base.patch(self.dim) } else { base }
    }
}

/// The fleet overlay.
pub struct FleetView;

impl FleetView {
    /// Draw into `buf`. This stub draws nothing, so a screen test sees an empty frame.
    pub fn draw(
        state: &FleetViewState,
        peek: Option<PeekPanel<'_>>,
        area: Rect,
        buf: &mut Buffer,
        theme: &FleetTheme,
    ) {
        let _ = (state, peek, area, buf, theme);
    }
}
