//! Pure session reducer. One [`SessionState`] turns Hermes events into cells and effects.
//!
//! Payload fields are read from the raw `params` object on [`Event::Session`].
//! The function [`update`] does no input or output and reads no clock.

pub mod cell;
pub mod effect;
pub mod event;
pub mod ids;
pub(crate) mod injected;
pub mod notices;
pub mod prompts;
pub mod reducer;
pub mod snapshot;
pub mod state;
pub mod text;
pub mod tool_view;
pub mod usage;
pub mod usage_limit;

pub use cell::{
    AnswerRefusal, AssistantCell, Cell, NoticeCell, NoticeKind, PromptCell, PromptKind,
    PromptState, ReasoningCell, ReplyMiss, Settlement, ToolCell, ToolProgress, ToolStatus,
    UserCell, assistant_reply, is_answerable, recovered_limit_seconds,
};
pub use effect::Effect;
pub use event::Event;
pub use ids::{CellId, PromptKey, SessionId};
pub use notices::{NoticeRow, NoticeStore};
pub use reducer::update;
pub use state::{
    ConnectionCard, ConnectionChoice, ReplayCursor, SessionState, StatusNote, TodoState,
    TurnOutcome, TurnState, Usage,
};
pub use tool_view::{
    AskedQuestion, OutcomeView, OutputView, ToolView, format_duration, format_progress,
    shorten_middle, tool_view,
};
pub use usage_limit::{UsageLimitCard, usage_limit_card, usage_limit_model_payload};
