//! Effects are data. The application sends them. The reducer does not.

use crate::attachments::Attachment;
use crate::background::BackgroundEffect;

use super::{AnswerRefusal, PromptKey, SessionId};
use crate::queue::{AdmissionId, SubmissionId};
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
    /// `prompt.cancel` for one queued row. Issue 232. Queue and steer.
    CancelQueued {
        /// Target session.
        session_id: SessionId,
        /// Row that must not run.
        admission_id: AdmissionId,
    },
    /// `prompt.replace_queued`. Issue 232. Queue and steer.
    ReplaceQueued {
        /// Target session.
        session_id: SessionId,
        /// Row being retired.
        admission_id: AdmissionId,
        /// Fresh id for the text admitted at the back.
        submission_id: SubmissionId,
        /// Text admitted at the back.
        text: String,
    },
    /// `prompt.steer_queued`. Issue 232. Queue and steer.
    SteerQueued {
        /// Target session.
        session_id: SessionId,
        /// Queued row to steer into the running turn.
        admission_id: AdmissionId,
    },
    /// `session.steer`. Issue 232. Queue and steer.
    Steer {
        /// Target session.
        session_id: SessionId,
        /// Generation the note is aimed at.
        execution_generation: i64,
        /// Note the agent should read.
        text: String,
    },
    /// `prompt.resolve_unknown`. Issue 232. Queue and steer.
    ResolveUnknown {
        /// Target session.
        session_id: SessionId,
        /// Lost row to discard.
        admission_id: AdmissionId,
        /// Generation stored on that row.
        execution_generation: i64,
    },
    /// A background-process or detached-task follow-up.
    Background(BackgroundEffect),
}
