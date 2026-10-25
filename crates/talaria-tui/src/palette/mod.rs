//! The slash palette in the live region, under the composer. Issue #223.
//!
//! Closing it shrinks the live region. Nothing is written into scrollback.

mod render;

use talaria_core::commands::PaletteState;

pub use render::render_palette;

/// One frame of the slash palette.
pub struct PaletteSection {
    state: PaletteState,
    terminal_rows: u16,
}

impl PaletteSection {
    /// `terminal_rows` is the whole terminal, used only for the height cap.
    #[must_use]
    pub fn new(state: &PaletteState, terminal_rows: u16) -> Self {
        Self {
            state: state.clone(),
            terminal_rows,
        }
    }

    /// The palette this frame draws.
    #[must_use]
    pub fn state(&self) -> &PaletteState {
        &self.state
    }

    /// Terminal rows the height cap was given.
    #[must_use]
    pub fn terminal_rows(&self) -> u16 {
        self.terminal_rows
    }
}
