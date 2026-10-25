//! The automation section and presented command cells. Stubs.
//!
//! Behavior contract: issue 236.

use ratatui::style::Style;
use ratatui::text::Line;
use talaria_core::automation::{AutomationState, HermesCommandCell};

use crate::live::LiveSection;

/// The automation section's colours.
#[derive(Clone, Debug, Default)]
pub struct AutomationTheme {
    /// The accent colour.
    pub accent: Style,
    /// The warning colour.
    pub warning: Style,
    /// The dim modifier.
    pub dim: Style,
}

/// The standing automation rows above the composer.
pub struct AutomationSection<'a> {
    /// The state to draw.
    pub state: &'a AutomationState,
    /// The clock in epoch milliseconds.
    pub now_ms: u64,
    /// The colours to draw with.
    pub theme: &'a AutomationTheme,
}

impl LiveSection for AutomationSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = width;
        todo!("#236")
    }

    fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let _ = (area, buf);
        todo!("#236")
    }
}

/// The lines of a presented command cell at `width`.
pub fn hermes_command_lines(
    cell: &HermesCommandCell,
    width: u16,
    theme: &AutomationTheme,
) -> Vec<Line<'static>> {
    let _ = (cell, width, theme);
    todo!("#236")
}
