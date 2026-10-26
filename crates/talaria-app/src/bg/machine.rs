//! The launch state machine. Pure: no input, no clock, no files.
//!
//! Step 0 emits no effects, so a test that expects a sequence sees an empty one.

use std::path::PathBuf;

use crate::bg::outcome::{LaunchError, LaunchOutcome};
use crate::bg::request::LaunchRequest;
use crate::worktree::WorktreeRecord;

/// Identifiers minted for one launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchIds {
    /// Sent on `session.create`.
    pub request_id: String,
    /// Sent on `prompt.submit`.
    pub submission_id: String,
}

/// The handshake summary this launch reads. No describe frame is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescribeSummary {
    /// Session sources the gateway offers.
    pub sources: Vec<String>,
    /// Create parameters the gateway offers.
    pub parameters: Vec<String>,
}

/// A session the gateway confirmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedSession {
    /// The live session id.
    pub session_id: String,
    /// The id a later resume uses, when the gateway sent one.
    pub stored_session_id: Option<String>,
    /// The subscription this connection holds.
    pub subscription_id: String,
    /// `info.cwd` from the snapshot, when present.
    pub info_cwd: Option<String>,
    /// `info.model` from the snapshot, when present.
    pub model: Option<String>,
}

/// A submit the gateway answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmitReply {
    /// Any admission status. The prompt was accepted.
    Admitted {
        /// The admission id.
        admission_id: String,
        /// Copied onto the outcome.
        status: String,
    },
    /// The gateway took the text as a voice stop phrase.
    VoiceStopped,
}

/// A call that did not return the shape this launch wanted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallFailure {
    /// An error reply other than 5001.
    Refused {
        /// The JSON-RPC code.
        code: i64,
        /// `data.reason`, when the reply had one.
        reason: Option<String>,
        /// `data.field` or `data.fields`.
        fields: Vec<String>,
    },
    /// Sent, then dropped, timed out, or answered 5001.
    Unknown,
    /// The frame was never written.
    NotSent(String),
    /// A reply arrived and matched neither shape.
    Malformed(String),
}

/// How the reported folder compares with the folder that was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CwdCheck {
    /// The same folder after canonicalising.
    Matches,
    /// A different folder, or a path that cannot be canonicalised.
    Differs,
    /// The snapshot named no folder.
    Missing,
}

/// Which call a failure belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The handshake summary.
    Describe,
    /// `session.create`.
    Create,
    /// `prompt.submit`.
    Submit,
    /// `session.detach`.
    Detach,
}

/// One result fed back into the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchInput {
    /// The handshake summary.
    Described(DescribeSummary),
    /// The worktree exists and is recorded.
    WorktreeReady(WorktreeRecord),
    /// Preparing the worktree failed.
    WorktreeFailed(LaunchError),
    /// The gateway created the session.
    Created(CreatedSession),
    /// The folder check finished.
    CwdChecked(CwdCheck),
    /// The session id was written, or the warning says why it was not.
    SessionRecorded { warning: Option<String> },
    /// The gateway answered the submit.
    Submitted(SubmitReply),
    /// A call failed.
    Failed { step: Step, failure: CallFailure },
    /// Detach finished or its failure was ignored.
    Detached,
    /// Rollback finished. `kept` is why the guard left the tree in place.
    RolledBack { kept: Option<String> },
    /// The operator interrupted, or Talaria is quitting.
    Cancel,
}

/// What the service should do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchEffect {
    /// Read the handshake summary. No frame is sent.
    Describe,
    /// Create the worktree.
    PrepareWorktree { folder: PathBuf, slug: String },
    /// Send `session.create`.
    Create { new: NewSessionSpec },
    /// Compare the sent folder with `info.cwd`.
    CheckCwd {
        sent: PathBuf,
        reported: Option<String>,
    },
    /// Write the session id into the git record.
    RecordSession {
        record: WorktreeRecord,
        session_id: String,
    },
    /// Send `prompt.submit`.
    Submit {
        session_id: String,
        text: String,
        submission_id: String,
    },
    /// Send `session.detach`. `end_reason` is set only when nothing was admitted.
    Detach {
        session_id: String,
        subscription_id: String,
        end_reason: Option<&'static str>,
    },
    /// Remove the tree this launch created.
    Rollback(WorktreeRecord),
    /// Tell the fleet the session is running.
    FleetRunning { session_id: String },
    /// Tell the fleet the launch has let go.
    FleetReleased { session_id: String },
    /// The launch is over.
    Finish(LaunchOutcome),
}

/// The create parameters, before the wire builder adds the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSessionSpec {
    /// Idempotency key for the one retry.
    pub request_id: String,
    /// The worktree root, or the launch folder.
    pub cwd: PathBuf,
    /// Sent only when `Some`.
    pub model: Option<String>,
    /// Sent only when `Some`.
    pub provider: Option<String>,
    /// The effort, sent as `reasoning` only when `Some`.
    pub reasoning: Option<String>,
}

/// The machine. Step 0 keeps the request and emits nothing.
#[derive(Clone, Debug)]
pub struct LaunchMachine {
    request: LaunchRequest,
    ids: LaunchIds,
}

/// Detach reason used when nothing was admitted.
pub const END_REASON_ABANDONED: &str = "launch_abandoned";

impl LaunchMachine {
    /// Begin a launch. Step 0 returns no effects.
    #[must_use]
    pub fn start(request: LaunchRequest, ids: LaunchIds) -> (Self, Vec<LaunchEffect>) {
        (Self { request, ids }, Vec::new())
    }

    /// Apply one input. Step 0 returns no effects.
    pub fn step(&mut self, input: LaunchInput) -> Vec<LaunchEffect> {
        let _ = input;
        Vec::new()
    }

    /// True from the step that emitted `Submit`. Step 0 never emits one.
    #[must_use]
    pub fn submit_sent(&self) -> bool {
        let _ = (&self.request, &self.ids);
        false
    }
}
