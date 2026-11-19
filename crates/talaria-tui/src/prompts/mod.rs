//! Approval, clarification and masked prompt cards: view state only.

pub mod approval;
pub mod clarify;
pub mod masked;
pub mod stack;

use std::fmt;

pub use talaria_core::session::ConnectionChoice;
use talaria_core::session::PromptKey;

pub use approval::display_choices;
pub use clarify::multi_select_answer;
pub use masked::{MaskedValue, MaskedView};
pub use stack::{PromptSection, PromptStack};

/// A live masked card, bound to the service and request that opened it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveMaskedTarget {
    pub token: String,
    pub session_id: Option<String>,
    pub generation: u64,
    pub instance_id: u64,
    pub service_id: u64,
}

/// A masked answer is either a live card or a recovered prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaskedTarget {
    Live(LiveMaskedTarget),
    Recovered { request_id: String },
}

/// What a key press asks the application to do.
pub enum PromptIntent {
    Connect {
        session_id: String,
        op_id: String,
        choice: ConnectionChoice,
    },
    Approve {
        key: PromptKey,
        choice: String,
        note: Option<String>,
    },
    Clarify {
        key: PromptKey,
        answer: String,
    },
    /// An empty value means declined.
    Masked {
        target: MaskedTarget,
        value: MaskedValue,
    },
}

impl fmt::Debug for PromptIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect { choice, .. } => f
                .debug_struct("Connect")
                .field("choice", choice)
                .finish_non_exhaustive(),
            Self::Approve { choice, note, .. } => f
                .debug_struct("Approve")
                .field("choice_chars", &choice.chars().count())
                .field("note_chars", &note.as_ref().map(|s| s.chars().count()))
                .finish(),
            Self::Clarify { answer, .. } => f
                .debug_struct("Clarify")
                .field("answer_chars", &answer.chars().count())
                .finish(),
            Self::Masked { value, .. } => f
                .debug_struct("Masked")
                .field("value_chars", &value.len_chars())
                .finish(),
        }
    }
}

/// The result of routing one key or paste to the card stack.
pub enum KeyOutcome {
    Consumed {
        intent: Option<PromptIntent>,
        interrupt_requested: bool,
        notice: Option<String>,
    },
    NotHandled,
}
