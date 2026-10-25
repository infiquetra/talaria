//! Inputs to [`super::update`]. Local variants are built by the application, never by the reducer.

use serde_json::Value;

use super::{NoticeKind, PromptKey, SessionId};

/// One reducer input.
///
/// `Event::Session.raw` is the whole `params` object. Envelope fields come from
/// `frame`. Every payload field comes from `raw["payload"]`.
///
/// The decoded frame stays by value. Boxing it would change this stub's public shape.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A decoded session notification plus the raw params it was decoded from.
    Session {
        /// Outer session id. Scope uses this value, not the copy inside `raw`.
        session_id: SessionId,
        /// Decoded envelope. `type_name()` is the event type.
        frame: hermes_contract::events::EventFrame,
        /// The `params` object given to `EventFrame::from_params`.
        raw: Value,
    },
    /// A new gateway connection. Clears the set of noted unknown event types.
    ConnectionOpened {
        /// Connection generation from the channel.
        generation: u64,
    },
    /// The person submitted text. Blank text does nothing.
    Submit {
        /// Prompt text.
        text: String,
        /// Client-chosen id. Not de-duplicated here.
        submission_id: String,
    },
    /// The submit will start no turn. Drops the queue entry. The user cell, if placed, stays.
    SubmitFailed {
        /// Id of the entry to drop.
        submission_id: String,
        /// Plain error text from the application.
        reason: String,
    },
    /// Ask Hermes to stop the running turn. Seals nothing.
    Interrupt,
    /// Hermes refused the interrupt. Clears the latch.
    InterruptFailed,
    /// Answer an open approval. `choice` must be one of the offered choices.
    AnswerApproval {
        /// Prompt key, generation included.
        key: PromptKey,
        /// Choice text.
        choice: String,
    },
    /// Answer an open clarification. The answer is sent as given, including empty.
    AnswerClarify {
        /// Prompt key, generation included.
        key: PromptKey,
        /// Answer text.
        answer: String,
    },
    /// The respond call failed. `Answering` returns to `Open`.
    AnswerFailed {
        /// Prompt key.
        key: PromptKey,
    },
    /// Hermes replied `already_resolved`.
    AnswerAlreadyResolved {
        /// Prompt key.
        key: PromptKey,
    },
    /// A slash command sent `text` to the model and shows `display` in the user cell.
    ///
    /// Same waiting and placement rules as [`Event::Submit`]. Issue #223.
    SubmitCommand {
        /// Model-facing text. `prompt.submit` carries this.
        text: String,
        /// Text of the user cell.
        display: String,
        /// Client-chosen id. Not de-duplicated here.
        submission_id: String,
    },
    /// Append one final notice cell. Issue #223.
    Notice {
        /// System for command output, Error for a refusal.
        kind: NoticeKind,
        /// Plain text of the notice.
        text: String,
    },
}
