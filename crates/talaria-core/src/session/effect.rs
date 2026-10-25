//! Effects are data. The application sends them. The reducer does not.

use crate::attachments::Attachment;

use super::{AnswerRefusal, PromptKey, SessionId};
use crate::side::RequestId;

/// A follow-up the application may send. Comparing these to recorded outbound frames is the test.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `prompt.submit` with exactly `session_id`, `text` and `submission_id`.
    /// `unattended` is never set true. `attachments` is added only when it is not empty.
    SendPrompt {
        /// Target session.
        session_id: SessionId,
        /// Submitted text. Image labels stay in it.
        text: String,
        /// Client-chosen id.
        submission_id: String,
        /// Ready images. Omitted on the wire when empty.
        attachments: Vec<Attachment>,
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
    /// `shell.exec` with exactly `session_id` and `command`.
    RunShell {
        /// Target session.
        session_id: SessionId,
        /// Id the reducer allocated.
        request_id: RequestId,
        /// Command text.
        command: String,
    },
    /// `prompt.btw` with exactly `session_id` and `text`.
    AskSide {
        /// Target session.
        session_id: SessionId,
        /// Id the reducer allocated.
        request_id: RequestId,
        /// Question text.
        question: String,
    },
}
