//! Effects are data. The application sends them. The reducer does not.

use super::{AnswerRefusal, PromptKey, SessionId};

/// A follow-up the application may send. Comparing these to recorded outbound frames is the test.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `prompt.submit` with exactly `session_id`, `text` and `submission_id`.
    /// `unattended` is never set true (parent #192 C-7).
    SendPrompt {
        /// Target session.
        session_id: SessionId,
        /// Submitted text.
        text: String,
        /// Client-chosen id.
        submission_id: String,
    },
    /// `session.interrupt` with exactly `session_id` and `execution_generation`.
    Interrupt {
        /// Target session.
        session_id: SessionId,
        /// Generation of the running turn.
        execution_generation: i64,
    },
    /// `approval.respond` with exactly the four canonical keys.
    RespondApproval {
        /// Prompt key.
        key: PromptKey,
        /// Offered choice.
        choice: String,
    },
    /// `clarify.respond` with exactly the four canonical keys.
    RespondClarify {
        /// Prompt key.
        key: PromptKey,
        /// Answer text, as given.
        answer: String,
    },
    /// The answer matched nothing the reducer will send.
    RefuseAnswer {
        /// Prompt key the caller used.
        key: PromptKey,
        /// First failing check.
        reason: AnswerRefusal,
    },
}
