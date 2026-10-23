//! Pure session reducer. One [`SessionState`] turns Hermes events into cells and effects.
//!
//! Payload fields are read from the raw `params` object on [`Event::Session`] (parent #192 C-2).
//! The function [`update`] does no input or output and reads no clock.

pub mod cell;
pub mod effect;
pub mod event;
pub mod ids;
pub mod prompts;
pub mod reducer;
pub mod snapshot;
pub mod state;
pub mod text;
pub mod usage;

pub use cell::{
    AnswerRefusal, AssistantCell, Cell, NoticeCell, NoticeKind, PromptCell, PromptKind,
    PromptState, ReasoningCell, Settlement, ToolCell, ToolProgress, ToolStatus, UserCell,
};
pub use effect::Effect;
pub use event::Event;
pub use ids::{CellId, PromptKey, SessionId};
pub use reducer::update;
pub use state::{ReplayCursor, SessionState, StatusNote, TodoState, TurnOutcome, TurnState, Usage};
