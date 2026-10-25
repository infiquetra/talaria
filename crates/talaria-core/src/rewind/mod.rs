//! Checkpoint rewind and the session diff. Pure: no clock, no network, no logging.
//!
//! Issue 233. Checkpoint rewind and diff.

pub mod checkpoints;
pub mod command;
pub mod esc;
pub mod state;
pub mod text;
pub mod turns;

pub use checkpoints::{
    Checkpoint, CheckpointList, CheckpointMatch, NoMatch, checkpoint_for_turn, clock_text,
    parse_checkpoint_time,
};
pub use command::{
    DiffCommand, RollbackCommand, is_checkpoint_target, parse_diff, parse_rollback, parse_undo,
};
pub use esc::{DOUBLE_ESC_WINDOW_MS, DoubleEsc};
pub use state::{
    Call, CallFailure, CallKind, DiffSource, LineStyle, MutateOutcome, Reply, RewindEffect,
    RewindInput, RewindKey, RewindScreen, RewindState, RewindView, SessionRead, ViewLine, ViewRow,
    update,
};
pub use text::{
    DiffDoc, RestoreOutcome, SessionDiffOutcome, checkpoint_diff_doc, failure_text,
    mutate_refusal_text, no_match_text, python_line_count, read_refusal_text, relative_age,
    restore_refusal_text, restore_summary, session_diff_doc,
};
pub use turns::{RewindTurn, is_active_row, lossless_text, rewind_turns};
