//! Approval, clarification and masked prompt cards: view state only.

pub mod approval;
pub mod clarify;
pub mod masked;
pub mod stack;

use std::fmt;

use talaria_core::session::PromptKey;

pub use approval::display_choices;
pub use clarify::multi_select_answer;
pub use masked::{MaskedValue, MaskedView};
pub use stack::{PromptSection, PromptStack};

/// What a key press asks the application to do.
pub enum PromptIntent {
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
        request_id: String,
        value: MaskedValue,
    },
}

impl fmt::Debug for PromptIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!("prompts")
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
