//! The picker's state machine: modes, inputs and effects (#231).

use super::export::ExportFormat;
use super::rows::{DirectoryRow, RowKey};

/// How one profile's lists are doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileLoad {
    Pending,
    Loaded,
    Failed { reason: String },
}

/// What the picker is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerMode {
    List,
    Search,
    Rename(LineEditor),
    Export(ExportDialog),
    Waiting(WaitingFor),
}

/// The rename editor: text, caret and its held limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineEditor {
    _held: (),
}

/// The export dialog: its row, format and redaction state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportDialog {
    pub key: RowKey,
    pub format: ExportFormat,
    pub redact: bool,
}

/// What a waiting picker waits for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitingFor {
    Rename(RowKey),
    Branch,
    Export(RowKey),
    Open(RowKey),
}

/// The profile filter: all profiles or one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileFilter {
    All,
    One(String),
}

/// The picker's state. The builder owns the fields; the red-run tests only
/// need the type and its constructor.
pub struct PickerState {
    _held: (),
}

impl PickerState {
    /// A picker that has not opened yet.
    pub fn new() -> Self {
        Self { _held: () }
    }

    /// The rows under the filter and query, ordered.
    pub fn visible_rows(&self) -> Vec<&DirectoryRow> {
        unimplemented!("#231")
    }

    /// The row under the cursor, if any.
    pub fn selected(&self) -> Option<&DirectoryRow> {
        unimplemented!("#231")
    }

    /// The current mode.
    pub fn mode(&self) -> &PickerMode {
        unimplemented!("#231")
    }

    /// The notice line, if any.
    pub fn notice(&self) -> Option<&str> {
        unimplemented!("#231")
    }

    /// One failed profile and scope pair: profile, origin, reason.
    pub fn unavailable(&self) -> Vec<(String, super::rows::RowOrigin, String)> {
        unimplemented!("#231")
    }

    /// The banner lines, at most two: the scope note, then a side failure.
    pub fn banners(&self) -> Vec<String> {
        unimplemented!("#231")
    }
}

impl Default for PickerState {
    fn default() -> Self {
        Self::new()
    }
}

/// One picker command from a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerAction {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Open,
    Rename,
    Branch,
    Export,
    Search,
    ProfileNext,
    ProfilePrev,
    Refresh,
    Close,
    FormatMarkdown,
    FormatJson,
    FormatHtml,
    ToggleRedact,
}

/// One input to the picker.
#[derive(Clone, Debug, PartialEq)]
pub enum PickerInput {
    Opened {
        profiles: Vec<String>,
        foreground: Option<RowKey>,
        scope_note: Option<String>,
        cached: Vec<DirectoryRow>,
        generation: u64,
    },
    Listed {
        generation: u64,
        profile: String,
        origin: super::rows::RowOrigin,
        rows: Vec<DirectoryRow>,
        skipped: usize,
    },
    ListFailed {
        generation: u64,
        profile: String,
        origin: super::rows::RowOrigin,
        reason: String,
    },
    SideFailed {
        reason: String,
    },
    Action(PickerAction),
    Text(char),
    Paste(String),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Enter,
    Escape,
    CtrlC,
    Renamed {
        key: RowKey,
        title: String,
    },
    Notice(String),
    Busy(bool),
    Resize {
        cols: u16,
        rows: u16,
    },
}

/// What the application does after one input.
#[derive(Clone, Debug, PartialEq)]
pub enum PickerEffect {
    List {
        generation: u64,
    },
    Open(RowKey),
    Rename {
        key: RowKey,
        title: String,
    },
    Branch {
        title: Option<String>,
    },
    Export {
        key: RowKey,
        format: ExportFormat,
        redact: bool,
    },
    Close,
}

/// Apply one input. No input or output: the effects say what to do.
pub fn update(_state: &mut PickerState, _input: PickerInput, _page: usize) -> Vec<PickerEffect> {
    unimplemented!("#231")
}
