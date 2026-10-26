//! The subagent dock: the live region above the composer. Issue #242.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use talaria_core::subagents::{Spawning, SubagentCounts, SubagentTree};

use crate::live::LiveSection;

/// One style per row state. `plain()` keeps bold and dim, drops colour.
pub struct SubagentStyle {
    /// Reserved for the per-state styles. The stub holds nothing yet.
    _reserved: (),
}

impl SubagentStyle {
    #[must_use]
    pub fn plain() -> Self {
        todo!("#242")
    }
}

impl Default for SubagentStyle {
    fn default() -> Self {
        todo!("#242")
    }
}

/// The dock: the header plus the live rows that fit `max_rows`.
pub struct SubagentDock<'a> {
    /// Reserved for the tree, counts, labels, spawning, style and budgets.
    _reserved: &'a (),
}

impl<'a> SubagentDock<'a> {
    /// Draw this tree with these counts, for a terminal of `terminal_rows`
    /// rows. `open_hint` is the `agents_panel` key label, if bound.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _tree: &'a SubagentTree,
        _counts: SubagentCounts,
        _key_label: Option<&'a str>,
        _spawning: Spawning,
        _open_hint: Option<&'a str>,
        _style: &'a SubagentStyle,
        _max_rows: u16,
        _terminal_rows: u16,
    ) -> Self {
        todo!("#242")
    }

    /// The dock's lines at this width, header first. Empty while hidden.
    #[must_use]
    pub fn lines(&self, _width: u16) -> Vec<Line<'static>> {
        todo!("#242")
    }
}

impl LiveSection for SubagentDock<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.lines(area.width);
        for (offset, row) in (0..area.height).zip(rows.iter()) {
            buf.set_line(area.x, area.y + offset, row, area.width);
        }
    }
}
