//! Rewind overlay state. Tests drive it through `update`. Issue 233.

use serde_json::Value;

use crate::session::NoticeKind;

use super::checkpoints::CheckpointList;
use super::text::{DiffDoc, RestoreOutcome};

pub use super::text::{CallFailure, CallKind};

/// One gateway call the application performs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// Read the session.
    ReadSession,
    /// List checkpoints.
    ListCheckpoints,
    /// Diff one checkpoint.
    CheckpointDiff { hash: String },
    /// Diff since the oldest kept checkpoint.
    SessionDiff,
    /// Rewind the conversation.
    Mutate {
        row_id: i64,
        expected_revision: i64,
        expected_generation: i64,
    },
    /// Restore files.
    Restore {
        hash: String,
        file_path: Option<String>,
        all: bool,
    },
}

/// A fresh session read.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRead {
    /// The raw resume result.
    pub raw: Value,
    /// `revision`.
    pub revision: i64,
    /// `execution_generation`.
    pub execution_generation: i64,
    /// A turn is running.
    pub running: bool,
    /// Pending entries whose status is queued, started, or unknown.
    pub pending: usize,
    /// Raw message rows.
    pub messages: Vec<Value>,
}

/// A committed rewind.
#[derive(Debug, Clone, PartialEq)]
pub struct MutateOutcome {
    /// New revision, when the reply carried one.
    pub revision: Option<i64>,
    /// Rows deactivated, the target included.
    pub rewound_count: u64,
    /// `target_message.content`, when present.
    pub target_content: Option<Value>,
}

/// A reply for one ticket.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// A session read.
    SessionRead(SessionRead),
    /// A checkpoint list.
    Checkpoints(CheckpointList),
    /// A diff document.
    DiffLoaded(DiffDoc),
    /// A committed rewind.
    Mutated(MutateOutcome),
    /// A restore result.
    Restored(RestoreOutcome),
    /// The call failed.
    Failed(CallFailure),
}

/// A key the overlay understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Esc,
    Close,
    C,
    F,
    D,
    R,
}

/// Which screen is showing. `Working` from a typed command is not an open overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindScreen {
    Closed,
    Loading,
    Turns,
    Actions,
    Confirm,
    Checkpoints,
    CheckpointConfirm,
    Diff,
    Working,
}

/// Where a diff was opened from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// `/diff`.
    Session { stat_only: bool },
    /// A checkpoint target.
    Checkpoint(String),
}

/// One input to [`update`].
#[derive(Debug, Clone, PartialEq)]
pub enum RewindInput {
    /// `/rewind`, `/checkpoint`, or Esc twice.
    Open { turn_running: bool },
    /// `/rollback` with no target.
    OpenCheckpoints,
    /// `/diff` or `/rollback diff`.
    OpenDiff(DiffSource),
    /// `/undo`.
    Undo { n: usize, turn_running: bool },
    /// `/retry`.
    Retry { turn_running: bool },
    /// `/rollback <target>`.
    TypedRestore {
        target: String,
        file_path: Option<String>,
        all: bool,
        turn_running: bool,
    },
    /// A key.
    Key(RewindKey),
    /// How many rows the renderer can show.
    Viewport(u16),
    /// A gateway reply.
    Reply { ticket: u64, reply: Reply },
    /// A `session.updated` frame.
    SessionUpdated {
        operation: String,
        revision: Option<i64>,
        rewound_count: Option<u64>,
    },
}

/// What the application should do after `update`.
#[derive(Debug, Clone, PartialEq)]
pub enum RewindEffect {
    /// Perform one call.
    Call { ticket: u64, call: Call },
    /// Replace the foreground session and commit `notice`.
    Rebuild {
        snapshot: Value,
        notice: Vec<String>,
    },
    /// Put the prompt back into an empty composer.
    ReturnPrompt { text: String, attachments: bool },
    /// Submit this text with a fresh submission id.
    Submit(String),
    /// Show a notice.
    Notice(NoticeKind, String),
}

/// How a body line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Normal,
    Dim,
    Warning,
    Accent,
}

/// One row of the turn or checkpoint list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewRow<'a> {
    /// The row is selected.
    pub mark: bool,
    /// `#3` or the checkpoint index.
    pub label: String,
    /// Relative age, padded by the renderer.
    pub age: String,
    /// `HH:MM`, or empty.
    pub clock: String,
    /// Preview or checkpoint message.
    pub text: &'a str,
    /// `files`, `·`, or empty.
    pub tail: &'static str,
}

/// One body line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    /// The line's text.
    pub text: String,
    /// How to draw it.
    pub style: LineStyle,
}

/// Everything the overlay draw reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindView<'a> {
    /// Current screen.
    pub screen: RewindScreen,
    /// Title line.
    pub title: String,
    /// Turn or checkpoint rows.
    pub rows: Vec<ViewRow<'a>>,
    /// Selected index.
    pub selected: usize,
    /// First row the viewport shows.
    pub first_visible: usize,
    /// Body lines.
    pub body: Vec<ViewLine>,
    /// Diff document, when the screen is a diff.
    pub diff: Option<&'a DiffDoc>,
    /// First diff line the viewport shows.
    pub diff_scroll: usize,
    /// Footer, the last row.
    pub footer: String,
}

/// Overlay state. The default is closed.
#[derive(Debug)]
pub struct RewindState {
    screen: RewindScreen,
}

impl Default for RewindState {
    fn default() -> Self {
        Self {
            screen: RewindScreen::Closed,
        }
    }
}

impl RewindState {
    /// An overlay is showing. A typed command's working step is not an overlay.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.screen != RewindScreen::Closed
    }

    /// The current screen.
    #[must_use]
    pub fn screen(&self) -> RewindScreen {
        self.screen
    }

    /// The view to draw at `now_ms`.
    #[must_use]
    pub fn view(&self, now_ms: u64) -> RewindView<'_> {
        let _ = now_ms;
        RewindView {
            screen: self.screen,
            title: String::new(),
            rows: Vec::new(),
            selected: 0,
            first_visible: 0,
            body: Vec::new(),
            diff: None,
            diff_scroll: 0,
            footer: String::new(),
        }
    }
}

/// Apply one input. `now_ms` is supplied by the application.
pub fn update(state: &mut RewindState, input: RewindInput, now_ms: u64) -> Vec<RewindEffect> {
    let _ = (state, input, now_ms);
    todo!("Issue 233. Checkpoint rewind and diff.")
}
