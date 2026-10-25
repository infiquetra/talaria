//! Palette rows and the pure keystroke transition. Issue #223.

use std::collections::BTreeSet;
use std::time::Duration;

use super::catalog::{CommandCatalog, CommandSource};
use super::local::LocalCommandTable;
use super::state::CatalogStatus;

/// A `complete.slash` result.
pub type CompleteResult = hermes_contract::methods::legacy::complete_slash::Result;

/// What a row is. Only [`RowKind::Command`] can be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A selectable command.
    Command,
    /// A group title.
    Header,
    /// Usage hint while an argument is being typed.
    Hint,
    /// Loading, failure, or "no match".
    Status,
}

/// One line of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRow {
    /// Row class.
    pub kind: RowKind,
    /// Command name, or the header text.
    pub name: String,
    /// Description. Empty for a header.
    pub description: String,
    /// Provenance. `None` for a header, hint, or status row.
    pub source: Option<CommandSource>,
    /// The bare form was refused on this connection.
    pub refused: bool,
    /// The command takes an argument.
    pub takes_argument: bool,
    /// Group title, when the row is a command.
    pub group: Option<String>,
}

/// Command name, or the argument after a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteStage {
    /// Choosing a command.
    Command,
    /// Choosing or typing an argument. `replace_from_bytes` is a byte offset.
    Argument { replace_from_bytes: Option<usize> },
}

/// The palette the screen draws. Issue #223.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteState {
    /// The palette is on screen.
    pub open: bool,
    /// Command name or argument.
    pub stage: PaletteStage,
    /// Rows in display order, including headers.
    pub rows: Vec<PaletteRow>,
    /// Index of the selected row in `rows`.
    pub selected: usize,
    /// The person moved the selection.
    pub explicit_selection: bool,
    /// Id of the completion request in flight. Zero when none is.
    pub request_id: u64,
    /// Footer text, when it replaces the default hint.
    pub footer: Option<String>,
    /// Draft for which Esc closed the palette. `None` when it was not closed that way.
    pub closed_for: Option<String>,
    /// Selectable rows in browse mode.
    pub total: usize,
}

impl Default for PaletteState {
    fn default() -> Self {
        Self::closed()
    }
}

impl PaletteState {
    /// A closed palette.
    #[must_use]
    pub fn closed() -> Self {
        Self {
            open: false,
            stage: PaletteStage::Command,
            rows: Vec::new(),
            selected: 0,
            explicit_selection: false,
            request_id: 0,
            footer: None,
            closed_for: None,
            total: 0,
        }
    }
}

/// One thing that happened to the draft or a reply.
#[derive(Clone, Debug, PartialEq)]
pub enum PaletteInput {
    /// The draft or the caret changed. `caret` is a byte offset.
    DraftChanged { text: String, caret: usize },
    /// The catalogue store changed. Rows are derived again.
    CatalogChanged,
    /// A `complete.slash` reply for `request_id`.
    CompletionArrived {
        request_id: u64,
        result: CompleteResult,
    },
    /// That completion failed.
    CompletionFailed { request_id: u64, reason: String },
    /// Select the next row.
    Next,
    /// Select the previous row.
    Prev,
    /// Tab.
    Accept,
    /// Enter.
    Enter,
    /// Esc.
    Close,
}

/// What the application should do after one input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteEffect {
    /// Ask Hermes to complete `text` after `after` (60 ms).
    RequestCompletion {
        request_id: u64,
        text: String,
        after: Duration,
    },
    /// Replace the composer text.
    ReplaceDraft { text: String },
    /// Run this slash line.
    Run { line: String },
    /// Draw again.
    Redraw,
}

/// What the transition may read. It does not fetch.
pub struct PaletteContext<'a> {
    /// The latest catalogue, when one has arrived.
    pub catalog: Option<&'a CommandCatalog>,
    /// Whether that catalogue is loading, ready, or failed.
    pub catalog_status: &'a CatalogStatus,
    /// Local commands.
    pub local: &'a LocalCommandTable,
    /// Bare names refused on this connection.
    pub refused: &'a BTreeSet<String>,
}

/// Apply one input. Issue #223. The body is a stub.
#[must_use]
pub fn update(
    state: &mut PaletteState,
    input: PaletteInput,
    ctx: &PaletteContext<'_>,
) -> Vec<PaletteEffect> {
    let _ = (
        state,
        input,
        ctx.catalog,
        ctx.catalog_status,
        ctx.local,
        ctx.refused,
    );
    todo!()
}
