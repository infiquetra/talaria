//! Vim editing state. The command tables are not applied yet.

use unicode_segmentation::UnicodeSegmentation;

/// One vim mode. Off is composer state, not a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    /// Motions and operators.
    Normal,
    /// Today's composer, plus Esc leaving this mode.
    Insert,
    /// A characterwise selection.
    Visual,
    /// A linewise selection.
    VisualLine,
}

/// Pending command state while vim is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VimState {
    mode: VimMode,
}

impl VimState {
    /// The mode now showing.
    #[must_use]
    pub fn mode(&self) -> VimMode {
        self.mode
    }

    /// Digits and operator letters waiting for the next key, such as `d2`.
    #[must_use]
    pub fn pending_label(&self) -> Option<String> {
        None
    }
}

/// Byte offset where cluster `n` starts. Past the end, the text length.
#[must_use]
pub fn byte_of_cluster(text: &str, n: usize) -> usize {
    text.grapheme_indices(true)
        .nth(n)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

pub(crate) fn editing_unavailable() -> ! {
    todo!("vim editing is not implemented");
}
