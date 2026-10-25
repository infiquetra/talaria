//! Standing automation state: goals, loops, heartbeats and session modes.
//!
//! Pure: no clock, no input, no output. Behavior contract: issue 236.

mod fold;
mod present;
mod rows;
mod state;

pub use fold::{FoldKind, FoldedPrompt, fold_user_text};
pub use present::{
    HermesCommandCell, PLAN_MODE_NOTE, PRESENTED_COMMANDS, Presentation, canonical_name,
    clean_command_text, present_command,
};
pub use rows::{AutomationRow, Tone, countdown, format_interval, next_tick, rows};
pub use state::{
    AutomationEffect, AutomationEvent, AutomationState, CommandResult, ControlStatus, ControlView,
    DecodeError, FastMode, GoalView, HeartbeatView, LoopView, ModeKey, ModeValue, ReadFailure,
    WaitView, update,
};
