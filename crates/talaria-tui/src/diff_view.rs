//! Diff rows for a tool cell. Issue 221.

use ratatui::text::Line;
use talaria_core::diff::DiffModel;

use crate::tool_cell::ToolTheme;

/// How to draw one diff.
pub struct DiffRenderOptions {
    /// Columns available for the whole row, including the indent.
    pub width: u16,
    /// Spaces before the line number.
    pub indent: u16,
    /// Most diff rows to draw. Absent means every row.
    pub max_rows: Option<usize>,
    /// Colours and weight.
    pub theme: ToolTheme,
    /// When false, colour is removed and bold and dim stay.
    pub color: bool,
}

/// Draw the diff. The second value is how many diff rows were not drawn.
///
/// Issue 221. The model is not drawn yet.
#[must_use]
pub fn diff_lines(model: &DiffModel, options: &DiffRenderOptions) -> (Vec<Line<'static>>, usize) {
    let _ = (model, options);
    todo!()
}
